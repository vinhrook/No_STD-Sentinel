#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::telemetry::{FreezerReading, HealthScore, TempCentiC};

pub const DEFAULT_TARGET_TEMP: TempCentiC = TempCentiC::from_centi(-2_000);
pub const MAX_HEALTH_POINTS: u16 = 1_000;
pub const DEFAULT_WIFI_RSSI_THRESHOLD: i8 = -75;
pub const DEFAULT_PENALTY: u16 = 5;
pub const DEFAULT_TEMP_PENALTY_PER_DEGREE: u16 = 50;


pub const DEFAULT_DOOR_PENALTY_PER_SEC: u16 = 10;

pub const DEFAULT_DOOR_PENALTY_MAX: u16 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HealthConfig {
    pub target_temp: TempCentiC,
    pub temp_penalty_per_deg: u16,
    pub door_penalty_per_sec: u16,
    pub door_penalty_max: u16,
    pub wifi_rssi_threshold: i8,
    pub wifi_penalty: u16,
    pub penalize_overcooling: bool,
}

impl HealthConfig {
    pub const HOME_FREEZER: Self = Self {
        target_temp: DEFAULT_TARGET_TEMP,
        temp_penalty_per_deg: DEFAULT_TEMP_PENALTY_PER_DEGREE,
        door_penalty_per_sec: DEFAULT_DOOR_PENALTY_PER_SEC,
        door_penalty_max: DEFAULT_DOOR_PENALTY_MAX,
        wifi_rssi_threshold: DEFAULT_WIFI_RSSI_THRESHOLD,
        wifi_penalty: DEFAULT_PENALTY,
        penalize_overcooling: false,
    };

    pub const ULTRA_LOW_COLD_STORAGE: Self = Self {
        target_temp: TempCentiC::from_centi(-8_000),
        temp_penalty_per_deg: 100,
        door_penalty_per_sec: 25,
        door_penalty_max: 500,
        wifi_rssi_threshold: -70,
        wifi_penalty: 10,
        penalize_overcooling: false,
    };
}

