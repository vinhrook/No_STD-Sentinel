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

use crate::onewire::OneWireError;
use sentinel_core::crc;

#[allow(dead_code)]
const CMD_SKIP_ROM: u8 = 0xCC;
#[allow(dead_code)]
const CMD_CONVERT_T: u8 = 0x44;
#[allow(dead_code)]
const CMD_READ_SCRATCHPAD: u8 = 0xBE;

const POWER_ON_DEFAULT_RAW: u16 = 0x0550;

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

/// Validates and parses a raw 9-byte DS18B20 scratchpad into a verified [`CentiCelsius`] reading.
pub fn parse_scratchpad(scratchpad: &[u8; 9]) -> Result<CentiCelsius, Ds18b20Error> {
    // 1. Verify CRC-8 using `sentinel_core::crc`
    if !crc::validate_ds18b20_scratchpad(scratchpad) {
        return Err(Ds18b20Error::CrcCorrupted);
    }

    // 2. Detect uncalibrated power-on default state (+85.0°C)
    let raw_temp = u16::from_le_bytes([scratchpad[0], scratchpad[1]]);
    if raw_temp == POWER_ON_DEFAULT_RAW {
        return Err(Ds18b20Error::PowerOnResetState);
    }

    // 3. Extract and convert to centi-celsius using `sentinel_core::crc`
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
                    // Recover bus and back off before next attempt
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
    // Phase 1: Initiate Conversion
    {
        let mut bus = OneWire::new(pin, Delay::new());
        // 1. Reset bus & verify sensor presence
        let presence = bus.reset().map_err(Ds18b20Error::BusFault)?;
        if !presence {
            return Err(Ds18b20Error::PresenceFailed);
        }
        // 2. Issue Skip ROM (0xCC) -> Convert T (0x44)
        bus.write_byte(CMD_SKIP_ROM);
        bus.write_byte(CMD_CONVERT_T);
    }

    // 3. Asynchronously yield for 750 ms (12-bit conversion). CPU is 100% free!
    Timer::after(Duration::from_millis(750)).await;
    let mut scratchpad = [0u8; 9];
    {
        let mut bus = OneWire::new(pin, Delay::new());

        // 4. Second reset pulse to ready the sensor for readout
        let presence = bus.reset().map_err(Ds18b20Error::BusFault)?;
        if !presence {
            return Err(Ds18b20Error::DisconnectedDuringConversion);
        }

        // 5. Issue Skip ROM (0xCC) -> Read Scratchpad (0xBE)
        bus.write_byte(CMD_SKIP_ROM);
        bus.write_byte(CMD_READ_SCRATCHPAD);
        // 6. Read 9 scratchpad bytes (8 data + 1 CRC-8)
        bus.read_bytes(&mut scratchpad);
    }

    parse_scratchpad(&scratchpad)
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
        // Cold chain freezer test vectors
        assert_eq!(alloc::format!("{}", CentiCelsius(2450)), "24.50 °C");
        assert_eq!(alloc::format!("{}", CentiCelsius(-2000)), "-20.00 °C");
        assert_eq!(alloc::format!("{}", CentiCelsius(-50)), "-0.50 °C");
        assert_eq!(alloc::format!("{}", CentiCelsius(-5)), "-0.05 °C");
        assert_eq!(alloc::format!("{}", CentiCelsius(5)), "0.05 °C");
        assert_eq!(alloc::format!("{}", CentiCelsius(0)), "0.00 °C");
    }

    #[test]
    fn test_parse_scratchpad_valid_temperature() {
        // Realistic DS18B20 12-bit reading of +25.0625°C:
        // Raw = 25.0625 * 16 = 401 = 0x0191 -> [0x91, 0x01]
        let mut scratchpad = [0x91, 0x01, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x00];
        scratchpad[8] = crc::crc8_maxim(&scratchpad[..8]);

        let result = parse_scratchpad(&scratchpad).expect("valid scratchpad should parse");
        assert_eq!(result.raw(), 2506); // 25.06 °C in centi-celsius
    }

    #[test]
    fn test_parse_scratchpad_subzero_temperature() {
        // -10.125°C reading:
        // Raw = -10.125 * 16 = -162 = 0xFF5E -> [0x5E, 0xFF]
        let mut scratchpad = [0x5E, 0xFF, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x00];
        scratchpad[8] = crc::crc8_maxim(&scratchpad[..8]);

        let result = parse_scratchpad(&scratchpad).expect("subzero scratchpad should parse");
        assert_eq!(result.raw(), -1012); // -10.12 °C in centi-celsius
    }

    #[test]
    fn test_parse_scratchpad_power_on_reset_rejected() {
        // Factory Maxim default uncalibrated state (+85.0°C)
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
        // Bus floating high
        let floating = [0xFF; 9];
        assert_eq!(
            parse_scratchpad(&floating).unwrap_err(),
            Ds18b20Error::CrcCorrupted
        );

        // Bus grounded
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