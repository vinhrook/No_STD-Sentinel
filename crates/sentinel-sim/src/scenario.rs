use std::str::FromStr;
use crate::physics::ThermalModel;
use crate::rng::FastRng;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenarioKind {
    Normal,
    ThermalRunaway,
    DoorAjar,
    DeepFreeze,
    ErraticNoise,
    Chaos,
}

impl FromStr for ScenarioKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "normal" => Ok(Self::Normal),
            "runaway" | "thermal-runaway" | "overheat" => Ok(Self::ThermalRunaway),
            "door" | "door-ajar" => Ok(Self::DoorAjar),
            "deep-freeze" | "ultralow" => Ok(Self::DeepFreeze),
            "erratic" | "noise" => Ok(Self::ErraticNoise),
            "chaos" => Ok(Self::Chaos),
            _ => Err(format!(
                "unknown scenario '{s}'. Valid: normal, runaway, door-ajar, deep-freeze, erratic, chaos"
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScenarioController {
    pub kind: ScenarioKind,
    pub elapsed_secs: f64,
    pub rng: FastRng,
}

impl ScenarioController {
    pub fn new(kind: ScenarioKind) -> Self {
        Self {
            kind,
            elapsed_secs: 0.0,
            rng: FastRng::default(),
        }
    }

    pub fn init_model(&self) -> ThermalModel {
        match self.kind {
            ScenarioKind::Normal => ThermalModel::new_freezer(-20.0, 24.0),
            ScenarioKind::ThermalRunaway => {
                let mut m = ThermalModel::new_freezer(-15.0, 36.0);
                m.compressor_power = 0.0;
                m.passive_leakage_rate = 0.025;
                m
            }
            ScenarioKind::DoorAjar => {
                let mut m = ThermalModel::new_freezer(-19.0, 25.0);
                m.door_open = true;
                m
            }
            ScenarioKind::DeepFreeze => {
                let mut m = ThermalModel::new_freezer(-80.0, 22.0);
                m.compressor_power = 0.080;
                m.compressor_on_threshold_c = -78.0;
                m.compressor_off_threshold_c = -82.0;
                m
            }
            ScenarioKind::ErraticNoise => ThermalModel::new_freezer(-18.0, 24.0),
            ScenarioKind::Chaos => ThermalModel::new_freezer(-20.0, 26.0),
        }
    }

    pub fn apply_step(&mut self, model: &mut ThermalModel, dt_secs: f64) {
        self.elapsed_secs += dt_secs;

        match self.kind {
            ScenarioKind::Normal => {
                let cycle_phase = (self.elapsed_secs as u64) % 120;
                model.door_open = cycle_phase >= 50 && cycle_phase <= 58;
            }
            ScenarioKind::ThermalRunaway => {
                model.compressor_active = false;
                model.compressor_power = 0.0;
            }
            ScenarioKind::DoorAjar => {
                model.door_open = true;
            }
            ScenarioKind::DeepFreeze => {
                model.door_open = false;
            }
            ScenarioKind::ErraticNoise => {
                let spike = self.rng.gen_range_f64(-1.5, 1.5);
                model.current_temp_c += spike;
            }
            ScenarioKind::Chaos => {
                if self.rng.gen_bool(0.05) {
                    model.door_open = !model.door_open;
                }
                if self.rng.gen_bool(0.02) {
                    model.compressor_active = false;
                }
            }
        }

        model.step(dt_secs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenario_from_str() {
        assert_eq!("normal".parse::<ScenarioKind>().unwrap(), ScenarioKind::Normal);
        assert_eq!("runaway".parse::<ScenarioKind>().unwrap(), ScenarioKind::ThermalRunaway);
        assert_eq!("door-ajar".parse::<ScenarioKind>().unwrap(), ScenarioKind::DoorAjar);
        assert_eq!("deep-freeze".parse::<ScenarioKind>().unwrap(), ScenarioKind::DeepFreeze);
        assert_eq!("chaos".parse::<ScenarioKind>().unwrap(), ScenarioKind::Chaos);
    }

    #[test]
    fn test_runaway_heats_up() {
        let mut controller = ScenarioController::new(ScenarioKind::ThermalRunaway);
        let mut model = controller.init_model();
        let initial_temp = model.current_temp_c;

        for _ in 0..100 {
            controller.apply_step(&mut model, 1.0);
        }

        assert!(
            model.current_temp_c > initial_temp,
            "Thermal runaway must cause temperature to rise"
        );
    }
}
