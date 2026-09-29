//! Telemetry domain types, fixed-point representations, and serialization invariants.
//!
//! Designed specifically for high-assurance, `no_std` embedded microcontrollers (such as ESP32):
//! - **Zero heap allocations:** No dynamic strings, no heap vectors, and no runtime formatters.
//! - **Fixed stack buffers:** Wire protocol encoding operates entirely over stack-allocated byte arrays.
//! - **Compile-time evaluation:** Core conversions, comparisons, and layout calculations are marked `const fn`.
//! - **Bit-exact invariants:** Strongly-typed wrappers prevent unit mismatches and out-of-range sensor readings.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::fmt;

/// A 32-bit unique hardware identifier for a Sentinel device node.
///
/// Typically derived from the lower 4 bytes of an IEEE 802 EUI-48 (MAC) address,
/// discarding the vendor-assigned 24-bit OUI (Organizationally Unique Identifier).
/// Formatted as an 8-character uppercase ASCII hexadecimal string for human-readable
/// logging and zero-allocation MQTT topic construction.
///
/// # Invariants
///
/// - Transparent wrapper around [`u32`].
/// - Serializes transparently as an unsigned 32-bit integer.
///
/// # Examples
///
/// ```
/// use sentinel_core::telemetry::DeviceId;
///
/// let id = DeviceId::new(0x00A4_F3B2);
/// assert_eq!(id.as_u32(), 0x00A4_F3B2);
/// assert_eq!(&id.as_hex(), b"00A4F3B2");
/// ```
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct DeviceId(u32); 

/// Error returned when parsing a [`DeviceId`] from an ASCII hexadecimal byte slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParseIdError {
    /// An invalid ASCII byte was encountered outside the `[0-9A-Fa-f]` ranges.
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
    /// The default all-zeros sentinel device identifier (`0x00000000`).
    pub const ZERO: Self = Self(0);

    /// Wraps a raw 32-bit integer as a [`DeviceId`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::DeviceId;
    ///
    /// let id = DeviceId::new(0xDEAD_BEEF);
    /// assert_eq!(id.as_u32(), 0xDEAD_BEEF);
    /// ```
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Extracts a [`DeviceId`] from the last 4 bytes of a 6-byte EUI-48 MAC address.
    ///
    /// The first 2 bytes (`mac[0..2]`) belonging to the 24-bit OUI prefix are ignored.
    /// The remaining 4 bytes (`mac[2..6]`) are decoded as big-endian.
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::DeviceId;
    ///
    /// let mac = [0xAA, 0xBB, 0x00, 0xA4, 0xF3, 0xB2];
    /// let id = DeviceId::from_mac_tail(mac);
    /// assert_eq!(id.as_u32(), 0x00A4_F3B2);
    /// ```
    #[must_use]
    pub const fn from_mac_tail(mac: [u8; 6]) -> Self {
        Self(u32::from_be_bytes([mac[2], mac[3], mac[4], mac[5]]))
    }

    /// Parses an 8-byte hexadecimal ASCII representation into a [`DeviceId`].
    ///
    /// Accepts both uppercase (`'A'..='F'`) and lowercase (`'a'..='f'`) characters.
    ///
    /// # Errors
    ///
    /// Returns [`ParseIdError::BadDigit`] if any byte is not a valid ASCII hex character.
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::DeviceId;
    ///
    /// let id = DeviceId::from_hex(b"00A4F3B2").unwrap();
    /// assert_eq!(id.as_u32(), 0x00A4_F3B2);
    /// ```
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

    /// Returns the underlying raw [`u32`] identifier.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// Formats the device identifier into an 8-byte uppercase hexadecimal ASCII buffer.
    ///
    /// Does not allocate; the result is returned directly on the stack.
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::DeviceId;
    ///
    /// let id = DeviceId::new(0x00A4_F3B2);
    /// assert_eq!(&id.as_hex(), b"00A4F3B2");
    /// ```
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

/// Fixed-point temperature representation in centi-degrees Celsius (0.01 °C / 10 m°C).
///
/// Backed by a signed 16-bit integer ([`i16`]), providing a representable temperature range
/// of `-327.68 °C` to `+327.67 °C`.
///
/// ### Why Fixed-Point?
/// On resource-constrained bare-metal targets (or soft-float FPUs), floating-point operations
/// introduce code bloat, non-deterministic execution times, and potential NaN/Infinity serialization issues.
/// Fixed-point centi-degrees eliminate float inaccuracies while preserving 2 decimal places of precision.
///
/// # Examples
///
/// ```
/// use sentinel_core::telemetry::TempCentiC;
///
/// let temp = TempCentiC::from_centi(-1850);
/// assert_eq!(temp.as_centi(), -1850);
/// ```
#[repr(transparent)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct TempCentiC(i16);

