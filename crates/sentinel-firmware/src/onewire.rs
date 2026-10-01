//! High-Precision 1-Wire Protocol Driver (`#![no_std]`)
#[cfg(target_arch = "xtensa")]
use esp_hal::delay::Delay;
#[cfg(target_arch = "xtensa")]
use esp_hal::gpio::{Flex, Pull};

// 1. Centralized calibration constants (zero runtime cost)
const RESET_LOW: u32 = 480;
const RESET_SAMPLE: u32 = 70;
const RESET_REST: u32 = 410;
const W1_LOW: u32 = 6;
const W1_REST: u32 = 64;
const W0_LOW: u32 = 60;
const W0_REST: u32 = 10;
const R_LOW: u32 = 2;
const R_SAMPLE: u32 = 10;
const R_REST: u32 = 53;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OneWireError {
    BusStuckLow,
    NoDevice,
    CrcMismatch,
}

#[cfg(target_arch = "xtensa")]
pub struct OneWire<'d> {
    pin: Flex<'d>,
    delay: Delay,
}

#[cfg(target_arch = "xtensa")]
impl<'d> OneWire<'d> {
    pub fn new(mut pin: Flex<'d>, delay: Delay) -> Self {
        pin.set_as_input(Pull::Up);
        Self { pin, delay }
    }

    #[inline(always)] fn low(&mut self) { self.pin.set_as_output(); self.pin.set_low(); }
    #[inline(always)] fn float(&mut self) { self.pin.set_as_input(Pull::Up); }

    /// Reset the bus and confirm slave presence.
    /// Returns `Ok(())` on valid presence pulse, or `Err` if bus is faulty/empty.
    pub fn reset(&mut self) -> Result<(), OneWireError> {
        self.float();
        if self.pin.is_low() { return Err(OneWireError::BusStuckLow); }

        self.low();
        self.delay.delay_micros(RESET_LOW);

        self.float();
        self.delay.delay_micros(RESET_SAMPLE);
        let device_present = self.pin.is_low();
        self.delay.delay_micros(RESET_REST);

        if self.pin.is_low() { return Err(OneWireError::BusStuckLow); }
        if !device_present { return Err(OneWireError::NoDevice); }

        Ok(())
    }

    #[inline(always)]
    pub fn write_bit(&mut self, bit: bool) {
        let delay = self.delay;
        critical_section::with(|_| {
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
        });
    }

    #[inline(always)]
    pub fn read_bit(&mut self) -> bool {
        let delay = self.delay;
        critical_section::with(|_| {
            self.low();
            delay.delay_micros(R_LOW);
            self.float();
            delay.delay_micros(R_SAMPLE);
            let bit = self.pin.is_high();
            delay.delay_micros(R_REST);
            bit
        })
    }

    pub fn write_byte(&mut self, mut byte: u8) {
        for _ in 0..8 {
            self.write_bit((byte & 1) != 0);
            byte >>= 1;
        }
    }

    pub fn read_byte(&mut self) -> u8 {
        (0..8).fold(0, |acc, i| acc | ((self.read_bit() as u8) << i))
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

    pub fn release(self) -> (Flex<'d>, Delay) { (self.pin, self.delay) }
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
        
        // Simulate DS18B20: When master releases after reset low (480µs),
        // wait 30µs, then slave pulls low for 100µs (presence pulse)
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
        // Official Maxim DS18B20 Power-On Reset (+85°C) factory scratchpad
        let scratchpad = [0x50, 0x05, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x1C];
        assert_eq!(crc8(&scratchpad[..8]), 0x1C);
        assert!(check_crc(&scratchpad).is_ok());
    }
}
