use rumqttc::{AsyncClient, Event, Incoming, MqttOptions, QoS};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use sentinel_core::alert::{AlertCause, AlertEvent, Severity};
use sentinel_core::telemetry::{
    DeviceId, FirmwareVersion, FreezerReading, HealthScore, TempCentiC,
};
use crate::AppState;

#[derive(Debug)]
pub enum MqttSubscriberError {
    ClientInit(rumqttc::ClientError),
    ConnectionDropped,
}

impl fmt::Display for MqttSubscriberError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ClientInit(err) => write!(f, "MQTT client initialization fault: {err}"),
            Self::ConnectionDropped => write!(f, "MQTT event loop terminated unexpectedly"),
        }
    }
}

impl std::error::Error for MqttSubscriberError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ClientInit(err) => Some(err),
            Self::ConnectionDropped => None,
        }
    }
}

#[derive(Debug)]
pub enum IngestionError {
    PayloadMalformed(serde_json::Error),
    Database(crate::db::DbError),
}

impl fmt::Display for IngestionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PayloadMalformed(err) => write!(f, "malformed MQTT JSON payload: {err}"),
            Self::Database(err) => write!(f, "database persistence failure: {err}"),
        }
    }
}

impl std::error::Error for IngestionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PayloadMalformed(err) => Some(err),
            Self::Database(err) => Some(err),
        }
    }
}

impl From<serde_json::Error> for IngestionError {
    fn from(err: serde_json::Error) -> Self {
        Self::PayloadMalformed(err)
    }
}

impl From<crate::db::DbError> for IngestionError {
    fn from(err: crate::db::DbError) -> Self {
        Self::Database(err)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct MqttWirePayload {
    #[serde(alias = "device_id")]
    pub d: u32,
    #[serde(alias = "timestamp_secs")]
    pub ts: u64,
    #[serde(alias = "temp")]
    pub tc: i16,
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "ambient_temp")]
    pub ac: Option<i16>,
    #[serde(rename = "do", alias = "door_open")]
    pub door_open: bool,
    #[serde(alias = "door_open_secs")]
    pub ds: u16,
    #[serde(alias = "health_score")]
    pub hs: u16,
    #[serde(alias = "rssi_dbm")]
    pub rs: i8,
    #[serde(alias = "firmware_version")]
    pub fv: u32,
}

impl From<MqttWirePayload> for FreezerReading {
    fn from(p: MqttWirePayload) -> Self {
        Self {
            device_id: DeviceId::new(p.d),
            timestamp_secs: p.ts,
            temp: TempCentiC::from_centi(p.tc),
            door_open: p.door_open,
            door_open_secs: p.ds,
            health_score: HealthScore::new_saturating(p.hs),
            rssi_dbm: p.rs,
            firmware_version: FirmwareVersion::try_from(p.fv).unwrap_or(FirmwareVersion::new(1, 0, 0)),
        }
    }
}

impl From<&FreezerReading> for MqttWirePayload {
    fn from(r: &FreezerReading) -> Self {
        Self {
            d: r.device_id.as_u32(),
            ts: r.timestamp_secs,
            tc: r.temp.as_centi(),
            ac: None,
            door_open: r.door_open,
            ds: r.door_open_secs,
            hs: r.health_score.as_u16(),
            rs: r.rssi_dbm,
            fv: r.firmware_version.as_u32(),
        }
    }
}

pub fn process_telemetry_message(
    state: &AppState,
    payload: &[u8],
) -> Result<FreezerReading, IngestionError> {
    let wire = serde_json::from_slice::<MqttWirePayload>(payload)?;
    let reading = FreezerReading::from(wire.clone());

    state.db.upsert_device(reading.device_id.as_u32())?;
    state.db.insert_reading(&reading)?;

    if let Ok(json) = serde_json::to_string(&wire) {
        let _ = state.live_tx.send(json);
    }

    Ok(reading)
}