impl Default for HealthConfig {
    #[inline]
    fn default() -> Self {
        Self::HOME_FREEZER
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HealthBreakdown {
    pub score: HealthScore,
    pub temp_penalty: u16,
    pub door_penalty: u16,
    pub wifi_penalty: u16,
    pub total_penalty: u16,
}

impl HealthBreakdown {
    #[inline]
    #[must_use]
    pub const fn is_perfect(&self) -> bool {
        self.total_penalty == 0
    }
}

#[inline]
#[must_use]
pub const fn calculate_breakdown(reading: &FreezerReading) -> HealthBreakdown {
    calculate_breakdown_with_config(reading, HealthConfig::HOME_FREEZER)
}

#[inline]
#[must_use]
pub const fn calculate(reading: &FreezerReading) -> HealthScore {
    calculate_breakdown(reading).score
}

#[must_use]
pub const fn calculate_breakdown_with_config(
    reading: &FreezerReading,
    config: HealthConfig,
) -> HealthBreakdown {
    calculate_raw(
        reading.temp,
        reading.door_open,
        reading.door_open_secs,
        reading.rssi_dbm,
        config,
    )
}

#[must_use]
pub const fn calculate_raw(
    temp: TempCentiC,
    door_open: bool,
    door_open_secs: u16,
    rssi_dbm: i8,
    config: HealthConfig,
) -> HealthBreakdown {
    let reading_centi = temp.as_centi() as i32;
    let target_centi = config.target_temp.as_centi() as i32;

    let diff_centi = reading_centi - target_centi;
    let effective_deviation = if config.penalize_overcooling {
        diff_centi.unsigned_abs()
    } else if diff_centi > 0 {
        diff_centi as u32
    } else {
        0
    };

    let temp_penalty_u32 =
        (effective_deviation.saturating_mul(config.temp_penalty_per_deg as u32)) / 100;
    let temp_penalty = if temp_penalty_u32 > u16::MAX as u32 {
        u16::MAX
    } else {
        temp_penalty_u32 as u16
    };

    let door_penalty = if door_open {
        let raw = (door_open_secs as u32).saturating_mul(config.door_penalty_per_sec as u32);
        let cap = config.door_penalty_max as u32;
        if raw < cap {
            raw as u16
        } else {
            config.door_penalty_max
        }
    } else {
        0
    };

   
    let wifi_penalty = if rssi_dbm < config.wifi_rssi_threshold {
        config.wifi_penalty
    } else {
        0
    };

    let total_penalty = temp_penalty
        .saturating_add(door_penalty)
        .saturating_add(wifi_penalty);

    let raw_score = MAX_HEALTH_POINTS.saturating_sub(total_penalty);
    let score = HealthScore::new_saturating(raw_score);

    HealthBreakdown {
        score,
        temp_penalty,
        door_penalty,
        wifi_penalty,
        total_penalty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::TempCentiC;

    #[test]
    fn perfect_nominal_score() {
        let r = FreezerReading {
            temp: TempCentiC::from_centi(-2000),
            door_open: false,
            door_open_secs: 0,
            rssi_dbm: -50,
            ..Default::default()
        };
        let b = calculate_breakdown(&r);
        assert_eq!(b.score.as_u16(), 1000);
        assert!(b.is_perfect());
        assert_eq!(b.total_penalty, 0);
        assert_eq!(calculate(&r).as_u16(), 1000);
    }

    #[test]
    fn two_degrees_warm_incurs_100_point_penalty() {
        let r = FreezerReading {
            temp: TempCentiC::from_centi(-1800),
            ..Default::default()
        };
        let b = calculate_breakdown(&r);
        assert_eq!(b.temp_penalty, 100);
        assert_eq!(b.score.as_u16(), 900);
        assert_eq!(calculate(&r).as_u16(), 900);
    }

    #[test]
    fn door_open_at_max_cap() {
        let r = FreezerReading {
            temp: TempCentiC::from_centi(-2000),
            door_open: true,
            door_open_secs: 30,
            ..Default::default()
        };
        let b = calculate_breakdown(&r);
        assert_eq!(b.door_penalty, 300);
        assert_eq!(b.score.as_u16(), 700);

        let r_longer = FreezerReading {
            door_open_secs: 60,
            ..r
        };
        assert_eq!(calculate(&r_longer).as_u16(), 700);
    }

    #[test]
    fn door_closed_ignores_residual_seconds() {
        let r = FreezerReading {
            temp: TempCentiC::from_centi(-2000),
            door_open: false,
            door_open_secs: 120,
            ..Default::default()
        };
        assert_eq!(calculate_breakdown(&r).door_penalty, 0);
    }

    #[test]
    fn catastrophic_thaw_saturates_to_zero() {
        let r = FreezerReading {
            temp: TempCentiC::from_centi(500),
            ..Default::default()
        };
        let b = calculate_breakdown(&r);
        assert_eq!(b.score.as_u16(), 0);
        assert_eq!(b.temp_penalty, 1250);
        assert_eq!(calculate(&r).as_u16(), 0);
    }

    #[test]
    fn weak_wifi_penalty() {
        let r = FreezerReading {
            temp: TempCentiC::from_centi(-2000),
            rssi_dbm: -82,
            ..Default::default()
        };
        let b = calculate_breakdown(&r);
        assert_eq!(b.wifi_penalty, 5);
        assert_eq!(b.score.as_u16(), 995);
        assert_eq!(calculate(&r).as_u16(), 995);
    }

    #[test]
    fn overcooling_is_not_penalized_in_home_freezer() {
        let r = FreezerReading {
            temp: TempCentiC::from_centi(-2500),
            ..Default::default()
        };
        let b = calculate_breakdown(&r);
        assert_eq!(b.temp_penalty, 0);
        assert_eq!(b.score.as_u16(), 1000);
    }

    #[test]
    fn corrupt_sensor_inputs_never_panic() {
        let b = calculate_raw(
            TempCentiC::from_centi(i16::MAX),
            true,
            u16::MAX,
            i8::MIN,
            HealthConfig::HOME_FREEZER,
        );
        assert_eq!(b.score.as_u16(), 0);

        let b_min = calculate_raw(
            TempCentiC::from_centi(i16::MIN),
            false,
            0,
            i8::MAX,
            HealthConfig::HOME_FREEZER,
        );
        assert_eq!(b_min.score.as_u16(), 1000);
    }

    #[test]
    const fn const_eval_verification() {
        const SCORE: HealthScore = calculate(&FreezerReading::PLACEHOLDER);
        assert!(SCORE.as_u16() == 1000);
    }
}