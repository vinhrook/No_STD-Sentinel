#![cfg_attr(not(feature = "std"), no_std)]
#![deny(unsafe_code)]
#![warn(missing_docs)]

pub mod alert;
pub mod telemetry;
pub mod health;
pub mod crc;