pub async fn run(state: Arc<AppState>) -> Result<(), MqttSubscriberError> {
    let mqtt_config = &state.config.mqtt;

    let mut mqtt_options = MqttOptions::new(
        &mqtt_config.client_id,
        &mqtt_config.host,
        mqtt_config.port,
    );
    mqtt_options.set_keep_alive(Duration::from_secs(30));
    mqtt_options.set_clean_session(true);

    let (client, mut eventloop) = AsyncClient::new(mqtt_options, 128);

    tracing::info!(
        host = %mqtt_config.host,
        port = mqtt_config.port,
        client_id = %mqtt_config.client_id,
        topic = %mqtt_config.topic_filter,
        "MQTT subscriber client configured. Launching ingestion event loop"
    );
    let mut high_in_alert = false;
    let mut last_high_alert_time: Option<Instant> = None;

    let mut low_in_alert = false;
    let mut last_low_alert_time: Option<Instant> = None;

    let reminder_interval = Duration::from_secs(600);

    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Incoming::ConnAck(_))) => {
                tracing::info!(
                    topic = %mqtt_config.topic_filter,
                    "Broker connected / re-established. Subscribing to telemetry filter"
                );
                if let Err(err) = client.subscribe(&mqtt_config.topic_filter, QoS::AtLeastOnce).await {
                    tracing::error!(error = %err, "Failed to register topic subscription");
                }
            }

            Ok(Event::Incoming(Incoming::Publish(publish))) => {
                match process_telemetry_message(&state, &publish.payload) {
                    Ok(reading) => {
                        let temp_centi = reading.temp.as_centi();

                        if temp_centi >= 3500 {
                            let should_alert = match last_high_alert_time {
                                None => true,
                                Some(t) => t.elapsed() >= reminder_interval,
                            };

                            if should_alert {
                                let is_first = !high_in_alert;
                                high_in_alert = true;
                                last_high_alert_time = Some(Instant::now());

                                let message = if is_first {
                                    "CRITICAL: High temperature limit exceeded! (+35°C breached)"
                                } else {
                                    "REMINDER: Temperature is STILL above +35°C! Urgent action required."
                                };

                                let alert_config = state.config.alerts.clone();
                                let alert = AlertEvent {
                                    timestamp_secs: reading.timestamp_secs,
                                    message,
                                    device_id: reading.device_id.as_u32(),
                                    temp_centi_c: temp_centi,
                                    door_open_secs: reading.door_open_secs,
                                    health_score: reading.health_score.as_u16(),
                                    severity: Severity::Critical,
                                    cause: AlertCause::TemperatureBreach,
                                    door_open: reading.door_open,
                                };

                                tokio::spawn(async move {
                                    crate::alerts::dispatch(&alert_config, &alert).await;
                                });
                            }
                        } else if temp_centi < 3300 && high_in_alert {
                            high_in_alert = false;
                            last_high_alert_time = None;

                            let alert_config = state.config.alerts.clone();
                            let alert = AlertEvent {
                                timestamp_secs: reading.timestamp_secs,
                                message: "RECOVERY: Temperature restored to safe levels (< 33°C).",
                                device_id: reading.device_id.as_u32(),
                                temp_centi_c: temp_centi,
                                door_open_secs: reading.door_open_secs,
                                health_score: reading.health_score.as_u16(),
                                severity: Severity::Healthy,
                                cause: AlertCause::TemperatureBreach,
                                door_open: reading.door_open,
                            };

                            tokio::spawn(async move {
                                crate::alerts::dispatch(&alert_config, &alert).await;
                            });
                        }

                        if temp_centi <= 700 {
                            let should_alert = match last_low_alert_time {
                                None => true,
                                Some(t) => t.elapsed() >= reminder_interval,
                            };

                            if should_alert {
                                let is_first = !low_in_alert;
                                low_in_alert = true;
                                last_low_alert_time = Some(Instant::now());

                                let message = if is_first {
                                    "COLD ALERT: Low temperature reached! (7°C or lower breached)"
                                } else {
                                    "COLD REMINDER: Temperature is STILL at or below 7°C."
                                };

                                let alert_config = state.config.alerts.clone();
                                let alert = AlertEvent {
                                    timestamp_secs: reading.timestamp_secs,
                                    message,
                                    device_id: reading.device_id.as_u32(),
                                    temp_centi_c: temp_centi,
                                    door_open_secs: reading.door_open_secs,
                                    health_score: reading.health_score.as_u16(),
                                    severity: Severity::Warning,
                                    cause: AlertCause::TemperatureBreach,
                                    door_open: reading.door_open,
                                };

                                tokio::spawn(async move {
                                    crate::alerts::dispatch(&alert_config, &alert).await;
                                });
                            }
                        } else if temp_centi >= 1000 && low_in_alert {
                            low_in_alert = false;
                            last_low_alert_time = None;

                            let alert_config = state.config.alerts.clone();
                            let alert = AlertEvent {
                                timestamp_secs: reading.timestamp_secs,
                                message: "RECOVERY: Temperature warmed back to safe levels (> 10°C).",
                                device_id: reading.device_id.as_u32(),
                                temp_centi_c: temp_centi,
                                door_open_secs: reading.door_open_secs,
                                health_score: reading.health_score.as_u16(),
                                severity: Severity::Healthy,
                                cause: AlertCause::TemperatureBreach,
                                door_open: reading.door_open,
                            };

                            tokio::spawn(async move {
                                crate::alerts::dispatch(&alert_config, &alert).await;
                            });
                        }

                        tracing::debug!(
                            node = %reading.device_id,
                            temp = format_args!("{}°C", reading.temp),
                            score = reading.health_score.as_u16(),
                            rssi = reading.rssi_dbm,
                            "Telemetry ingested"
                        );
                    }
                    Err(err) => {
                        tracing::warn!(
                            topic = %publish.topic,
                            error = %err,
                            "MQTT telemetry ingestion failed"
                        );
                    }
                }
            }

            Ok(_) => {}

            Err(err) => {
                tracing::warn!(error = %err, "MQTT transport error, awaiting automatic reconnect");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GatewayConfig;
    use crate::db::Database;
    use tokio::sync::broadcast;
    use tokio::sync::broadcast::error::TryRecvError;

    fn setup_test_state() -> (Arc<AppState>, broadcast::Receiver<String>) {
        let db = Database::open_in_memory().expect("in-memory test database");
        db.run_migrations().expect("migrations applied");
        let (live_tx, live_rx) = broadcast::channel(32);
        let state = Arc::new(AppState {
            db,
            live_tx,
            config: GatewayConfig::default(),
        });
        (state, live_rx)
    }

    #[test]
    fn test_wire_payload_deserialization_compact_keys() {
        let json = br#"{
            "d": 3735928559,
            "ts": 1710000000,
            "tc": -1850,
            "do": true,
            "ds": 12,
            "hs": 950,
            "rs": -65,
            "fv": 10203
        }"#;

        let payload: MqttWirePayload = serde_json::from_slice(json).expect("valid wire payload");
        assert_eq!(payload.d, 0xDEAD_BEEF);
        assert_eq!(payload.ts, 1_710_000_000);
        assert_eq!(payload.tc, -1850);
        assert!(payload.door_open);
        assert_eq!(payload.ds, 12);
        assert_eq!(payload.hs, 950);
        assert_eq!(payload.rs, -65);
        assert_eq!(payload.fv, 10203);
    }

    #[test]
    fn test_wire_payload_serialization_roundtrip() {
        let original = MqttWirePayload {
            d: 0xCAFE_BABE,
            ts: 1_710_000_500,
            tc: -2100,
            ac: Some(2450),
            door_open: false,
            ds: 0,
            hs: 990,
            rs: -55,
            fv: 20100,
        };

        let serialized = serde_json::to_string(&original).expect("serialization succeeds");
        assert!(serialized.contains(r#""do":false"#));
        assert!(!serialized.contains(r#""door_open""#));
        assert!(serialized.contains(r#""ac":2450"#));

        let deserialized: MqttWirePayload =
            serde_json::from_str(&serialized).expect("deserialization succeeds");
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_wire_payload_to_freezer_reading_mapping() {
        let payload = MqttWirePayload {
            d: 0x1234_5678,
            ts: 1_720_000_000,
            tc: -1925,
            ac: Some(2500),
            door_open: true,
            ds: 45,
            hs: 920,
            rs: -70,
            fv: 10203,
        };

        let reading = FreezerReading::from(payload);
        assert_eq!(reading.device_id.as_u32(), 0x1234_5678);
        assert_eq!(reading.timestamp_secs, 1_720_000_000);
        assert_eq!(reading.temp.as_centi(), -1925);
        assert!(reading.door_open);
        assert_eq!(reading.door_open_secs, 45);
        assert_eq!(reading.health_score.as_u16(), 920);
        assert_eq!(reading.rssi_dbm, -70);
        assert_eq!(reading.firmware_version.parts(), (1, 2, 3));
    }

    #[test]
    fn test_freezer_reading_to_wire_payload_mapping() {
        let reading = FreezerReading {
            device_id: DeviceId::new(0xAABB_CCDD),
            timestamp_secs: 1_730_000_000,
            temp: TempCentiC::from_centi(-2050),
            door_open: false,
            door_open_secs: 0,
            health_score: HealthScore::new_saturating(980),
            rssi_dbm: -58,
            firmware_version: FirmwareVersion::new(2, 4, 1),
        };

        let wire = MqttWirePayload::from(&reading);
        assert_eq!(wire.d, 0xAABB_CCDD);
        assert_eq!(wire.ts, 1_730_000_000);
        assert_eq!(wire.tc, -2050);
        assert_eq!(wire.ac, None);
        assert!(!wire.door_open);
        assert_eq!(wire.ds, 0);
        assert_eq!(wire.hs, 980);
        assert_eq!(wire.rs, -58);
        assert_eq!(wire.fv, reading.firmware_version.as_u32());
    }

    #[test]
    fn test_wire_payload_conversion_saturation_and_fallback() {
        let payload = MqttWirePayload {
            d: 0x1111_2222,
            ts: 1_710_000_000,
            tc: 0,
            ac: None,
            door_open: false,
            ds: 0,
            hs: 5000,
            rs: -80,
            fv: u32::MAX,
        };

        let reading = FreezerReading::from(payload);
        assert_eq!(reading.health_score.as_u16(), 1000);
        assert_eq!(reading.firmware_version.parts(), (429496, 72, 95));
        assert_eq!(reading.temp.as_centi(), 0);
    }

    #[test]
    fn test_wire_payload_deserialization_errors() {
        let corrupt = b"not a json object";
        assert!(serde_json::from_slice::<MqttWirePayload>(corrupt).is_err());

        let missing_field = br#"{"d":1,"ts":10,"do":false,"ds":0,"hs":100,"rs":-50,"fv":10000}"#;
        assert!(serde_json::from_slice::<MqttWirePayload>(missing_field).is_err());

        let invalid_type = br#"{"d":"device-1","ts":10,"tc":-1800,"do":false,"ds":0,"hs":100,"rs":-50,"fv":10000}"#;
        assert!(serde_json::from_slice::<MqttWirePayload>(invalid_type).is_err());
    }

    #[test]
    fn test_subscriber_error_display_and_source() {
        let dropped = MqttSubscriberError::ConnectionDropped;
        assert_eq!(
            format!("{dropped}"),
            "MQTT event loop terminated unexpectedly"
        );
        assert!(std::error::Error::source(&dropped).is_none());
    }

    #[test]
    fn test_ingestion_error_display_and_source() {
        let parse_err = serde_json::from_slice::<MqttWirePayload>(b"bad").unwrap_err();
        let err = IngestionError::from(parse_err);
        assert!(format!("{err}").contains("malformed MQTT JSON payload"));
        assert!(std::error::Error::source(&err).is_some());

        let db_err = crate::db::DbError::LockPoisoned;
        let err2 = IngestionError::from(db_err);
        assert!(format!("{err2}").contains("database persistence failure"));
        assert!(std::error::Error::source(&err2).is_some());

        let sqlite_err = crate::db::DbError::Sqlite(rusqlite::Error::QueryReturnedNoRows);
        let err3 = IngestionError::from(sqlite_err);
        assert!(std::error::Error::source(&err3).is_some());
    }

    #[test]
    fn test_process_telemetry_message_pipeline_success() {
        let (state, mut live_rx) = setup_test_state();

        let payload = br#"{
            "d": 3735928559,
            "ts": 1710000000,
            "tc": -1875,
            "do": false,
            "ds": 0,
            "hs": 995,
            "rs": -60,
            "fv": 10000
        }"#;

        let result = process_telemetry_message(&state, payload);
        assert!(result.is_ok());
        let reading = result.unwrap();

        assert_eq!(reading.device_id.as_u32(), 0xDEAD_BEEF);
        assert_eq!(reading.temp.as_centi(), -1875);

        let devices = state.db.get_devices().expect("devices query");
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device_id, 0xDEAD_BEEF);

        let readings = state.db.get_latest_readings(0xDEAD_BEEF, 10).expect("readings query");
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].temp.as_centi(), -1875);
        assert_eq!(readings[0].health_score.as_u16(), 995);

        let broadcast_frame = live_rx.try_recv().expect("broadcast message received");
        let parsed_broadcast: MqttWirePayload =
            serde_json::from_str(&broadcast_frame).expect("valid reading json");
        assert_eq!(parsed_broadcast.d, 0xDEAD_BEEF);
        assert_eq!(parsed_broadcast.tc, -1875);
    }

    #[test]
    fn test_process_telemetry_message_malformed_rejects_without_side_effects() {
        let (state, mut live_rx) = setup_test_state();

        let bad_payload = b"{ corrupted_json: true }";
        let result = process_telemetry_message(&state, bad_payload);

        assert!(matches!(result, Err(IngestionError::PayloadMalformed(_))));

        let devices = state.db.get_devices().expect("devices query");
        assert!(devices.is_empty());

        assert_eq!(live_rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn test_process_telemetry_message_sequential_telemetry_streams() {
        let (state, mut live_rx) = setup_test_state();
        let device_id = 0xCAFE_BABE;

        for i in 0..3 {
            let payload = serde_json::to_vec(&MqttWirePayload {
                d: device_id,
                ts: 1_710_000_000 + i,
                tc: -1800 - (i as i16 * 50),
                ac: Some(2200 + i as i16 * 10),
                door_open: i == 1,
                ds: if i == 1 { 5 } else { 0 },
                hs: 950 - (i as u16 * 10),
                rs: -60 - (i as i8),
                fv: 10000,
            })
            .unwrap();

            let res = process_telemetry_message(&state, &payload);
            assert!(res.is_ok());
        }

        let readings = state.db.get_latest_readings(device_id, 10).expect("readings query");
        assert_eq!(readings.len(), 3);

        for i in 0..3 {
            let frame = live_rx.try_recv().expect("broadcast frame");
            let wire: MqttWirePayload = serde_json::from_str(&frame).unwrap();
            assert_eq!(wire.ts, 1_710_000_000 + i);
            assert_eq!(wire.tc, -1800 - (i as i16 * 50));
            assert_eq!(wire.ac, Some(2200 + i as i16 * 10));
        }

        assert_eq!(live_rx.try_recv(), Err(TryRecvError::Empty));
    }

    #[test]
    fn test_process_telemetry_message_multi_device_isolation() {
        let (state, _) = setup_test_state();

        let dev_a = 0x1111_1111;
        let dev_b = 0x2222_2222;

        let payload_a = serde_json::to_vec(&MqttWirePayload {
            d: dev_a,
            ts: 1_000,
            tc: -2000,
            ac: None,
            door_open: false,
            ds: 0,
            hs: 990,
            rs: -65,
            fv: 10000,
        })
        .unwrap();

        let payload_b = serde_json::to_vec(&MqttWirePayload {
            d: dev_b,
            ts: 1_001,
            tc: -1500,
            ac: Some(2150),
            door_open: true,
            ds: 10,
            hs: 800,
            rs: -75,
            fv: 10000,
        })
        .unwrap();

        process_telemetry_message(&state, &payload_a).unwrap();
        process_telemetry_message(&state, &payload_b).unwrap();

        let devices = state.db.get_devices().unwrap();
        assert_eq!(devices.len(), 2);

        let readings_a = state.db.get_latest_readings(dev_a, 10).unwrap();
        assert_eq!(readings_a.len(), 1);
        assert_eq!(readings_a[0].device_id.as_u32(), dev_a);
        assert_eq!(readings_a[0].temp.as_centi(), -2000);

        let readings_b = state.db.get_latest_readings(dev_b, 10).unwrap();
        assert_eq!(readings_b.len(), 1);
        assert_eq!(readings_b[0].device_id.as_u32(), dev_b);
        assert_eq!(readings_b[0].temp.as_centi(), -1500);
    }

    #[test]
    fn test_process_telemetry_message_with_no_websocket_subscribers() {
        let (state, live_rx) = setup_test_state();
        drop(live_rx);

        let payload = serde_json::to_vec(&MqttWirePayload {
            d: 0x9999,
            ts: 5_000,
            tc: -1800,
            ac: None,
            door_open: false,
            ds: 0,
            hs: 900,
            rs: -60,
            fv: 10000,
        })
        .unwrap();

        let res = process_telemetry_message(&state, &payload);
        assert!(res.is_ok());

        let readings = state.db.get_latest_readings(0x9999, 10).unwrap();
        assert_eq!(readings.len(), 1);
    }
}
