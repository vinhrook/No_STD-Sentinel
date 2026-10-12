#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

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
    pub const ZERO: Self = Self(0);

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
            buf[i] = DIGITS[((self.0 >> (28 - i * 4)) & 0xF) as usize];
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

#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
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

    #[must_use]
    pub const fn display_parts(self) -> TempParts {
        let is_negative = self.0 < 0;
        let mag = self.0.unsigned_abs();
        TempParts {
            is_negative,
            whole: mag / 100,
            centis: (mag % 100) as u8,
        }
    }
}

impl fmt::Display for TempCentiC {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = self.0;
        if v < 0 {
            f.write_str("-")?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TempParts {
    pub is_negative: bool,
    pub whole: u16,
    pub centis: u8,
}

impl fmt::Display for TempParts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_negative {
            f.write_str("-")?;
        }
        write!(f, "{}.{:02}", self.whole, self.centis)
    }
}

pub mod thresholds {
    use super::TempCentiC;

    pub const NORMAL_MAX: TempCentiC = TempCentiC::from_centi(-1_500);
    pub const WARNING_MAX: TempCentiC = TempCentiC::from_centi(-1_000);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
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
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HealthScoreError(
    pub u16,
);

impl fmt::Display for HealthScoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "health score {} exceeds max 1000", self.0)
    }
}

impl core::error::Error for HealthScoreError {}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct HealthScore(u16);

impl HealthScore {
    pub const MAX: u16 = 1_000;
    pub const ZERO: Self = Self(0);

    pub const fn try_new(score: u16) -> Result<Self, HealthScoreError> {
        if score <= Self::MAX { Ok(Self(score)) } else { Err(HealthScoreError(score)) }
    }

    #[must_use]
    pub const fn new(score: u16) -> Self {
        match Self::try_new(score) {
            Ok(v) => v,
            Err(_) => panic!("HealthScore out of range 0..=1000"),
        }
    }

    #[must_use]
    pub const fn new_saturating(score: u16) -> Self {
        Self(if score > Self::MAX { Self::MAX } else { score })
    }

    #[must_use]
    pub const fn as_u16(self) -> u16 { self.0 }
}

impl TryFrom<u16> for HealthScore {
    type Error = HealthScoreError;
    fn try_from(v: u16) -> Result<Self, Self::Error> { Self::try_new(v) }
}

impl From<HealthScore> for u16 {
    fn from(h: HealthScore) -> Self { h.0 }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FirmwareVersionError {
    Major,
    Minor,
    Patch,
}

impl fmt::Display for FirmwareVersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let p = match self {
            Self::Major => "major <= 429496",
            Self::Minor => "minor < 100",
            Self::Patch => "patch < 100",
        };
        write!(f, "invalid firmware {p}")
    }
}

impl core::error::Error for FirmwareVersionError {}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "u32", into = "u32")]
pub struct FirmwareVersion(u32);

impl TryFrom<u32> for FirmwareVersion {
    type Error = FirmwareVersionError;

    fn try_from(raw: u32) -> Result<Self, Self::Error> {
        let major = raw / 10_000;
        let minor = ((raw / 100) % 100) as u8;
        let patch = (raw % 100) as u8;
        Self::try_new(major, minor, patch)
    }
}

impl FirmwareVersion {
    pub const MAX_MAJOR: u32 = u32::MAX / 10_000;

    pub const fn try_new(major: u32, minor: u8, patch: u8) -> Result<Self, FirmwareVersionError> {
        if major > Self::MAX_MAJOR {
            Err(FirmwareVersionError::Major)
        } else if minor >= 100 {
            Err(FirmwareVersionError::Minor)
        } else if patch >= 100 {
            Err(FirmwareVersionError::Patch)
        } else {
            Ok(Self(major * 10_000 + minor as u32 * 100 + patch as u32))
        }
    }

    #[must_use]
    pub const fn new(major: u32, minor: u8, patch: u8) -> Self {
        match Self::try_new(major, minor, patch) {
            Ok(v) => v,
            Err(FirmwareVersionError::Major) => panic!("firmware major must be <= 429496"),
            Err(FirmwareVersionError::Minor) => panic!("firmware minor must be < 100"),
            Err(FirmwareVersionError::Patch) => panic!("firmware patch must be < 100"),
        }
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

impl fmt::Display for FirmwareVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (major, minor, patch) = self.parts();
        write!(f, "{major}.{minor}.{patch}")
    }
}

