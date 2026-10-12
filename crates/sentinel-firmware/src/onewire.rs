#[cfg(target_arch = "xtensa")]
use esp_hal::delay::Delay;
#[cfg(target_arch = "xtensa")]
use esp_hal::gpio::{Flex, Pull};

const RESET_LOW: u32 = 480;
const RESET_SAMPLE: u32 = 70;
const RESET_REST: u32 = 410;
const W1_LOW: u32 = 6;
const W1_REST: u32 = 64;
const W0_LOW: u32 = 60;
const W0_REST: u32 = 10;
const R_LOW: u32 = 3;
const R_SAMPLE: u32 = 7;
const R_REST: u32 = 55;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OneWireError {
    BusStuckLow,
    NoDevice,
    CrcMismatch,
}

pub type RomAddress = [u8; 8];

pub const CMD_SEARCH_ROM: u8 = 0xF0;
pub const CMD_READ_ROM: u8 = 0x33;
pub const CMD_MATCH_ROM: u8 = 0x55;
pub const CMD_SKIP_ROM: u8 = 0xCC;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SearchState {
    pub last_discrepancy: u8,
    pub last_family_discrepancy: u8,
    pub last_device_flag: bool,
    pub rom: RomAddress,
}

impl SearchState {
    pub const fn new() -> Self {
        Self {
            last_discrepancy: 0,
            last_family_discrepancy: 0,
            last_device_flag: false,
            rom: [0u8; 8],
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

#[cfg(target_arch = "xtensa")]
pub struct OneWire<'a, 'd> {
    pin: &'a mut Flex<'d>,
    delay: Delay,
}

#[cfg(target_arch = "xtensa")]
impl<'a, 'd> OneWire<'a, 'd> {
    pub fn new(pin: &'a mut Flex<'d>, delay: Delay) -> Self {
        pin.set_as_open_drain(Pull::Up);
        pin.set_high();
        Self { pin, delay }
    }

    #[inline(always)] fn low(&mut self) { self.pin.set_low(); }
    #[inline(always)] fn float(&mut self) { self.pin.set_high(); }

    pub fn reset(&mut self) -> Result<(), OneWireError> {
        let delay = self.delay;
        critical_section::with(|_| {
            self.float();
            if self.pin.is_low() { return Err(OneWireError::BusStuckLow); }

            self.low();
            delay.delay_micros(RESET_LOW);

            self.float();
            delay.delay_micros(RESET_SAMPLE);
            let device_present = self.pin.is_low();
            delay.delay_micros(RESET_REST);

            if self.pin.is_low() { return Err(OneWireError::BusStuckLow); }
            if !device_present { return Err(OneWireError::NoDevice); }

            Ok(())
        })
    }

    #[inline(always)]
    fn write_bit_raw(&mut self, bit: bool) {
        let delay = self.delay;
        self.low();
        if bit {
            delay.delay_micros(W1_LOW);
            self.float();
            delay.delay_micros(W1_REST);
        } else {
            delay.delay_micros(W0_LOW);
            self.float();
            delay.delay_micros(W0_REST);
        }
    }

    #[inline(always)]
    fn read_bit_raw(&mut self) -> bool {
        let delay = self.delay;
        self.low();
        delay.delay_micros(R_LOW);
        self.float();
        delay.delay_micros(R_SAMPLE);
        let bit = self.pin.is_high();
        delay.delay_micros(R_REST);
        bit
    }

    #[inline(always)]
    pub fn write_bit(&mut self, bit: bool) {
        critical_section::with(|_| {
            self.write_bit_raw(bit);
        });
    }

    #[inline(always)]
    pub fn read_bit(&mut self) -> bool {
        critical_section::with(|_| {
            self.read_bit_raw()
        })
    }

    pub fn write_byte(&mut self, mut byte: u8) {
        critical_section::with(|_| {
            for _ in 0..8 {
                self.write_bit_raw((byte & 1) != 0);
                byte >>= 1;
            }
        });
    }

    pub fn read_byte(&mut self) -> u8 {
        critical_section::with(|_| {
            let mut byte = 0u8;
            for i in 0..8 {
                if self.read_bit_raw() {
                    byte |= 1 << i;
                }
            }
            byte
        })
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        bytes.iter().for_each(|&b| self.write_byte(b));
    }

    pub fn read_bytes(&mut self, buf: &mut [u8]) {
        buf.iter_mut().for_each(|b| *b = self.read_byte());
    }

    pub fn recover(&mut self) -> Result<(), OneWireError> {
        self.float();
        self.delay.delay_micros(100);
        for _ in 0..9 {
            if self.pin.is_high() { return Ok(()); }
            self.low();
            self.delay.delay_micros(10);
            self.float();
            self.delay.delay_micros(10);
        }
        if self.pin.is_low() { Err(OneWireError::BusStuckLow) } else { Ok(()) }
    }

    pub fn search(&mut self, state: &mut SearchState) -> Result<Option<RomAddress>, OneWireError> {
        if state.last_device_flag {
            return Ok(None);
        }

        match self.reset() {
            Ok(()) => {}
            Err(OneWireError::NoDevice) => {
                state.reset();
                return Ok(None);
            }
            Err(e) => return Err(e),
        }

        self.write_byte(CMD_SEARCH_ROM);

        let mut id_bit_number: u8 = 1;
        let mut last_zero: u8 = 0;
        let mut rom_byte_number: usize = 0;
        let mut rom_byte_mask: u8 = 1;

        while id_bit_number <= 64 {
            let id_bit = self.read_bit();
            let cmp_id_bit = self.read_bit();

            if id_bit && cmp_id_bit {
                return Err(OneWireError::NoDevice);
            }

            let search_direction = if id_bit != cmp_id_bit {
                id_bit
            } else {
                if id_bit_number == state.last_discrepancy {
                    true
                } else if id_bit_number > state.last_discrepancy {
                    false
                } else {
                    (state.rom[rom_byte_number] & rom_byte_mask) != 0
                }
            };

            if !search_direction && id_bit == cmp_id_bit {
                last_zero = id_bit_number;
                if last_zero < 9 {
                    state.last_family_discrepancy = last_zero;
                }
            }

            if search_direction {
                state.rom[rom_byte_number] |= rom_byte_mask;
            } else {
                state.rom[rom_byte_number] &= !rom_byte_mask;
            }

            self.write_bit(search_direction);

            id_bit_number += 1;
            rom_byte_mask <<= 1;
            if rom_byte_mask == 0 {
                rom_byte_number += 1;
                rom_byte_mask = 1;
            }
        }

        state.last_discrepancy = last_zero;
        if state.last_discrepancy == 0 {
            state.last_device_flag = true;
        }

        if crc8(&state.rom[..7]) != state.rom[7] {
            return Err(OneWireError::CrcMismatch);
        }

        Ok(Some(state.rom))
    }

    pub fn search_all<const MAX: usize>(
        &mut self,
    ) -> Result<heapless::Vec<RomAddress, MAX>, OneWireError> {
        let mut devices = heapless::Vec::new();
        let mut state = SearchState::new();

        while let Some(rom) = self.search(&mut state)? {
            let _ = devices.push(rom);
            if devices.is_full() {
                break;
            }
        }

        Ok(devices)
    }

    pub fn read_rom(&mut self) -> Result<RomAddress, OneWireError> {
        self.reset()?;
        self.write_byte(CMD_READ_ROM);
        let mut rom = [0u8; 8];
        self.read_bytes(&mut rom);
        if crc8(&rom[..7]) != rom[7] {
            return Err(OneWireError::CrcMismatch);
        }
        Ok(rom)
    }

    pub fn release(self) -> (&'a mut Flex<'d>, Delay) { (self.pin, self.delay) }
}

pub const fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    let mut i = 0;
    while i < data.len() {
        let mut byte = data[i];
        let mut bit = 0;
        while bit < 8 {
            crc = if ((crc ^ byte) & 1) != 0 { (crc >> 1) ^ 0x8C } else { crc >> 1 };
            byte >>= 1;
            bit += 1;
        }
        i += 1;
    }
    crc
}

pub fn check_crc(buf: &[u8]) -> Result<(), OneWireError> {
    match buf.split_last() {
        Some((&crc, data)) if crc8(data) == crc => Ok(()),
        _ => Err(OneWireError::CrcMismatch),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    struct Mock1WireBus {
        time_us: u32,
        master_output: bool,
        slave_pulling_low: bool,
    }

    impl Mock1WireBus {
        fn is_pin_low(&self) -> bool {
            !self.master_output || self.slave_pulling_low
        }
        
        fn advance_time(&mut self, us: u32) {
            self.time_us += us;
            if self.time_us >= 510 && self.time_us <= 610 {
                self.slave_pulling_low = true;
            } else {
                self.slave_pulling_low = false;
            }
        }
    }

    #[test]
    fn test_valid_ds18b20_rom_crc() {
        let rom = [0x28, 0x5E, 0x48, 0x56, 0x05, 0x00, 0x00, 0x10];
        assert_eq!(crc8(&rom[..7]), 0x10);
        assert!(check_crc(&rom).is_ok());
    }

    #[test]
    fn test_corrupted_crc() {
        let corrupted_rom = [0x28, 0x5E, 0x48, 0x56, 0x05, 0x00, 0x00, 0xFF];
        assert_eq!(check_crc(&corrupted_rom), Err(OneWireError::CrcMismatch));
    }

    #[test]
    fn test_ds18b20_scratchpad_golden_power_on_crc() {
        let scratchpad = [0x50, 0x05, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x1C];
        assert_eq!(crc8(&scratchpad[..8]), 0x1C);
        assert!(check_crc(&scratchpad).is_ok());
    }

    #[test]
    fn test_search_state_lifecycle() {
        let mut state = SearchState::new();
        assert_eq!(state.last_discrepancy, 0);
        assert!(!state.last_device_flag);
        state.last_discrepancy = 12;
        state.last_device_flag = true;
        state.reset();
        assert_eq!(state.last_discrepancy, 0);
        assert!(!state.last_device_flag);
    }
}