impl TempCentiC {
    /// Creates a [`TempCentiC`] from a raw integer count of hundredths of a degree Celsius.
    ///
    /// Example: `-1850` represents `-18.50 °C`.
    #[must_use]
    pub const fn from_centi(centi: i16) -> Self {
        Self(centi)
    }

    /// Returns the raw temperature value in centi-degrees Celsius.
    #[must_use]
    pub const fn as_centi(self) -> i16 {
        self.0
    }

    /// Compares two [`TempCentiC`] instances in a compile-time `const` context.
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::TempCentiC;
    /// use core::cmp::Ordering;
    ///
    /// const COLD: TempCentiC = TempCentiC::from_centi(-1800);
    /// const WARM: TempCentiC = TempCentiC::from_centi(-1000);
    /// const CMP: Ordering = COLD.const_cmp(WARM);
    /// assert_eq!(CMP, Ordering::Less);
    /// ```
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

    /// Deconstructs the temperature into sign, whole degrees, and fractional centis.
    ///
    /// Correctly handles the subtle `-0.xx °C` case (e.g. `-0.50 °C`) where integer division
    /// by 100 yields `0`, which would otherwise lose the negative sign in basic integer formatting.
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

/// Deconstructed temperature components with explicit sign semantics.
///
/// Designed to format temperatures into strings or displays without heap allocation, avoiding
/// the common integer truncation bug where `-0.50 °C` prints as `0.50 °C`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TempParts {
    /// True if the temperature is strictly below `0.00 °C`.
    pub is_negative: bool,
    /// Absolute whole degree component (`|centi| / 100`).
    pub whole: u16,
    /// Fractional hundredths of a degree (`|centi| % 100`), in `0..=99`.
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

/// Standard cold-chain freezer operating thresholds.
pub mod thresholds {
    use super::TempCentiC;

    /// The upper limit for normal deep-freeze storage (-15.00 °C).
    ///
    /// Temperatures at or below this limit are considered [`TemperatureStatus::Normal`](super::TemperatureStatus::Normal).
    pub const NORMAL_MAX: TempCentiC = TempCentiC::from_centi(-1_500);

    /// The upper limit for warned storage before critical failure (-10.00 °C).
    ///
    /// Temperatures above this threshold are classified as [`TemperatureStatus::Critical`](super::TemperatureStatus::Critical).
    pub const WARNING_MAX: TempCentiC = TempCentiC::from_centi(-1_000);
}

/// Operational temperature health status of a monitored freezer unit.
///
/// Mapped directly to alert tiers for supervisory dashboards, cold-chain compliance,
/// and automated relay trip logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
#[repr(u8)]
pub enum TemperatureStatus {
    /// Operating within nominal deep-freeze range (<= -15.00 °C).
    Normal = 0,
    /// Elevated temperature warning (-15.00 °C < temp <= -10.00 °C). Inspection recommended.
    Warning = 1,
    /// Critical high-temperature threshold breached (> -10.00 °C). Immediate intervention required.
    Critical = 2,
}

impl TemperatureStatus {
    /// Returns the uppercase ASCII status label (`"NORMAL"`, `"WARNING"`, or `"CRITICAL"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "NORMAL",
            Self::Warning => "WARNING",
            Self::Critical => "CRITICAL",
        }
    }

    /// Returns `true` if the status indicates an abnormal state requiring operator attention.
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

/// Error indicating that a health score exceeded the maximum allowable value of 1,000.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HealthScoreError(
    /// The invalid raw score that caused the error.
    pub u16,
);

impl fmt::Display for HealthScoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "health score {} exceeds max 1000", self.0)
    }
}

impl core::error::Error for HealthScoreError {}

/// Normalized device reliability and operational health score, constrained to `0..=1000`.
///
/// Represents unit health from `0.0%` (complete hardware/communication failure) to `100.0%`
/// (optimal nominal telemetry). Serde serialization enforces this invariant via `try_from = "u16"`.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub struct HealthScore(u16);

impl HealthScore {
    /// Maximum permissible health score (1,000, corresponding to 100.0%).
    pub const MAX: u16 = 1_000;

    /// Minimum health score (0, corresponding to failed/decommissioned unit).
    pub const ZERO: Self = Self(0);

    /// Attempts to create a [`HealthScore`], returning an error if `score > 1000`.
    ///
    /// # Errors
    ///
    /// Returns [`HealthScoreError`] if `score` exceeds [`HealthScore::MAX`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::HealthScore;
    ///
    /// let score = HealthScore::try_new(950).unwrap();
    /// assert_eq!(score.as_u16(), 950);
    /// assert!(HealthScore::try_new(1001).is_err());
    /// ```
    pub const fn try_new(score: u16) -> Result<Self, HealthScoreError> {
        if score <= Self::MAX { Ok(Self(score)) } else { Err(HealthScoreError(score)) }
    }

