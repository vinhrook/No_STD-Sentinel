use core::error::Error;
use core::fmt;

#[cfg(target_arch = "xtensa")]
use {
    crate::onewire::OneWire,
    embassy_time::{Duration, Timer},
    esp_hal::delay::Delay,
    esp_hal::gpio::Flex,
    esp_println::println,
};

#[allow(unused_imports)]
use crate::onewire::{
    OneWireError, RomAddress, CMD_MATCH_ROM, CMD_READ_ROM, CMD_SEARCH_ROM, CMD_SKIP_ROM,
};
use sentinel_core::crc;

#[allow(dead_code)]
const CMD_CONVERT_T: u8 = 0x44;
#[allow(dead_code)]
const CMD_READ_SCRATCHPAD: u8 = 0xBE;

const POWER_ON_DEFAULT_RAW: u16 = 0x0550;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DualSensorReading {
    pub internal: Result<CentiCelsius, Ds18b20Error>,
    pub ambient: Option<Result<CentiCelsius, Ds18b20Error>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct CentiCelsius(pub i16);

impl CentiCelsius {
    #[inline(always)]
    pub const fn raw(self) -> i16 {
        self.0
    }

    #[inline(always)]
    pub fn to_celsius(self) -> f32 {
        self.0 as f32 / 100.0
    }
}

impl fmt::Display for CentiCelsius {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let whole = self.0 / 100;
        let frac = (self.0 % 100).abs();
        if self.0 < 0 && whole == 0 {
            write!(f, "-0.{frac:02} °C")
        } else {
            write!(f, "{whole}.{frac:02} °C")
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ds18b20Error {
    BusFault(OneWireError),
    PresenceFailed,
    DisconnectedDuringConversion,
    CrcCorrupted,
    PowerOnResetState,
    DecodeFailed,
}

impl fmt::Display for Ds18b20Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BusFault(err) => write!(f, "bus fault: {err:?}"),
            Self::PresenceFailed => write!(f, "sensor presence pulse missing"),
            Self::DisconnectedDuringConversion => {
                write!(f, "sensor disconnected during conversion")
            }
            Self::CrcCorrupted => write!(f, "scratchpad CRC-8 validation failed"),
            Self::PowerOnResetState => {
                write!(f, "uncalibrated power-on default detected (+85°C)")
            }
            Self::DecodeFailed => write!(f, "failed to decode scratchpad bytes to temperature"),
        }
    }
}

impl Error for Ds18b20Error {}

impl From<OneWireError> for Ds18b20Error {
    fn from(err: OneWireError) -> Self {
        Self::BusFault(err)
    }
}

pub fn parse_scratchpad(scratchpad: &[u8; 9]) -> Result<CentiCelsius, Ds18b20Error> {
    if !crc::validate_ds18b20_scratchpad(scratchpad) {
        return Err(Ds18b20Error::CrcCorrupted);
    }

    let raw_temp = u16::from_le_bytes([scratchpad[0], scratchpad[1]]);
    if raw_temp == POWER_ON_DEFAULT_RAW {
        return Err(Ds18b20Error::PowerOnResetState);
    }

    let centi = crc::parse_ds18b20_temp(scratchpad).ok_or(Ds18b20Error::DecodeFailed)?;
    Ok(CentiCelsius(centi))
}

#[cfg(target_arch = "xtensa")]
pub async fn read_temperature_async(
    pin: &mut Flex<'_>,
    max_retries: u8,
) -> Result<CentiCelsius, Ds18b20Error> {
    let mut last_error = Ds18b20Error::PresenceFailed;
    for attempt in 1..=max_retries {
        match execute_conversion_cycle(pin).await {
            Ok(temp) => return Ok(temp),
            Err(err) => {
                println!("[DS18B20] Attempt {attempt}/{max_retries} failed: {err}");
                last_error = err;
                if attempt < max_retries {
                    let mut bus = OneWire::new(pin, Delay::new());
                    let _ = bus.recover();
                    Timer::after(Duration::from_millis(100)).await;
                }
            }
        }
    }
    Err(last_error)
}

#[cfg(target_arch = "xtensa")]
async fn execute_conversion_cycle(pin: &mut Flex<'_>) -> Result<CentiCelsius, Ds18b20Error> {
    {
        let mut bus = OneWire::new(pin, Delay::new());
        match bus.reset() {
            Ok(()) => {}
            Err(OneWireError::NoDevice) => return Err(Ds18b20Error::PresenceFailed),
            Err(err) => return Err(Ds18b20Error::BusFault(err)),
        }
        bus.write_byte(CMD_SKIP_ROM);
        bus.write_byte(CMD_CONVERT_T);
    }

    Timer::after(Duration::from_millis(750)).await;
    let mut scratchpad = [0u8; 9];
    {
        let mut bus = OneWire::new(pin, Delay::new());

        match bus.reset() {
            Ok(()) => {}
            Err(OneWireError::NoDevice) => return Err(Ds18b20Error::DisconnectedDuringConversion),
            Err(err) => return Err(Ds18b20Error::BusFault(err)),
        }

        bus.write_byte(CMD_SKIP_ROM);
        bus.write_byte(CMD_READ_SCRATCHPAD);
        bus.read_bytes(&mut scratchpad);
    }

    match parse_scratchpad(&scratchpad) {
        Ok(temp) => Ok(temp),
        Err(e) => {
            println!("[DS18B20] Scratchpad bytes read: {:02X?}", scratchpad);
            Err(e)
        }
    }
}

