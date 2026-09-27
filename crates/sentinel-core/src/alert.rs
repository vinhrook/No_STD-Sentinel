#![forbid(unsafe_code)]

use core::fmt;
use serde::{Deserialize, Serialize};
use crate::telemetry::{FreezerReading, TempCentiC};

/// Alert Severity levels (`Healthy < Warning < Critical`)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum Severity {
    #[default] Healthy = 0,
    Warning = 1,
    Critical = 2,
}

impl Severity {
    #[inline] pub const fn is_alert(self) -> bool { self as u8 > 0 }
    #[inline] pub const fn is_critical(self) -> bool { matches!(self, Self::Critical) }
    #[inline] pub const fn is_healthy(self) -> bool { matches!(self, Self::Healthy) }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self { Self::Healthy => "HEALTHY", Self::Warning => "WARNING", Self::Critical => "CRITICAL" })
    }
}

/// Root cause triggering an alert transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum AlertCause {
    TemperatureBreach = 0,
    DoorAjarTimeout = 1,
    CompoundBreach = 2,
}

impl fmt::Display for AlertCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TemperatureBreach => "Temperature threshold breached",
            Self::DoorAjarTimeout => "Door open duration exceeded",
            Self::CompoundBreach => "Compound failure (temp + door)",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdError {
    InvalidWarningHysteresis,
    InvalidCriticalHysteresis,
    CriticalBelowWarning,
    DoorCriticalBelowWarning,
}

impl fmt::Display for ThresholdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "threshold invariant violation: {self:?}")
    }
}
impl core::error::Error for ThresholdError {}

/// Configuration thresholds for the alert engine (in centi-celsius and seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thresholds {
    pub warn_trigger_cc: i16,
    pub warn_clear_cc: i16,
    pub critical_trigger_cc: i16,
    pub critical_clear_cc: i16,
    pub door_warn_secs: u16,
    pub door_critical_secs: u16,
}

impl Thresholds {
    pub const fn validate(&self) -> Result<(), ThresholdError> {
        if self.warn_trigger_cc <= self.warn_clear_cc {
            return Err(ThresholdError::InvalidWarningHysteresis);
        }
        if self.critical_trigger_cc <= self.critical_clear_cc {
            return Err(ThresholdError::InvalidCriticalHysteresis);
        }
        if self.critical_trigger_cc < self.warn_trigger_cc {
            return Err(ThresholdError::CriticalBelowWarning);
        }
        if self.door_critical_secs < self.door_warn_secs {
            return Err(ThresholdError::DoorCriticalBelowWarning);
        }
        Ok(())
    }

    #[must_use]
    pub const fn home_freezer() -> Self {
        Self {
            warn_trigger_cc: -1500,
            warn_clear_cc: -1700,
            critical_trigger_cc: -1000,
            critical_clear_cc: -1300,
            door_warn_secs: 120,
            door_critical_secs: 300,
        }
    }

    #[must_use]
    pub const fn cold_storage() -> Self {
        Self {
            warn_trigger_cc: -1800,
            warn_clear_cc: -2000,
            critical_trigger_cc: -1500,
            critical_clear_cc: -1800,
            door_warn_secs: 180,
            door_critical_secs: 600,
        }
    }
}

impl Default for Thresholds {
    fn default() -> Self {
        Self::home_freezer()
    }
}

/// An Alert event generated when severity escalates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlertEvent {
    pub timestamp_secs: u64,
    pub message: &'static str,
    pub device_id: u32,
    pub temp_centi_c: i16,
    pub door_open_secs: u16,
    pub health_score: u16,
    pub severity: Severity,
    pub cause: AlertCause,
    pub door_open: bool,
}

impl AlertEvent {
    #[must_use]
    pub const fn contextual_message(sev: Severity, cause: AlertCause) -> &'static str {
        match (sev, cause) {
            (Severity::Critical, AlertCause::TemperatureBreach) => "CRITICAL: Freezer temp breached critical threshold! Intervention required!",
            (Severity::Critical, AlertCause::DoorAjarTimeout) => "CRITICAL: Freezer door open past critical timeout! Close door immediately!",
            (Severity::Critical, AlertCause::CompoundBreach) => "CRITICAL: Compound failure! Door open and critical temp breached!",
            (Severity::Warning, AlertCause::TemperatureBreach) => "WARNING: Freezer temp rising above threshold. Check seal/compressor.",
            (Severity::Warning, AlertCause::DoorAjarTimeout) => "WARNING: Freezer door left open beyond allowed limit.",
            (Severity::Warning, AlertCause::CompoundBreach) => "WARNING: Freezer door open and temperature rising.",
            _ => "Freezer parameters nominal.",
        }
    }
}

/// Stateful alert evaluator with hysteresis and root-cause tracking.
#[derive(Debug, Clone)]
pub struct AlertEngine {
    thresholds: Thresholds,
    current_severity: Severity,
    temp_severity: Severity,
    active_cause: Option<AlertCause>,
    alert_sent: bool,
}

impl AlertEngine {
    #[must_use]
    pub const fn new(thresholds: Thresholds) -> Self {
        Self {
            thresholds,
            current_severity: Severity::Healthy,
            temp_severity: Severity::Healthy,
            active_cause: None,
            alert_sent: false,
        }
    }

