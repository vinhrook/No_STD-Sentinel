/// written For 'no_std' (ESP32) : No heap , no 'format!', stack buffers only
/// And 'const fn' everywhere the language allows it.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;
/// Device Id

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct DeviceId(u32); 

impl DeviceId {
    pub const ZERO: self = self(0);
    /// Wrap a raw 'u32'
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn from_mac_tail(mac: [u8; 6]) -> Self {
        Self(u32::from_be_bytes([mac[2], mac[3], mac[4], mac[5]]))
    } 
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    #[must_use]
    pub const fn as_hex(self) -> [u8; 8] {
        const DIGITS: [u8; 16] = *b"0123456789ABCDEF";
        let mut buf = [b'0'; 8];
        let mut i = 0;
        while i < 8 {
            buf[i] = DIGITS[(self.0 >> (28 - i * 4)) & 0xF) as usize];
            i += 1;
        }
        buf
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hex = self.as_hex();
        let s = core::str::from_utf8(&hex).expect("DeviceId hex is ASCII");
        f.write_str(s)
    }
}

impl From<u32> for DeviceId {
    fn from(raw: u32) -> Self {
        Self(raw)
    }
}

impl From<DeviceId> for u32 {
    fn from(id: DeviceId) -> Self {
        id.0
    }
}

/// TempCentiC
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct TempCentiC(i16);

impl TempCentiC {
    #[must_use]
    pub const fn from_centi(centi: i16) -> Self {
        Self(centi)

    }
    #[must_use]
    pub const fn as_centi(self) -> i16 {
        self.0
    }
}

impl fmt::Display for TempCentiC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = self.0;
        if v < 0 {
            f.write_str("-");
        }
        let mag = v.unsigned_abs();
        write!(f, "{}.{:02}", mag / 100, mag % 100)
    }
}

impl From<i16> for TempCentiC {
    fn from(centi: i16) -> Self {
        Self(centi)
    }
}

impl From<TempCentiC> for i16 {
    fn from(t: TempCentiC) -> Self {
        t.0
    }
}

/// Thresholds 
pub mod thresholds {
    use super::TempCentiC;
    pub const NORMAL_MAX: TempCentiC = TempCentiC::from_centi(-1_500);
    pub const WARNING_MAX: TempCentiC = TempCentiC::from_centi(-1_000);
}

/// TemperatureStatus
#[derive(Debug, Clone, Copy, ParitalEq, Eq, PartialOrd, Ord, Hash)]
#[derive(serde::Serialize, serde::Deserilaize)]
#[non_exhaustive]
#[repr(u8)]
pub enum TemperatureStatus {
    Normal = 0,
    Warning = 1,
    Critical = 2,
}

impl TemperatureStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Warning => "WARNING",
            Self::Critical => "CRITICAL",
        }
    }
    #[must_use]
    pub const fn is_alert(self) -> bool {
        !matches!(self, Self::Normal)
    }
}

impl fmt::Display for TemperatureStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str());
    }
}