    /// Creates a [`HealthScore`], panicking if `score > 1000`.
    ///
    /// # Panics
    ///
    /// Panics if `score` exceeds [`HealthScore::MAX`].
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::HealthScore;
    ///
    /// let score = HealthScore::new(1000);
    /// assert_eq!(score.as_u16(), 1000);
    /// ```
    #[must_use]
    pub const fn new(score: u16) -> Self {
        match Self::try_new(score) {
            Ok(v) => v,
            Err(_) => panic!("HealthScore out of range 0..=1000"),
        }
    }

    /// Creates a [`HealthScore`] saturating at [`HealthScore::MAX`] if out of bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::HealthScore;
    ///
    /// let score = HealthScore::new_saturating(1500);
    /// assert_eq!(score.as_u16(), 1000);
    /// ```
    #[must_use]
    pub const fn new_saturating(score: u16) -> Self {
        Self(if score > Self::MAX { Self::MAX } else { score })
    }

    /// Returns the raw health score integer value in `0..=1000`.
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

/// Error conditions encountered when constructing or parsing a [`FirmwareVersion`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FirmwareVersionError {
    /// Major version exceeded the maximum packable limit of 429,496.
    Major,
    /// Minor version was greater than or equal to 100.
    Minor,
    /// Patch version was greater than or equal to 100.
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

/// Compact decimal-packed semantic version (`MAJOR.MINOR.PATCH`).
///
/// Encoded into a single 32-bit integer as `major * 10,000 + minor * 100 + patch`.
/// - `major`: `0..=429,496`
/// - `minor`: `0..=99`
/// - `patch`: `0..=99`
///
/// This compact representation allows version exchange across low-bandwidth mesh radios,
/// CAN-bus frames, and atomic register comparisons without dynamic string allocations.
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
    /// Maximum allowable major version number (429,496) before overflowing a [`u32`].
    pub const MAX_MAJOR: u32 = u32::MAX / 10_000;

    /// Attempts to construct a [`FirmwareVersion`] from components.
    ///
    /// # Errors
    ///
    /// Returns [`FirmwareVersionError`] if `major > 429,496`, `minor >= 100`, or `patch >= 100`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::FirmwareVersion;
    ///
    /// let ver = FirmwareVersion::try_new(1, 2, 3).unwrap();
    /// assert_eq!(ver.parts(), (1, 2, 3));
    /// ```
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

    /// Constructs a [`FirmwareVersion`], panicking if components violate range limits.
    ///
    /// # Panics
    ///
    /// Panics if `major > 429,496`, `minor >= 100`, or `patch >= 100`.
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::FirmwareVersion;
    ///
    /// let ver = FirmwareVersion::new(2, 4, 1);
    /// assert_eq!(ver.parts(), (2, 4, 1));
    /// ```
    #[must_use]
    pub const fn new(major: u32, minor: u8, patch: u8) -> Self {
        match Self::try_new(major, minor, patch) {
            Ok(v) => v,
            Err(FirmwareVersionError::Major) => panic!("firmware major must be <= 429496"),
            Err(FirmwareVersionError::Minor) => panic!("firmware minor must be < 100"),
            Err(FirmwareVersionError::Patch) => panic!("firmware patch must be < 100"),
        }
    }

    /// Returns the underlying decimal-packed 32-bit version representation.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }

    /// Unpacks the version into `(major, minor, patch)` components.
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::FirmwareVersion;
    ///
    /// let ver = FirmwareVersion::new(1, 20, 9);
    /// assert_eq!(ver.parts(), (1, 20, 9));
    /// ```
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

/// Comprehensive telemetry record emitted by a Sentinel cold-storage sensor node.
///
/// Contains environmental metrics, hardware tamper/door events, signal strength,
/// device diagnostics, and firmware metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[derive(serde::Serialize, serde::Deserialize)]
pub struct FreezerReading {
    /// Unique 32-bit hardware node identifier.
    pub device_id: DeviceId,
    /// Unix timestamp in seconds since epoch (UTC).
    pub timestamp_secs: u64,
    /// Ambient freezer temperature in centi-degrees Celsius (0.01 °C resolution).
    pub temp: TempCentiC,
    /// Instantaneous door switch state (`true` = open, `false` = closed).
    pub door_open: bool,
    /// Continuous duration in seconds the door has remained unsealed.
    pub door_open_secs: u16,
    /// Aggregate hardware health and link score (`0..=1000`).
    pub health_score: HealthScore,
    /// Received Signal Strength Indication (RSSI) in dBm.
    pub rssi_dbm: i8,
    /// Node firmware version.
    pub firmware_version: FirmwareVersion,
}

impl FreezerReading {
    /// Maximum possible health score constant (1,000).
    pub const HEALTH_MAX: u16 = HealthScore::MAX;