#[cfg(target_arch = "xtensa")]
pub fn trigger_broadcast_conversion(pin: &mut Flex<'_>) -> Result<(), Ds18b20Error> {
    let mut bus = OneWire::new(pin, Delay::new());
    match bus.reset() {
        Ok(()) => {}
        Err(OneWireError::NoDevice) => return Err(Ds18b20Error::PresenceFailed),
        Err(err) => return Err(Ds18b20Error::BusFault(err)),
    }
    bus.write_byte(CMD_SKIP_ROM);
    bus.write_byte(CMD_CONVERT_T);
    Ok(())
}

#[cfg(target_arch = "xtensa")]
pub fn read_scratchpad_by_rom(
    pin: &mut Flex<'_>,
    rom: &RomAddress,
) -> Result<CentiCelsius, Ds18b20Error> {
    let mut last_err = Ds18b20Error::PresenceFailed;
    for attempt in 0..3 {
        if attempt > 0 {
            let mut bus = OneWire::new(pin, Delay::new());
            let _ = bus.recover();
        }
        let mut scratchpad = [0u8; 9];
        {
            let mut bus = OneWire::new(pin, Delay::new());
            match bus.reset() {
                Ok(()) => {}
                Err(OneWireError::NoDevice) => {
                    last_err = Ds18b20Error::DisconnectedDuringConversion;
                    continue;
                }
                Err(err) => {
                    last_err = Ds18b20Error::BusFault(err);
                    continue;
                }
            }
            bus.write_byte(CMD_MATCH_ROM);
            bus.write_bytes(rom);
            bus.write_byte(CMD_READ_SCRATCHPAD);
            bus.read_bytes(&mut scratchpad);
        }
        match parse_scratchpad(&scratchpad) {
            Ok(temp) => return Ok(temp),
            Err(e) => {
                last_err = e;
            }
        }
    }
    Err(last_err)
}

pub const DEFAULT_HARDWARE_ROMS: [RomAddress; 2] = [
    [0x28, 0xCA, 0x4A, 0x23, 0x00, 0x00, 0x00, 0xE2],
    [0x28, 0xA1, 0x18, 0x23, 0x00, 0x00, 0x00, 0xCF],
];

#[cfg(target_arch = "xtensa")]
pub async fn read_dual_temperatures_async(
    pin: &mut Flex<'_>,
    roms: &[RomAddress],
    max_retries: u8,
) -> DualSensorReading {
    let effective_roms: &[RomAddress] = if roms.is_empty() {
        &DEFAULT_HARDWARE_ROMS
    } else {
        roms
    };

    if effective_roms.len() == 1 {
        let single = read_single_by_rom_async(pin, &effective_roms[0], max_retries).await;
        return DualSensorReading {
            internal: single,
            ambient: None,
        };
    }

    let mut best_internal = Err(Ds18b20Error::PresenceFailed);
    let mut best_ambient = Err(Ds18b20Error::PresenceFailed);

    for attempt in 1..=max_retries {
        if let Err(e) = trigger_broadcast_conversion(pin) {
            println!("[DS18B20] Attempt {attempt}/{max_retries} broadcast conversion failed: {e}");
            if best_internal.is_err() {
                best_internal = Err(e);
            }
            let mut bus = OneWire::new(pin, Delay::new());
            let _ = bus.recover();
            Timer::after(Duration::from_millis(100)).await;
            continue;
        }

        Timer::after(Duration::from_millis(750)).await;

        let r0 = read_scratchpad_by_rom(pin, &effective_roms[0]);
        Timer::after(Duration::from_millis(10)).await;
        let r1 = read_scratchpad_by_rom(pin, &effective_roms[1]);

        if r0.is_ok() {
            best_internal = r0;
        } else if best_internal.is_err() {
            best_internal = r0;
        }

        if r1.is_ok() {
            best_ambient = r1;
        } else if best_ambient.is_err() {
            best_ambient = r1;
        }

        if best_internal.is_ok() || best_ambient.is_ok() {
            return DualSensorReading {
                internal: best_internal,
                ambient: Some(best_ambient),
            };
        } else {
            if attempt == max_retries {
                println!(
                    "[DS18B20] Bus readout failed on attempt {attempt}/{max_retries}: S0={:?}, S1={:?}",
                    best_internal, best_ambient
                );
            }
            if attempt < max_retries {
                let mut bus = OneWire::new(pin, Delay::new());
                let _ = bus.recover();
                Timer::after(Duration::from_millis(50)).await;
            }
        }
    }

    DualSensorReading {
        internal: best_internal,
        ambient: Some(best_ambient),
    }
}

