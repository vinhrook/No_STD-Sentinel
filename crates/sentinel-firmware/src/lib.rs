//! High-Precision 1-Wire Protocol Driver (`#![no_std]`)
use esp_hal::delay::Delay;
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

pub struct OneWire<'d> {
    pin: Flex<'d>,
    delay: Delay,
}

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