    fn eval_temp(&self, t: i16) -> Severity {
        match self.temp_severity {
            Severity::Critical => {
                if t < self.thresholds.critical_clear_cc {
                    if t < self.thresholds.warn_clear_cc {
                        Severity::Healthy
                    } else {
                        Severity::Warning
                    }
                } else {
                    Severity::Critical
                }
            }
            Severity::Warning => {
                if t > self.thresholds.critical_trigger_cc {
                    Severity::Critical
                } else if t < self.thresholds.warn_clear_cc {
                    Severity::Healthy
                } else {
                    Severity::Warning
                }
            }
            Severity::Healthy => {
                if t > self.thresholds.critical_trigger_cc {
                    Severity::Critical
                } else if t > self.thresholds.warn_trigger_cc {
                    Severity::Warning
                } else {
                    Severity::Healthy
                }
            }
        }
    }

    fn eval_door(&self, door: bool, secs: u16) -> Severity {
        if door {
            if secs >= self.thresholds.door_critical_secs {
                Severity::Critical
            } else if secs >= self.thresholds.door_warn_secs {
                Severity::Warning
            } else {
                Severity::Healthy
            }
        } else {
            Severity::Healthy
        }
    }

    pub fn evaluate(&mut self, r: &FreezerReading) -> Severity {
        self.temp_severity = self.eval_temp(r.temp.as_centi());
        let door_sev = self.eval_door(r.door_open, r.door_open_secs);

        let new_severity = core::cmp::max(self.temp_severity, door_sev);

        let new_cause = match (self.temp_severity > Severity::Healthy, door_sev > Severity::Healthy) {
            (true, true) => Some(AlertCause::CompoundBreach),
            (true, false) => Some(AlertCause::TemperatureBreach),
            (false, true) => Some(AlertCause::DoorAjarTimeout),
            (false, false) => None,
        };

        let escalated = new_severity > self.current_severity;
        let compound_upgrade = new_severity > Severity::Healthy
            && new_cause == Some(AlertCause::CompoundBreach)
            && self.active_cause != Some(AlertCause::CompoundBreach);

        if escalated || compound_upgrade {
            self.alert_sent = false;
        }

        self.current_severity = new_severity;
        self.active_cause = new_cause;
        self.current_severity
    }

    pub fn try_alert(&mut self, r: &FreezerReading) -> Option<AlertEvent> {
        if self.current_severity == Severity::Healthy || self.alert_sent {
            return None;
        }
        self.alert_sent = true;
        let cause = self.active_cause.unwrap_or(AlertCause::TemperatureBreach);

        Some(AlertEvent {
            timestamp_secs: r.timestamp_secs,
            message: AlertEvent::contextual_message(self.current_severity, cause),
            device_id: r.device_id.as_u32(),
            temp_centi_c: r.temp.as_centi(),
            door_open_secs: r.door_open_secs,
            health_score: r.health_score.as_u16(),
            severity: self.current_severity,
            cause,
            door_open: r.door_open,
        })
    }

    #[inline]
    #[must_use]
    pub fn process(&mut self, r: &FreezerReading) -> Option<AlertEvent> {
        self.evaluate(r);
        self.try_alert(r)
    }

    #[inline] #[must_use] pub const fn severity(&self) -> Severity { self.current_severity }
    #[inline] #[must_use] pub const fn active_cause(&self) -> Option<AlertCause> { self.active_cause }
    #[inline] #[must_use] pub const fn is_alert_dispatched(&self) -> bool { self.alert_sent }
    #[inline] #[must_use] pub const fn thresholds(&self) -> &Thresholds { &self.thresholds }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading_at(temp_cc: i16, door: bool, door_secs: u16) -> FreezerReading {
        FreezerReading {
            temp: TempCentiC::from_centi(temp_cc),
            door_open: door,
            door_open_secs: door_secs,
            ..Default::default()
        }
    }

    #[test]
    fn door_alert_clears_instantly_on_close_even_if_temp_is_not_below_clear_threshold() {
        let mut engine = AlertEngine::new(Thresholds::home_freezer());

        // Freezer at -16°C (Healthy: below -15°C trigger, but above -17°C clear)
        // Door open for 120s triggers Warning
        assert_eq!(engine.evaluate(&reading_at(-1600, true, 120)), Severity::Warning);
        assert_eq!(engine.active_cause(), Some(AlertCause::DoorAjarTimeout));

        // Close door -> MUST return to Healthy immediately!
        assert_eq!(engine.evaluate(&reading_at(-1600, false, 0)), Severity::Healthy);
        assert_eq!(engine.active_cause(), None);
    }

    #[test]
    fn warning_upgrades_to_compound_when_door_subsequently_opens() {
        let mut engine = AlertEngine::new(Thresholds::home_freezer());

        // 1. Temp warning triggers
        let r1 = reading_at(-1400, false, 0);
        assert!(engine.process(&r1).is_some());
        assert_eq!(engine.active_cause(), Some(AlertCause::TemperatureBreach));

        // 2. Door also stays open for 120s -> MUST re-alert as Compound!
        let r2 = reading_at(-1400, true, 120);
        let alert = engine.process(&r2).expect("Must re-alert on compound breach");
        assert_eq!(alert.cause, AlertCause::CompoundBreach);
    }
}