    /// Nominal placeholder reading used for default initialization and offline testing.
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

    /// Evaluates the temperature reading against configured operating thresholds.
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

    /// Returns `true` if temperature exceeds [`thresholds::WARNING_MAX`].
    #[must_use]
    pub const fn is_temp_critical(&self) -> bool {
        matches!(self.temperature_status(), TemperatureStatus::Critical)
    }

    /// Returns `true` if temperature is in warning range (`NORMAL_MAX < temp <= WARNING_MAX`).
    #[must_use]
    pub const fn is_temp_warning(&self) -> bool {
        matches!(self.temperature_status(), TemperatureStatus::Warning)
    }

    /// Returns `true` if temperature is either in warning or critical status.
    #[must_use]
    pub const fn is_temp_alert(&self) -> bool {
        self.temperature_status().is_alert()
    }

    /// Deconstructs the sensor temperature into signed display components.
    #[must_use]
    pub const fn temp_display_parts(&self) -> TempParts {
        self.temp.display_parts()
    }

    /// Unpacks the firmware version into `(major, minor, patch)`.
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

/// Compile-time and runtime ASCII MQTT topic generation.
///
/// Produces fixed-size byte buffers formatted as `sentinel/<DEVICE_HEX>/telemetry`
/// without allocating heap memory or using formatters.
pub mod topics {
    use super::DeviceId;

    /// The topic prefix byte sequence (`"sentinel/"`).
    pub const PREFIX: &[u8; 9] = b"sentinel/";

    /// The topic suffix byte sequence (`"/telemetry"`).
    pub const SUFFIX: &[u8; 10] = b"/telemetry";

    /// Total byte length of a fully rendered telemetry topic (27 bytes).
    pub const LEN: usize = PREFIX.len() + 8 + SUFFIX.len();

    /// Constructs a fixed-size ASCII byte array representing the MQTT telemetry topic for a device.
    ///
    /// # Examples
    ///
    /// ```
    /// use sentinel_core::telemetry::{topics, DeviceId};
    ///
    /// let id = DeviceId::new(0x00A4_F3B2);
    /// let topic = topics::telemetry(id);
    /// assert_eq!(topics::as_str(&topic), "sentinel/00A4F3B2/telemetry");
    /// ```
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

    /// Converts a validated topic buffer into a `&str` reference.
    ///
    /// Guaranteed not to panic because topic bytes are ASCII by construction.
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
    // DeviceId
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

    // Thresholds
    #[test]
    fn thresholds_are_ordered() {
        assert!(thresholds::NORMAL_MAX.as_centi() < thresholds::WARNING_MAX.as_centi());
    }

    // TempCentiC
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
        // Deep sub-zero
        let r = reading_with_temp(-1850).temp_display_parts();
        assert!(r.is_negative);
        assert_eq!((r.whole, r.centis), (18, 50));

        // Positive
        let r = reading_with_temp(520).temp_display_parts();
        assert!(!r.is_negative);
        assert_eq!((r.whole, r.centis), (5, 20));
        assert_eq!(std::format!("{}", r), "5.20");
        
        // The Edge case that was previously broken: -0.50 C
        let r = reading_with_temp(-50).temp_display_parts();
        assert!(r.is_negative);
        assert_eq!((r.whole, r.centis), (0, 50));
        assert_eq!(std::format!("{}", r), "-0.50");

        // Tiny negative: -0.05 C
        let r = reading_with_temp(-5).temp_display_parts();
        assert!(r.is_negative);
        assert_eq!((r.whole, r.centis), (0, 5));
        assert_eq!(std::format!("{}", r), "-0.05");

        // Exact zero: 0.00 C
        let r = reading_with_temp(0).temp_display_parts();
        assert!(!r.is_negative);
        assert_eq!((r.whole, r.centis), (0, 0));
        assert_eq!(std::format!("{}", r), "0.00");
    }

    // HealthScore
    
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
        
        // Invalid score over 100 Must be rejected by Serde
        let invalid_json = "1001";
        let result: Result<HealthScore, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());
    }


// FirmwareVersion

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

// Temprature status boundaries
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

// Default reading
    #[test]
    fn defalut_reading_is_healthy() {
        let r = FreezerReading::default();
        assert_eq!(r.health_score.as_u16(), FreezerReading::HEALTH_MAX);
        assert_eq!(r.temperature_status(), TemperatureStatus::Normal);
        assert!(!r.is_temp_critical());
        assert!(!r.is_temp_warning());
        assert_eq!(FreezerReading::PLACEHOLDER, r);
    }

// firmwareversion
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

// topics
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