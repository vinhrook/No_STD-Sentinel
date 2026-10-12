#![no_std]
#![cfg_attr(target_arch = "xtensa", feature(impl_trait_in_assoc_type))]

#[cfg(target_arch = "xtensa")]
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, channel::Channel};
use sentinel_core::telemetry::{DeviceId, FreezerReading};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MqttPayload {
    Telemetry {
        reading: FreezerReading,
        ambient_cc: Option<i16>,
    },
    Log {
        device_id: DeviceId,
        message: heapless::String<128>,
    },
}

#[cfg(target_arch = "xtensa")]
pub static TELEMETRY_CHANNEL: Channel<CriticalSectionRawMutex, MqttPayload, 16> = Channel::new();

pub mod app_desc;
pub mod config;
pub mod onewire;
pub mod publisher;
pub mod sensor;
pub mod ota;