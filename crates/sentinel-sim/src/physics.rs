use crate::rng::FastRng;

pub const DS18B20_RESOLUTION_C: f64 = 0.0625;

#[derive(Debug, Clone)]
pub struct ThermalModel {
    pub current_temp_c: f64,
    pub ambient_temp_c: f64,
    pub target_temp_c: f64,
    pub compressor_on_threshold_c: f64,
    pub compressor_off_threshold_c: f64,
    pub compressor_active: bool,
    pub door_open: bool,
    pub door_open_secs: u16,
    pub passive_leakage_rate: f64,
    pub compressor_power: f64,
    pub door_ingress_rate: f64,
    pub rng: FastRng,
}

impl Default for ThermalModel {
    fn default() -> Self {
        Self::new_freezer(-20.0, 24.0)
    }
}

impl ThermalModel {
    pub fn new_freezer(target_c: f64, ambient_c: f64) -> Self {
        Self {
            current_temp_c: target_c,
            ambient_temp_c: ambient_c,
            target_temp_c: target_c,
            compressor_on_threshold_c: target_c + 2.5,
            compressor_off_threshold_c: target_c - 1.5,
            compressor_active: false,
            door_open: false,
            door_open_secs: 0,
            passive_leakage_rate: 0.0006,
            compressor_power: 0.060,
            door_ingress_rate: 0.080,
            rng: FastRng::default(),
        }
    }

    pub fn step(&mut self, dt_secs: f64) {
        if self.current_temp_c >= self.compressor_on_threshold_c {
            self.compressor_active = true;
        } else if self.current_temp_c <= self.compressor_off_threshold_c {
            self.compressor_active = false;
        }

        let mut d_temp = 0.0;

        let ambient_delta = self.ambient_temp_c - self.current_temp_c;
        d_temp += self.passive_leakage_rate * ambient_delta * dt_secs;

        if self.compressor_active {
            d_temp -= self.compressor_power * dt_secs;
        }

        if self.door_open {
            d_temp += self.door_ingress_rate * ambient_delta * dt_secs;
            self.door_open_secs = self.door_open_secs.saturating_add(dt_secs.round() as u16);
        } else {
            self.door_open_secs = 0;
        }

        let thermal_noise = self.rng.gen_range_f64(-0.015, 0.015);
        d_temp += thermal_noise;

        self.current_temp_c += d_temp;
    }

    pub fn read_primary_centi_c(&self) -> i16 {
        quantize_ds18b20(self.current_temp_c)
    }

    pub fn read_ambient_centi_c(&self) -> i16 {
        let condenser_boost = if self.compressor_active { 1.8 } else { 0.0 };
        quantize_ds18b20(self.ambient_temp_c + condenser_boost)
    }
}

#[inline]
pub fn quantize_ds18b20(temp_c: f64) -> i16 {
    let steps = (temp_c / DS18B20_RESOLUTION_C).round();
    let quantized_c = steps * DS18B20_RESOLUTION_C;
    (quantized_c * 100.0).round() as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quantization_precision() {
        assert_eq!(quantize_ds18b20(-20.0), -2000);
        assert_eq!(quantize_ds18b20(0.0625), 6);
    }

    #[test]
    fn test_thermostat_cooling_cycle() {
        let mut model = ThermalModel::new_freezer(-20.0, 25.0);
        model.current_temp_c = -16.0;
        model.step(1.0);
        assert!(model.compressor_active, "Compressor must turn on when warm");

        for _ in 0..300 {
            model.step(1.0);
            if !model.compressor_active {
                break;
            }
        }
        assert!(!model.compressor_active, "Compressor must disengage once cooled");
    }
}
