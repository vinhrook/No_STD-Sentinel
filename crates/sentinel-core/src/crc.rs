#![forbid(unsafe_code)]

use crate::telemetry::TempCentiC;

#[must_use]
pub const fn make_crc8_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut i = 0usize;
    while i < 256 {
        let mut curr = i as u8;
        let mut j = 0;
        while j < 8 {
            if curr & 1 != 0 {
                curr = (curr >> 1) ^ 0x8C;
            } else {
                curr >>= 1;
            }
            j += 1;
        }
        table[i] = curr;
        i += 1;
    }
    table
}

#[must_use]
pub const fn make_crc16_table() -> [u16; 256] {
    let mut table = [0u16; 256];
    let mut i = 0usize;
    while i < 256 { 
        let mut curr = i as u16;
        let mut j = 0;
        while j < 8 {
            if curr & 1 != 0 {
                curr = (curr >> 1) ^ 0xA001;
            } else {
                curr >>= 1;
            }
            j += 1;
        }
        table[i] = curr;
        i += 1;
    }
    table
}

pub const CRC8_MAXIM_TABLE: [u8; 256] = make_crc8_table();
pub const CRC16_MODBUS_TABLE: [u16; 256] = make_crc16_table();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Crc8Maxim {
    state: u8,
}

impl Crc8Maxim {
    pub const INITIAL: u8 = 0x00;
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: Self::INITIAL,
        }
    }

    #[inline]
    pub const fn update(mut self, data: &[u8]) -> Self {
        let mut i = 0;
        while i < data.len() {
            let index = (self.state ^ data[i]) as usize;
            self.state = CRC8_MAXIM_TABLE[index];
            i += 1;
        }
        self
    }

    #[inline]
    #[must_use]
    pub const fn finalize(self) -> u8 {
        self.state
    }

    #[inline]
    #[must_use]
    pub const fn digest(data: &[u8]) -> u8 {
        Self::new().update(data).finalize()
    }
}

#[inline]
#[must_use]
pub const fn crc16_modbus(data: &[u8]) -> u16 {
    Crc16Modbus::digest(data)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ds18b20Error {
    CrcMismatch {
        calculated: u8,
        stored: u8,
    }, 
    BusFloatingAllOnes,
    BusGroundedAllZeros,
}

impl core::fmt::Display for Ds18b20Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::CrcMismatch { calculated, stored } => {
                write!(
                    f,
                    "DS18B20 CRC mismatch: calculated 0x{calculated:02X}, stored 0x{stored:02X}"
                )
            }
            Self::BusFloatingAllOnce => write!(f, "1-wire bus floating high (disconnected/open)"),
            Self::BusgroundedAllZeros => write!(f, "1-wire bus shorted to ground"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Ds18b20Scratchpad([u8; 9]);

impl Ds18b20Scratchpad {
    pub const POWER_ON_RESET_RAW: i16 = 0x0550;

    #[inline]
    #[must_use]
    pub const fn new(bytes: [u8; 9]) -> Self {
        Self(bytes)
    }

    #[inline]
    #[must_use]
    pub const fn raw_temp(&self) -> i16 {
        i16::from_le_bytes([self.0[0], self.0[1]])
    }

    #[inline]
    #[must_use]
    pub const fn calculate_crc(&self) -> u8 {
        let mut data = [0u8; 8];
        let mut i = 0;
        while i < 8 {
            data[i] = self.0[i];
            i += 1;
        }
        crc8_maxim(&data)
    }

    #[inline]
    #[must_use]
    pub const fn is_valid_crc(&self) -> bool {
        crc8_maxim(&self.0) == 0x00
    }

    #[inline]
    #[must_use]
    pub const fn is_bus_floating(&self) -> bool {
        let mut i = 0;
        while i < 9 {
            if self.0[i] != 0xFF {
                return false;
            }
            i += 1;
        }
        true
    }

    #[inline]
    #[must_use]
    pub const fn is_power_on_reset_value(&self) -> bool {
        self.raw_temp() == Self::POWER_ON_RESET_RAW
    }

    #[inline]
    #[must_use]
    pub const fn resolution_bits(&self) -> u8 {
        match (self.0[4] >> 5) & 0x03 {
            0b00 => 9,
            0b01 => 10,
            0b10 => 11,
            _ => 12,
        }
    }

    #[inline]

}