impl From<FirmwareVersion> for u32 {
    fn from(v: FirmwareVersion) -> Self {
        v.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct FreezerReading {
    pub device_id: DeviceId,
    pub timestamp_secs: u64,
    pub temp: TempCentiC,
    pub door_open: bool,
    pub door_open_secs: u16,
    pub health_score: HealthScore,
    pub rssi_dbm: i8,
    pub firmware_version: FirmwareVersion,
}

impl FreezerReading {
    pub const HEALTH_MAX: u16 = HealthScore::MAX;

    pub const PLACEHOLDER: Self = Self {
        device_id: DeviceId::ZERO,
        timestamp_secs: 0,
        temp: TempCentiC::from_centi(thresholds::NORMAL_MAX.as_centi() - 500),
        door_open: false,
        door_open_secs: 0,
        health_score: HealthScore::new(HealthScore::MAX),
        rssi_dbm: -50,
        firmware_version: FirmwareVersion::new(1, 0, 0),
    };

    #[must_use]
    pub const fn temperature_status(&self) -> TemperatureStatus {
        match self.temp.const_cmp(thresholds::WARNING_MAX) {
            core::cmp::Ordering::Greater => TemperatureStatus::Critical,
            _ => match self.temp.const_cmp(thresholds::NORMAL_MAX) {
                core::cmp::Ordering::Greater => TemperatureStatus::Warning,
                _ => TemperatureStatus::Normal,
            },
        }
    }

    #[must_use]
    pub const fn is_temp_critical(&self) -> bool {
        matches!(self.temperature_status(), TemperatureStatus::Critical)
    }

    #[must_use]
    pub const fn is_temp_warning(&self) -> bool {
        matches!(self.temperature_status(), TemperatureStatus::Warning)
    }

    #[must_use]
    pub const fn is_temp_alert(&self) -> bool {
        self.temperature_status().is_alert()
    }

    #[must_use]
    pub const fn temp_display_parts(&self) -> TempParts {
        self.temp.display_parts()
    }

    #[must_use]
    pub const fn firmware_parts(&self) -> (u32, u8, u8) {
        self.firmware_version.parts()
    }
}

impl Default for FreezerReading {
    fn default() -> Self {
        Self::PLACEHOLDER
    }
}

pub mod topics {
    use super::DeviceId;

    pub const PREFIX: &[u8; 9] = b"sentinel/";
    pub const SUFFIX: &[u8; 10] = b"/telemetry";
    pub const LEN: usize = PREFIX.len() + 8 + SUFFIX.len();

    #[must_use]
    pub const fn telemetry(device: DeviceId) -> [u8; LEN] {
        let hex = device.as_hex();
        let mut buf = [0u8; LEN];
        let mut i = 0;
        while i < PREFIX.len() {
            buf[i] = PREFIX[i];
            i += 1;
        }
        let mut j = 0;
        while j < 8 {
            buf[PREFIX.len() + j] = hex[j];
            j += 1;
        }
        let mut k = 0;
        while k < SUFFIX.len() {
            buf[PREFIX.len() + 8 + k] = SUFFIX[k];
            k += 1;
        }
        buf
    }

    #[must_use]
    pub fn as_str(topic: &[u8; LEN]) -> &str {
        core::str::from_utf8(topic).expect("topic is ASCII by construction")
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    const TEST_ID: DeviceId = DeviceId::from_mac_tail([0xAA, 0xBB, 0x00, 0xA4, 0xF3, 0xB2]);

    fn reading_with_temp(centi: i16) -> FreezerReading {
        FreezerReading {
            temp: TempCentiC::from_centi(centi),
            ..Default::default()
        }
    }
    #[test]
    fn device_id_from_mac() {
        assert_eq!(TEST_ID.as_u32(), 0x00A4_F3B2);
    }

    #[test]
    fn device_id_ignores_oui_bytes() {
        let other = DeviceId::from_mac_tail([0x11, 0x22, 0x00, 0xA4, 0xF3, 0xB2]);
        assert_eq!(other, TEST_ID);
    }

    #[test]
    fn device_id_hex_edges() {
        assert_eq!(DeviceId::new(0).as_hex(), *b"00000000");
        assert_eq!(DeviceId::new(u32::MAX).as_hex(), *b"FFFFFFFF");
        assert_eq!(TEST_ID.as_hex(), *b"00A4F3B2");
    }

    #[test]
    fn device_id_from_hex_roundtrip() {
        assert_eq!(DeviceId::from_hex(b"00A4F3B2"), Ok(TEST_ID));
        assert_eq!(DeviceId::from_hex(b"00a4f3b2"), Ok(TEST_ID));
        assert_eq!(DeviceId::from_hex(&TEST_ID.as_hex()), Ok(TEST_ID));
    }

    #[test]
    fn device_id_from_hex_rejects_bad_digit() {
        assert_eq!(
            DeviceId::from_hex(b"00A4F3BZ"),
            Err(ParseIdError::BadDigit(b'Z'))
        );
    }

    #[test]
    fn device_id_u32_round_trip() {
        let id = DeviceId::from(0xDEAD_BEEF);
        assert_eq!(u32::from(id), 0xDEAD_BEEF);
        assert_eq!(DeviceId::ZERO.as_u32(), 0);
    }

    #[test]
    fn parse_id_error_is_std_error() {
        fn assert_error<T: core::error::Error>() {}
        assert_error::<ParseIdError>();
        assert_eq!(
           std::format!("{}", ParseIdError::BadDigit(b'Z')),
            "invalid hex digit: 0x5A"
        );
    }

    #[test]
    fn device_id_display() {
        assert_eq!(std::format!("{}",TEST_ID), "00A4F3B2");
    }

    #[test]
    fn thresholds_are_ordered() {
        assert!(thresholds::NORMAL_MAX.as_centi() < thresholds::WARNING_MAX.as_centi());
    }

    #[test]
    fn temp_const_cmpordering() {
        let cold = TempCentiC::from_centi(-1500);
        let warm = TempCentiC::from_centi(-1000);
        assert_eq!(cold.const_cmp(warm), core::cmp::Ordering::Less);
        assert_eq!(warm.const_cmp(cold), core::cmp::Ordering::Greater);
        assert_eq!(cold.const_cmp(cold), core::cmp::Ordering::Equal);
        assert!(cold < warm);
    }

    #[test]
    fn temp_display_negative() {
        assert_eq!(std::format!("{}", reading_with_temp(-1850).temp), "-18.50");
    }

    #[test]
    fn temp_display_positive() {
        assert_eq!(std::format!("{}", reading_with_temp(520).temp), "5.20");
    }

    #[test]
    fn temp_display_positive_zero_keeps_sign() {
        assert_eq!(std::format!("{}", reading_with_temp(-5).temp), "-0.05");
    }

    #[test]
    fn temp_display_parts_split() {
        let r = reading_with_temp(-1850).temp_display_parts();
        assert!(r.is_negative);
        assert_eq!((r.whole, r.centis), (18, 50));

        let r = reading_with_temp(520).temp_display_parts();
        assert!(!r.is_negative);
        assert_eq!((r.whole, r.centis), (5, 20));
        assert_eq!(std::format!("{}", r), "5.20");
        
        let r = reading_with_temp(-50).temp_display_parts();
        assert!(r.is_negative);
        assert_eq!((r.whole, r.centis), (0, 50));
        assert_eq!(std::format!("{}", r), "-0.50");

        let r = reading_with_temp(-5).temp_display_parts();
        assert!(r.is_negative);
        assert_eq!((r.whole, r.centis), (0, 5));
        assert_eq!(std::format!("{}", r), "-0.05");

        let r = reading_with_temp(0).temp_display_parts();
        assert!(!r.is_negative);
        assert_eq!((r.whole, r.centis), (0, 0));
        assert_eq!(std::format!("{}", r), "0.00");
    }

    #[test]
    fn health_score_const_new() {
        const H: HealthScore = HealthScore::new(1_000);
        assert_eq!(H.as_u16(), 1_000);
        assert_eq!(HealthScore::ZERO.as_u16(), 0);
        assert_eq!(u16::from(H), 1_000);
    }

    #[test]
    #[should_panic(expected = "HealthScore out of range")]
    fn health_score_rejects_over_max() {
        let _ = HealthScore::new(1_001);
    }

    #[test]
    fn health_score_try_new() {
        assert_eq!(HealthScore::try_new(0).map(|h| h.as_u16()), Ok(0));
        assert_eq!(HealthScore::try_new(1000).map(|h| h.as_u16()), Ok(1000));
        assert_eq!(
            HealthScore::try_new(1001),
            Err(HealthScoreError(1001))
        );
    }

    #[test]
    fn health_score_saturating() {
        assert_eq!(HealthScore::new_saturating(500).as_u16(), 500);
        assert_eq!(HealthScore::new_saturating(2000).as_u16(), 1000);
    }

    #[test]
    fn health_score_serde_enforces_invariant() {
        let valid_json = "950";
        let score: Result<HealthScore, _> = serde_json::from_str(valid_json);
        assert_eq!(score.unwrap().as_u16(), 950);
        
        let invalid_json = "1001";
        let result: Result<HealthScore, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn firmware_version_new_and_parts() {
        const V: FirmwareVersion = FirmwareVersion::new(1, 2, 3);
        assert_eq!(V.as_u32(), 10_203);
        assert_eq!(V.parts(), (1, 2, 3));
        assert_eq!(u32::from(V), 10_203);
    }

    #[test]
    #[should_panic(expected = "firmware minor must be < 100")]
    fn firmware_version_rejects_bad_minor() {
        let _ = FirmwareVersion::new(1, 100, 0);
    }

    #[test]
    fn status_normal_at_boundary() {
        let r = reading_with_temp(-1500);
        assert_eq!(r.temperature_status(), TemperatureStatus::Normal);
        assert!(!r.is_temp_warning());
        assert!(!r.is_temp_critical());
    }

    #[test]
    fn status_warning_just_above_normal() {
        let r = reading_with_temp(-1499);
        assert_eq!(r.temperature_status(), TemperatureStatus::Warning);
        assert!(!r.is_temp_critical());
        assert!(r.is_temp_warning());
    }

    #[test]
    fn status_warning_at_critical_boundary() {
        let r = reading_with_temp(-1000);
        assert_eq!(r.temperature_status(), TemperatureStatus::Warning);
        assert!(!r.is_temp_critical());
        assert!(r.is_temp_warning());
    }

    #[test]
    fn status_critical_just_above_boundary() {
        let r = reading_with_temp(-999);
        assert_eq!(r.temperature_status(), TemperatureStatus::Critical);
        assert!(r.is_temp_critical());
        assert!(!r.is_temp_warning());
        assert!(r.is_temp_alert());
    }

    #[test]
    fn test_temperature_status_predicates() {
        let critical = reading_with_temp(-800);
        assert!(critical.is_temp_critical());
        assert!(!critical.is_temp_warning());
        assert!(critical.is_temp_alert());

        let warning = reading_with_temp(-1200);
        assert!(!warning.is_temp_critical());
        assert!(warning.is_temp_warning());
        assert!(warning.is_temp_alert());

        let normal = reading_with_temp(-1800);
        assert!(!normal.is_temp_critical());
        assert!(!normal.is_temp_warning());
        assert!(!normal.is_temp_alert());
    }

    #[test]
    fn status_lables_and_ordering() {
        assert_eq!(TemperatureStatus::Critical.as_str(), "CRITICAL");
        assert_eq!(std::format!("{}",TemperatureStatus::Warning), "WARNING");
        assert!(TemperatureStatus::Critical.is_alert());
        assert!(!TemperatureStatus::Normal.is_alert());
        assert!(TemperatureStatus::Critical > TemperatureStatus::Warning);
    }

    #[test]
    fn defalut_reading_is_healthy() {
        let r = FreezerReading::default();
        assert_eq!(r.health_score.as_u16(), FreezerReading::HEALTH_MAX);
        assert_eq!(r.temperature_status(), TemperatureStatus::Normal);
        assert!(!r.is_temp_critical());
        assert!(!r.is_temp_warning());
        assert_eq!(FreezerReading::PLACEHOLDER, r);
    }

    #[test]
    fn firmware_parts_v1_2_3() {
        let r = FreezerReading {
            firmware_version: FirmwareVersion::new(1, 2, 3),
            ..Default::default()
        };
        assert_eq!(r.firmware_parts(), (1, 2, 3));
    }
    #[test]
    fn firmware_version_display() {
        assert_eq!(std::format!("{}", FirmwareVersion::new(1, 2, 3)), "1.2.3");
        assert_eq!(std::format!("{}", FirmwareVersion::new(0, 0, 0)), "0.0.0");
    }
    #[test]
    fn firmware_version_try_new() {
        assert_eq!(FirmwareVersion::try_new(1, 2, 3).map(|v| v.parts()), Ok((1, 2, 3)));
        assert_eq!(
            FirmwareVersion::try_new(FirmwareVersion::MAX_MAJOR + 1, 0, 0),
            Err(FirmwareVersionError::Major)
        );
        assert_eq!(FirmwareVersion::try_new(1, 100, 0), Err(FirmwareVersionError::Minor));
        assert_eq!(FirmwareVersion::try_new(1, 0, 100), Err(FirmwareVersionError::Patch));
    }
    #[test]
    #[should_panic(expected = "firmware major must be <= 429496")]
    fn firmware_version_rejects_overflow_major() {
        let _ = FirmwareVersion::new(500_000, 0, 0);
    }

    #[test]
    fn topic_i_exact_lenght() {
        let topic = topics::telemetry(TEST_ID);
        assert_eq!(topic.len(), topics::LEN);
        assert_eq!(topics::as_str(&topic), "sentinel/00A4F3B2/telemetry");
        assert!(!topic.contains(&0));
    }
    
    #[test]
    fn topic_zero_id() {
        let topic = topics::telemetry(DeviceId::ZERO);
        assert_eq!(&topic, b"sentinel/00000000/telemetry");
    }
    const CT_TOPIC:[u8; topics::LEN] = topics::telemetry(TEST_ID);

    #[test]
    fn telemetry_topic_const_eval() {
        assert_eq!(&CT_TOPIC, b"sentinel/00A4F3B2/telemetry");
    }
}