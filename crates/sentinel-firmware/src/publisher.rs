use core::fmt::Write;
use embassy_executor::task;
use esp_println::println;
use heapless::String;

use sentinel_core::telemetry::{DeviceId, FreezerReading, TopicBuilder};
use crate::TELEMETRY_CHANNEL;

/// Static buffer size for single wire frames.
const JSON_BUFFER_CAPACITY: usize = 256;
const FALLBACK_TOPIC: &str = "sentinel/unknown/telemetry";

#[task]
pub async fn mqtt_publish_task() {
    println!("[MQTT] Publisher pipeline online. Listening on telemetry channel...");
    loop {
        // 1. Await next reading from the sensor sampling task
        let reading = TELEMETRY_CHANNEL.receive().await;
        // 2. Resolve destination topic (bounds-safe C-string slice decoding)
        let topic_bytes = TopicBuilder::telemetry_topic(DeviceId(reading.device_id));
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
                
#[inline]
fn resolve_topic_str(raw: &[u8]) -> &str {
    let len = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    core::str::from_utf8(&raw[..len]).unwrap_or(FALLBACK_TOPIC)
}

#[inline]
fn serialize_compact_json(r: &FreezerReading) -> Option<String<JSON_BUFFER_CAPACITY>> {
    let mut buffer = String::new();
    write!(
        buffer,
        r#"{{"d":{},"ts":{},"tc":{},"do":false,"ds":0,"hs":{},"rs":{},"fv":{}}}"#,
        r.device_id,
        r.timestamp_secs,
        r.temp_centi_c,
        r.health_score,
        r.rssi_dbm,
        r.firmware_version,
    )
    .ok()?;
    Some(buffer)
}
