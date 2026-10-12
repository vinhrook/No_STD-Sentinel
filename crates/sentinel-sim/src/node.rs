use serde::{Deserialize, Serialize};
use sentinel_core::health::{calculate_raw, HealthConfig};
use sentinel_core::telemetry::{DeviceId, TempCentiC};
use crate::physics::ThermalModel;
use crate::rng::FastRng;
use crate::scenario::{ScenarioController, ScenarioKind};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MqttWirePayload {
    pub d: u32,
    pub ts: u64,
    pub tc: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ac: Option<i16>,
    #[serde(rename = "do")]
    pub door_open: bool,
    pub ds: u16,
    pub hs: u16,
    pub rs: i8,
    pub fv: u32,
}

pub struct VirtualNode {
    pub device_id: DeviceId,
    pub model: ThermalModel,
    pub scenario: ScenarioController,
    pub uptime_secs: u64,
    pub base_rssi: i8,
    pub firmware_version: u32,
    pub health_config: HealthConfig,
    pub rng: FastRng,
}

impl VirtualNode {
    pub fn new(id: u32, scenario_kind: ScenarioKind) -> Self {
        let scenario = ScenarioController::new(scenario_kind);
        let model = scenario.init_model();
        let health_config = match scenario_kind {
            ScenarioKind::DeepFreeze => HealthConfig::ULTRA_LOW_COLD_STORAGE,
            _ => HealthConfig::HOME_FREEZER,
        };

        Self {
            device_id: DeviceId::new(id),
            model,
            scenario,
            uptime_secs: 0,
            base_rssi: -58,
            firmware_version: 0x0001_0000,
            health_config,
            rng: FastRng::default(),
        }
    }

    pub fn tick(&mut self, dt_secs: f64) -> MqttWirePayload {
        self.scenario.apply_step(&mut self.model, dt_secs);
        self.uptime_secs = self.uptime_secs.saturating_add(dt_secs.round() as u64);

        let primary_tc = self.model.read_primary_centi_c();
        let ambient_tc = self.model.read_ambient_centi_c();

        let rssi_noise: i8 = self.rng.gen_range_i8(-3, 3);
        let current_rssi = (self.base_rssi + rssi_noise).clamp(-95, -30);

        let temp_fixed = TempCentiC::from_centi(primary_tc);
        let breakdown = calculate_raw(
            temp_fixed,
            self.model.door_open,
            self.model.door_open_secs,
            current_rssi,
            self.health_config,
        );
        let health_pts = breakdown.score.as_u16();

        MqttWirePayload {
            d: self.device_id.as_u32(),
            ts: self.uptime_secs,
            tc: primary_tc,
            ac: Some(ambient_tc),
            door_open: self.model.door_open,
            ds: self.model.door_open_secs,
            hs: health_pts,
            rs: current_rssi,
            fv: self.firmware_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_tick_generates_valid_payload() {
        let mut node = VirtualNode::new(0xA101_0001, ScenarioKind::Normal);
        let payload = node.tick(1.0);

        assert_eq!(payload.d, 0xA101_0001);
        assert_eq!(payload.ts, 1);
        assert!(payload.hs > 900, "Nominal freezer node should have high health score");
        assert!(payload.tc < 0, "Nominal freezer node should be sub-zero");
        assert!(payload.ac.is_some(), "Ambient reading should be present");
    }

    #[test]
    fn test_json_wire_compatibility() {
        let mut node = VirtualNode::new(0xBEEF_0001, ScenarioKind::Normal);
        let payload = node.tick(1.0);
        let json_str = serde_json::to_string(&payload).expect("Serialization must succeed");

        assert!(json_str.contains(&format!("\"d\":{}", 0xBEEF_0001u32)));
        assert!(json_str.contains("\"do\":false"));
        assert!(json_str.contains("\"tc\":"));
    }
}
