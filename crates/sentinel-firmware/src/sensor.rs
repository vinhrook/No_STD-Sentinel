use core::error::Error;
use core::fmt;
use embassy_time::{Duration, Timer};
use esp_hal::delay::Delay;
use esp_hal::gpio::Flex;
use esp_println::println;


use sentinel_core::crc;
use crate::onewire::{OneWire, OneWireError};

const CMD_SKIP_ROM: u8 = 0xCC;
const CMD_CONVERT_T: u8 = 0x44;
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
        write!(f, "{whole}.{frac:02} °C")
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
            Self::DisconnectedDuringConversion => write!(f, "sensor disconnected during conversion"),
            Self::CrcCorrupted => write!(f, "scratchpad CRC-8 validation failed"),
            Self::PowerOnResetState => write!(f, "uncalibrated power-on default detected (+85°C)"),
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

    // 7. Verify CRC-8 using `sentinel_core::crc`
    if !crc::validate_ds18b20_scratchpad(&scratchpad) {
        return Err(Ds18b20Error::CrcCorrupted);
    }

    // 8. Detect uncalibrated power-on default state (+85.0°C)
    let raw_temp = u16::from_le_bytes([scratchpad[0], scratchpad[1]]);
    if raw_temp == POWER_ON_DEFAULT_RAW {
         return Err(Ds18b20Error::PowerOnResetState);
    }
    
    // 9. Extract and convert to centi-celsius using `sentinel_core::crc`
    let centi = crc::parse_ds18b20_temp(&scratchpad)
            .ok_or(Ds18b20Error::DecodeFailed)?;
    Ok(CentiCelsius(centi))
}
    
            