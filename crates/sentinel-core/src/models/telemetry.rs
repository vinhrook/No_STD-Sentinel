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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseIdError {
    BadDigit(u8),
}

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadDigit(b) => write!(f, "invalid hex digit: {:#04X}", b),
        }
    }
}

impl core::error::Error for ParseIdError {}

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
    pub const fn from_hex(hex: &[u8; 8]) -> Result<Self, ParseIdError> {
        let mut v: u32 = 0;
        let mut i = 0;
        while i < 8 {
            let d = hex[i];
            let n: u32 = match d {
                b'0'..=b'9' => (d - b'0') as u32,
                b'A'..=b'F' => (d - b'A' + 10) as u32,
                b'a'..=b'f' => (d - b'a' + 10) as u32,
                _ => return Err(ParseIdError::BadDigit(d)),
            };
            v = (v << 4) | n;
            i += 1;
        }
        Ok(Self(v))
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
    #[must_use]
    pub const fn const_cmp(self, other: Self) -> core::cmp::Ordering {
        let a = self.0;
        let b = other.0;
        if a < b {
            core::cmp::Ordering::Less
        } else if a > b {
            core::cmp::Ordering::Greater
        } else {
            core::cmp::Ordering::Equal
        }
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


/// HealthScore
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct HealthScore(u16);

impl HealthScore {
    pub const MAX: u16 = 1_000;
    pub const ZERO: Self = Self::new(0);
    #[must_use]
    pub const fn new(score: u16) -> Self {
        assert!(score <= Self::MAX, "HealthScore out of range 0..=1000");
        Self(score)
    }
    #[must_use]
    pub const fn as_u16(self) -> u16 {
        self.0
    }
}

impl From<HealthScore> for u16 {
    fn from(h: HealthScore) -> Self {
        h.0
    }
}

/// FirmwareVersion
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct FirmwareVersion(u32);

impl FirmwareVersion {
    #[must_use]
    pub const fn new(major: u32, minor: u8, patch: u8) -> Self {
        assert!(major < 100, "firmware minor must be < 100");
        assert!(patch < 100, "firmware patch must be < 100");
        Self(major * 10_000 + minor as u32 * 100 + patch as u32)
    }
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
    #[must_use]
    pub const fn parts(self) -> (u32, u8, u8) {
        let v = self.0;
        (v / 10_000, ((v / 100) % 100) as u8, (v % 100) as u8)
    }
}

impl From<FirmwareVersion> for u32 {
    fn from(v: FirmwareVersion) -> Self {
        v.0
    }
}

/// FreezerReading 
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct FreezerReading {
    pub device_id: DeviceId,
    pub timestamp_secs: u64,
    pub temp: TempSentiC,
    pub door_open: bool,
    pub door_open_secs: u16,
    pub health_score: u16,
    pub rssi_dbm: i8,
    pub firmware_version: u32,
}

impl FreezerReading {
    pub const HEALTH_MAX: u16 = 1_000;
    pub const PLACEHOLDER: Self = Self {
        device_id: DeviceId::ZERO,
        timestamp_secs: 0,
        temp: TempCentiC::from_centi(thresholds::NORMAL_MAX.as_centi() - 500),
        door_open: false,
        door_open_secs: 0,
        health_score: Self::HEALTH_MAX,
        rssi_dbm: -50,
        firmware_version: 10_000, // v1.0.0
    };
    #[must_use]
    pub const fn 
}