#[cfg(target_arch = "xtensa")]
async fn read_single_by_rom_async(
    pin: &mut Flex<'_>,
    rom: &RomAddress,
    max_retries: u8,
) -> Result<CentiCelsius, Ds18b20Error> {
    let mut last_error = Ds18b20Error::PresenceFailed;
    for attempt in 1..=max_retries {
        if let Err(e) = trigger_broadcast_conversion(pin) {
            last_error = e;
            let mut bus = OneWire::new(pin, Delay::new());
            let _ = bus.recover();
            Timer::after(Duration::from_millis(100)).await;
            continue;
        }
        Timer::after(Duration::from_millis(750)).await;
        match read_scratchpad_by_rom(pin, rom) {
            Ok(temp) => return Ok(temp),
            Err(e) => {
                last_error = e;
                if attempt < max_retries {
                    let mut bus = OneWire::new(pin, Delay::new());
                    let _ = bus.recover();
                    Timer::after(Duration::from_millis(100)).await;
                }
            }
        }
    }
    Err(last_error)
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::format;
    use super::*;

    #[test]
    fn test_centicelsius_math() {
        let temp = CentiCelsius(-1850);
        assert_eq!(temp.raw(), -1850);
        assert_eq!(temp.to_celsius(), -18.5);

        let zero = CentiCelsius(0);
        assert_eq!(zero.raw(), 0);
        assert_eq!(zero.to_celsius(), 0.0);

        let warm = CentiCelsius(3725);
        assert_eq!(warm.raw(), 3725);
        assert_eq!(warm.to_celsius(), 37.25);
    }

    #[test]
    fn test_centicelsius_display_formatting() {
        assert_eq!(alloc::format!("{}", CentiCelsius(2450)), "24.50 °C");
        assert_eq!(alloc::format!("{}", CentiCelsius(-2000)), "-20.00 °C");
        assert_eq!(alloc::format!("{}", CentiCelsius(-50)), "-0.50 °C");
        assert_eq!(alloc::format!("{}", CentiCelsius(-5)), "-0.05 °C");
        assert_eq!(alloc::format!("{}", CentiCelsius(5)), "0.05 °C");
        assert_eq!(alloc::format!("{}", CentiCelsius(0)), "0.00 °C");
    }

    #[test]
    fn test_parse_scratchpad_valid_temperature() {
        let mut scratchpad = [0x91, 0x01, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x00];
        scratchpad[8] = crc::crc8_maxim(&scratchpad[..8]);

        let result = parse_scratchpad(&scratchpad).expect("valid scratchpad should parse");
        assert_eq!(result.raw(), 2506);
    }

    #[test]
    fn test_parse_scratchpad_subzero_temperature() {
        let mut scratchpad = [0x5E, 0xFF, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x00];
        scratchpad[8] = crc::crc8_maxim(&scratchpad[..8]);

        let result = parse_scratchpad(&scratchpad).expect("subzero scratchpad should parse");
        assert_eq!(result.raw(), -1012);
    }

    #[test]
    fn test_parse_scratchpad_power_on_reset_rejected() {
        let scratchpad = [0x50, 0x05, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x1C];
        let err = parse_scratchpad(&scratchpad).unwrap_err();
        assert_eq!(err, Ds18b20Error::PowerOnResetState);
    }

    #[test]
    fn test_parse_scratchpad_crc_corruption() {
        let corrupted = [0x50, 0x05, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0xEE];
        let err = parse_scratchpad(&corrupted).unwrap_err();
        assert_eq!(err, Ds18b20Error::CrcCorrupted);
    }

    #[test]
    fn test_parse_scratchpad_bus_fault_states() {
        let floating = [0xFF; 9];
        assert_eq!(
            parse_scratchpad(&floating).unwrap_err(),
            Ds18b20Error::CrcCorrupted
        );

        let grounded = [0x00; 9];
        assert_eq!(
            parse_scratchpad(&grounded).unwrap_err(),
            Ds18b20Error::CrcCorrupted
        );
    }

    #[test]
    fn test_error_display_and_conversion() {
        assert_eq!(
            alloc::format!("{}", Ds18b20Error::PresenceFailed),
            "sensor presence pulse missing"
        );
        assert_eq!(
            alloc::format!("{}", Ds18b20Error::PowerOnResetState),
            "uncalibrated power-on default detected (+85°C)"
        );

        let bus_err: Ds18b20Error = OneWireError::BusStuckLow.into();
        assert_eq!(bus_err, Ds18b20Error::BusFault(OneWireError::BusStuckLow));
    }
}