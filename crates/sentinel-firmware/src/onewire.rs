use esp_hal::delay::Delay;
use esp_hal::gpio::{Flex, Pull};

const RESET_LOW: u32 = 480;
const RESET_SAMPLE: u32 = 70;
const RESET_REST: u32 = 6;
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

    pub fn reset(&mut self) -> Result<(), OneWireError> {
        self.float();
        if self.pin.is_low() { return Err(OneWireError::BusStuckLow); }

        self.low();
        self.delay.delay_micros(RESET_LOW);

        self.float();
        self.delay.delay_micros(RESET_SAMPLE);
        let device_present = self.pin.is_low();

        if self.pin.is_low() { return Err(OneWireError::BusStuckLow); }
        if !device_present { return Err(OneWireError::NoDevice); }

        Ok(())
    }

    [inline(always)]
    pub fn write_bit(&mut self, bit: bool) {
        let delay = self.delay;
        critical_section::with(|_| {
            self.low();
            if bit {
                delay.delay_micros(W1_LOW);
                self.float();
                delay.delay_micros(W1_REST);
            } else {
                delay.delay_micros(W0_REST);
                self.float();
                delay.delay_micros(W0_REST)
            }
        });
    }
}