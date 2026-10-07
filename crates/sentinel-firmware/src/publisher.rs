use core::fmt::Write;

#[cfg(target_arch = "xtensa")]
use {
    crate::TELEMETRY_CHANNEL,
    embassy_executor::task,
    esp_println::println,
};

use heapless::String;
use sentinel_core::telemetry::{topics, DeviceId, FreezerReading};

/// Static buffer size for single wire frames.
pub const JSON_BUFFER_CAPACITY: usize = 256;

/// Fallback topic when UTF-8 decoding fails on anomalous hardware buffers.
pub const FALLBACK_TOPIC: &str = "sentinel/unknown/telemetry";

/// Formats the MQTT destination topic for a given [`DeviceId`].
///
/// Guaranteed zero-heap, constant-time, and bounds-safe.
#[inline]
pub const fn topic_for_device(device_id: DeviceId) -> [u8; topics::LEN] {
    topics::telemetry(device_id)
}

/// Resolves a topic string from a byte slice, stripping trailing NUL terminators
/// and falling back to a safe topic if UTF-8 decoding fails.
#[inline]
pub fn resolve_topic_str(raw: &[u8]) -> &str {
    let len = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    core::str::from_utf8(&raw[..len]).unwrap_or(FALLBACK_TOPIC)
}

/// Serializes a [`FreezerReading`] into a zero-allocation compact JSON wire payload.
///
/// Returns `None` if the formatted payload exceeds [`JSON_BUFFER_CAPACITY`].
#[inline]
pub fn serialize_compact_json(r: &FreezerReading) -> Option<String<JSON_BUFFER_CAPACITY>> {
    let mut buffer = String::new();
    write!(
        buffer,
        r#"{{"d":{},"ts":{},"tc":{},"do":{},"ds":{},"hs":{},"rs":{},"fv":{}}}"#,
        r.device_id.as_u32(),
        r.timestamp_secs,
        r.temp.as_centi(),
        r.door_open,
        r.door_open_secs,
        r.health_score.as_u16(),
        r.rssi_dbm,
        r.firmware_version.as_u32(),
    )
    .ok()?;
    Some(buffer)
}

#[cfg(target_arch = "xtensa")]
#[task]
pub async fn mqtt_publish_task() {
    println!("[MQTT] Publisher pipeline online. Listening on telemetry channel...");
    loop {
        // 1. Await next reading from the sensor sampling task
        let reading = TELEMETRY_CHANNEL.receive().await;
        // 2. Resolve destination topic (bounds-safe C-string slice decoding)
        let topic_bytes = topic_for_device(reading.device_id);
        let topic = resolve_topic_str(&topic_bytes);
        // 3. Serialize to zero-heap, compact JSON wire payload
        match serialize_compact_json(&reading) {
            Some(payload) => {
                println!("[MQTT] Publish -> {topic}\n    {payload}");
            }
            None => {
                println!("[MQTT] Buffer overflow: JSON payload exceeded {JSON_BUFFER_CAPACITY}B");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinel_core::telemetry::{FirmwareVersion, HealthScore, TempCentiC};

    #[test]
    fn test_topic_for_device_format() {
        let dev = DeviceId::new(0x00A4_F3B2);
        let topic_bytes = topic_for_device(dev);
        assert_eq!(topic_bytes.len(), topics::LEN);
        assert_eq!(resolve_topic_str(&topic_bytes), "sentinel/00A4F3B2/telemetry");

        let zero_dev = DeviceId::ZERO;
        let zero_bytes = topic_for_device(zero_dev);
        assert_eq!(resolve_topic_str(&zero_bytes), "sentinel/00000000/telemetry");
    }

    #[test]
    fn test_resolve_topic_str_nul_handling() {
        // Topic slice with trailing NULs
        let raw = b"sentinel/00A4F3B2/telemetry\0\0\0extra";
        assert_eq!(resolve_topic_str(raw), "sentinel/00A4F3B2/telemetry");

        // Plain string without NUL
        let plain = b"sentinel/test/telemetry";
        assert_eq!(resolve_topic_str(plain), "sentinel/test/telemetry");

        // Invalid UTF-8 sequence should fall back
        let bad_utf8 = [0xFF, 0xFE, 0xFD, 0x00];
        assert_eq!(resolve_topic_str(&bad_utf8), FALLBACK_TOPIC);
    }

    #[test]
    fn test_serialize_compact_json_nominal() {
        let reading = FreezerReading {
            device_id: DeviceId::new(0x1234_5678),
            timestamp_secs: 1_710_000_000,
            temp: TempCentiC::from_centi(-2000), // -20.00°C
            door_open: false,
            door_open_secs: 0,
            health_score: HealthScore::new(1000),
            rssi_dbm: -55,
            firmware_version: FirmwareVersion::new(1, 0, 0),
        };

        let json = serialize_compact_json(&reading).expect("serialization should succeed");
        assert_eq!(
            json.as_str(),
            r#"{"d":305419896,"ts":1710000000,"tc":-2000,"do":false,"ds":0,"hs":1000,"rs":-55,"fv":10000}"#
        );
    }

    #[test]
    fn test_serialize_compact_json_door_alarm_and_subzero() {
        let reading = FreezerReading {
            device_id: DeviceId::new(0x00A4_F3B2),
            timestamp_secs: 1_710_000_120,
            temp: TempCentiC::from_centi(450), // +4.50°C (warm breach)
            door_open: true,
            door_open_secs: 180,
            health_score: HealthScore::new(350),
            rssi_dbm: -82,
            firmware_version: FirmwareVersion::new(2, 4, 1),
        };

        let json = serialize_compact_json(&reading).expect("serialization should succeed");
        assert_eq!(
            json.as_str(),
            r#"{"d":10810290,"ts":1710000120,"tc":450,"do":true,"ds":180,"hs":350,"rs":-82,"fv":20401}"#
        );
    }

    #[test]
    fn test_serialize_compact_json_valid_json_schema() {
        let reading = FreezerReading::PLACEHOLDER;
        let json_str = serialize_compact_json(&reading).expect("should serialize placeholder");

        // Verify with serde_json parser that the compact output is 100% valid JSON
        let parsed: serde_json::Value =
            serde_json::from_str(json_str.as_str()).expect("output must be valid JSON");

        assert_eq!(parsed["d"], reading.device_id.as_u32());
        assert_eq!(parsed["ts"], reading.timestamp_secs);
        assert_eq!(parsed["tc"], reading.temp.as_centi());
        assert_eq!(parsed["do"], reading.door_open);
        assert_eq!(parsed["ds"], reading.door_open_secs);
        assert_eq!(parsed["hs"], reading.health_score.as_u16());
        assert_eq!(parsed["rs"], reading.rssi_dbm);
        assert_eq!(parsed["fv"], reading.firmware_version.as_u32());
    }

    #[test]
    fn test_payload_size_margin() {
        let reading = FreezerReading {
            device_id: DeviceId::new(u32::MAX),
            timestamp_secs: u64::MAX,
            temp: TempCentiC::from_centi(i16::MIN),
            door_open: true,
            door_open_secs: u16::MAX,
            health_score: HealthScore::new(HealthScore::MAX),
            rssi_dbm: i8::MIN,
            firmware_version: FirmwareVersion::new(429_495, 99, 99),
        };

        let json = serialize_compact_json(&reading).expect("maximum values should fit in buffer");
        // Compact JSON should fit comfortably under 256 bytes (usually < 120 bytes even with max values)
        assert!(json.len() < JSON_BUFFER_CAPACITY / 2);
    }
}
