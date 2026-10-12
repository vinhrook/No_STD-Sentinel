use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryReading {
    #[serde(alias = "d", alias = "device_id")]
    pub device_id: u32,
    #[serde(alias = "ts", alias = "timestamp_secs")]
    pub timestamp_secs: u64,
    #[serde(alias = "tc", alias = "temp")]
    pub temp: i16,
    #[serde(alias = "ac", alias = "ambient_temp", default)]
    pub ambient_temp: Option<i16>,
    #[serde(alias = "do", alias = "door_open")]
    pub door_open: bool,
    #[serde(alias = "ds", alias = "door_open_secs")]
    pub door_open_secs: u16,
    #[serde(alias = "hs", alias = "health_score")]
    pub health_score: u16,
    #[serde(alias = "rs", alias = "rssi_dbm")]
    pub rssi_dbm: i8,
    #[serde(alias = "fv", alias = "firmware_version")]
    pub firmware_version: u32,
}

impl Default for TelemetryReading {
    fn default() -> Self {
        Self {
            device_id: 176760392,
            timestamp_secs: 0,
            temp: -2100,
            ambient_temp: Some(2350),
            door_open: false,
            door_open_secs: 0,
            health_score: 980,
            rssi_dbm: -48,
            firmware_version: 10000,
        }
    }
}

impl TelemetryReading {
    #[inline]
    pub fn temp_c(&self) -> f64 {
        self.temp as f64 / 100.0
    }

    #[inline]
    pub fn temp_f(&self) -> f64 {
        (self.temp_c() * 9.0 / 5.0) + 32.0
    }

    #[inline]
    pub fn ambient_c(&self) -> Option<f64> {
        self.ambient_temp.map(|t| t as f64 / 100.0)
    }

    #[inline]
    pub fn ambient_f(&self) -> Option<f64> {
        self.ambient_c().map(|c| (c * 9.0 / 5.0) + 32.0)
    }

    #[inline]
    pub fn delta_c(&self) -> Option<f64> {
        self.ambient_c().map(|amb| amb - self.temp_c())
    }

    #[inline]
    pub fn delta_f(&self) -> Option<f64> {
        self.delta_c().map(|d| d * 9.0 / 5.0)
    }

    #[inline]
    pub fn hex_id(&self) -> String {
        format!("0x{:08X}", self.device_id)
    }

    pub fn severity(&self) -> Severity {
        let temp_c = self.temp_c();
        if temp_c > -12.0 || self.door_open_secs > 180 || self.health_score < 500 {
            Severity::Critical
        } else if temp_c > -18.0 || self.door_open_secs > 60 || self.health_score < 750 {
            Severity::Warning
        } else {
            Severity::Healthy
        }
    }

    pub fn health_grade(&self) -> &'static str {
        match self.severity() {
            Severity::Healthy => "OPTIMAL",
            Severity::Warning => "ATTENTION REQUIRED",
            Severity::Critical => "CRITICAL DEGRADATION",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Healthy,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TempUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartDataPoint {
    pub time: String,
    pub timestamp_secs: u64,
    pub temp_c: f64,
    pub ambient_c: Option<f64>,
    pub score: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub time: String,
    pub tag: String,
    pub message: String,
    pub is_alert: bool,
}
