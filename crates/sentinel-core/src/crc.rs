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
pub const fn crc8_maxim(data: &[u8]) -> u8 {
    Crc8Maxim::digest(data)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crc16Modbus {
    state: u16,
}

impl Default for Crc16Modbus {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Crc16Modbus {
    pub const INITIAL: u16 = 0xFFFF;

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
            let table_idx = ((self.state ^ (data[i] as u16)) & 0x00FF) as usize;
            self.state = (self.state >> 8) ^ CRC16_MODBUS_TABLE[table_idx];
            i += 1;
        }
        self
    }

    #[inline]
    #[must_use]
    pub const fn finalize(self) -> u16 {
        self.state
    }

    #[inline]
    #[must_use]
    pub const fn digest(data: &[u8]) -> u16 {
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
    CrcMismatch { calculated: u8, stored: u8 },
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
            Self::BusFloatingAllOnes => write!(f, "1-Wire bus floating high (disconnected/open)"),
            Self::BusGroundedAllZeros => write!(f, "1-Wire bus shorted to ground"),
        }
    }
}

impl core::error::Error for Ds18b20Error {}

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
    pub const fn as_bytes(&self) -> &[u8; 9] {
        &self.0
    }

    #[inline]
    #[must_use]
    pub const fn into_bytes(self) -> [u8; 9] {
        self.0
    }

    #[inline]
    #[must_use]
    pub const fn raw_temp(&self) -> i16 {
        i16::from_le_bytes([self.0[0], self.0[1]])
    }

    #[inline]
    #[must_use]
    pub const fn stored_crc(&self) -> u8 {
        self.0[8]
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
    pub const fn is_bus_grounded(&self) -> bool {
        let mut i = 0;
        while i < 9 {
            if self.0[i] != 0x00 {
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
    pub const fn to_centi_c(&self) -> Result<TempCentiC, Ds18b20Error> {
        if self.is_bus_floating() {
            return Err(Ds18b20Error::BusFloatingAllOnes);
        }
        if self.is_bus_grounded() {
            return Err(Ds18b20Error::BusGroundedAllZeros);
        }

        let calc_crc = self.calculate_crc();
        let stored = self.stored_crc();
        if calc_crc != stored {
            return Err(Ds18b20Error::CrcMismatch {
                calculated: calc_crc,
                stored,
            });
        }

        let mask: i16 = match self.resolution_bits() {
            9 => !0b0000_0111,
            10 => !0b0000_0011,
            11 => !0b0000_0001,
            _ => !0,
        };
        let raw = self.raw_temp() & mask;
        let centi = (raw as i32 * 25) / 4;
        Ok(TempCentiC::from_centi(centi as i16))
    }
}

#[inline]
#[must_use]
pub const fn validate_ds18b20_scratchpad(scratchpad: &[u8; 9]) -> bool {
    let sp = Ds18b20Scratchpad::new(*scratchpad);
    if sp.is_bus_floating() || sp.is_bus_grounded() {
        return false;
    }
    sp.is_valid_crc()
}

#[inline]
#[must_use]
pub const fn parse_ds18b20_temp(scratchpad: &[u8; 9]) -> Option<i16> {
    match Ds18b20Scratchpad::new(*scratchpad).to_centi_c() {
        Ok(temp) => Some(temp.as_centi()),
        Err(_) => None,
    }
}

#[inline]
pub const fn parse_ds18b20_reading(
    scratchpad: &[u8; 9],
) -> Result<TempCentiC, Ds18b20Error> {
    Ds18b20Scratchpad::new(*scratchpad).to_centi_c()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc8_known_vector() {
        assert_eq!(crc8_maxim(&[0x02]), 0xBC);
        assert_eq!(crc8_maxim(b"123456789"), 0xA1);
    }

    #[test]
    fn crc8_streaming_matches_oneshot() {
        let data = [0x50, 0x05, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10];
        let oneshot = crc8_maxim(&data);
        let streamed = Crc8Maxim::new()
            .update(&data[..4])
            .update(&data[4..])
            .finalize();
        assert_eq!(oneshot, streamed);
    }

    #[test]
    fn ds18b20_scratchpad_85c_power_on_nominal() {
        let sp: [u8; 9] = [0x50, 0x05, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x1C];
        let pad = Ds18b20Scratchpad::new(sp);

        assert!(validate_ds18b20_scratchpad(&sp));
        assert!(pad.is_valid_crc());
        assert!(pad.is_power_on_reset_value());
        assert_eq!(pad.resolution_bits(), 12);
        assert_eq!(parse_ds18b20_temp(&sp), Some(8500));
        assert_eq!(
            parse_ds18b20_reading(&sp),
            Ok(TempCentiC::from_centi(8500))
        );
    }

    #[test]
    fn corrupted_scratchpad_rejected() {
        let bad: [u8; 9] = [0x51, 0x05, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x1C];
        assert!(!validate_ds18b20_scratchpad(&bad));
        assert_eq!(parse_ds18b20_temp(&bad), None);

        let err = parse_ds18b20_reading(&bad).unwrap_err();
        match err {
            Ds18b20Error::CrcMismatch { calculated, stored } => {
                assert_eq!(stored, 0x1C);
                assert_ne!(calculated, 0x1C);
            }
            _ => panic!("Expected CrcMismatch error, got {err:?}"),
        }
    }

    #[test]
    fn floating_bus_all_ones_rejected() {
        let floating = [0xFF; 9];
        assert!(!validate_ds18b20_scratchpad(&floating));
        assert_eq!(parse_ds18b20_temp(&floating), None);
        assert_eq!(
            parse_ds18b20_reading(&floating),
            Err(Ds18b20Error::BusFloatingAllOnes)
        );
    }

    #[test]
    fn grounded_bus_all_zeros_rejected() {
        let grounded = [0x00; 9];
        assert!(!validate_ds18b20_scratchpad(&grounded));
        assert_eq!(parse_ds18b20_temp(&grounded), None);
        assert_eq!(
            parse_ds18b20_reading(&grounded),
            Err(Ds18b20Error::BusGroundedAllZeros)
        );
    }

    #[test]
    fn low_resolution_undefined_bits_masked() {
        let mut sp = [0x57, 0x05, 0x4B, 0x46, 0x1F, 0xFF, 0x0C, 0x10, 0x00];
        sp[8] = crc8_maxim(&sp[0..8]);

        let pad = Ds18b20Scratchpad::new(sp);
        assert_eq!(pad.resolution_bits(), 9);
        assert_eq!(pad.to_centi_c(), Ok(TempCentiC::from_centi(8500)));
    }

    #[test]
    fn crc16_known_vector() {
        let data = b"123456789";
        assert_eq!(crc16_modbus(data), 0x4B37);
    }

    #[test]
    fn crc16_streaming_matches_oneshot() {
        let data = b"sentinel/telemetry/payload_sample_data_test";
        let oneshot = crc16_modbus(data);
        let streamed = Crc16Modbus::new()
            .update(&data[..10])
            .update(&data[10..25])
            .update(&data[25..])
            .finalize();
        assert_eq!(oneshot, streamed);
    }

    #[test]
    fn freezer_temp_conversion_subzero() {
        let raw = (-296_i16) as u16;
        let [lsb, msb] = raw.to_le_bytes();
        let mut scratchpad = [0u8; 9];
        scratchpad[0] = lsb;
        scratchpad[1] = msb;
        scratchpad[4] = 0x7F;
        scratchpad[5] = 0xFF;
        scratchpad[6] = 0x0C;
        scratchpad[7] = 0x10;
        scratchpad[8] = crc8_maxim(&scratchpad[0..8]);

        assert!(validate_ds18b20_scratchpad(&scratchpad));
        assert_eq!(parse_ds18b20_temp(&scratchpad), Some(-1850));
    }

    #[test]
    fn extreme_temp_boundary_conversions() {
        let raw_min = (-880_i16) as u16;
        let [lsb, msb] = raw_min.to_le_bytes();
        let mut sp_min = [0u8; 9];
        sp_min[0] = lsb;
        sp_min[1] = msb;
        sp_min[4] = 0x7F;
        sp_min[5] = 0xFF;
        sp_min[6] = 0x0C;
        sp_min[7] = 0x10;
        sp_min[8] = crc8_maxim(&sp_min[0..8]);
        assert_eq!(parse_ds18b20_temp(&sp_min), Some(-5500));

        let raw_max = 2000_i16 as u16;
        let [lsb, msb] = raw_max.to_le_bytes();
        let mut sp_max = [0u8; 9];
        sp_max[0] = lsb;
        sp_max[1] = msb;
        sp_max[4] = 0x7F;
        sp_max[5] = 0xFF;
        sp_max[6] = 0x0C;
        sp_max[7] = 0x10;
        sp_max[8] = crc8_maxim(&sp_max[0..8]);
        assert_eq!(parse_ds18b20_temp(&sp_max), Some(12500));
    }

    #[test]
    const fn const_eval_verification() {
        const C8: u8 = crc8_maxim(&[0x02]);
        assert!(C8 == 0xBC);

        const C16: u16 = crc16_modbus(b"123456789");
        assert!(C16 == 0x4B37);

        const SP: [u8; 9] = [0x50, 0x05, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x1C];
        const VALID: bool = validate_ds18b20_scratchpad(&SP);
        assert!(VALID);

        const TEMP: Option<i16> = parse_ds18b20_temp(&SP);
        assert!(matches!(TEMP, Some(8500)));
    }
}
