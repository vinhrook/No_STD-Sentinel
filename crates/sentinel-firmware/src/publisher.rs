use core::fmt::Write as FmtWrite;

#[cfg(target_arch = "xtensa")]
use {
    crate::{config, MqttPayload, TELEMETRY_CHANNEL},
    embassy_executor::task,
    embassy_net::{tcp::TcpSocket, IpAddress, Stack},
    esp_println::println,
};

use heapless::String;
use sentinel_core::telemetry::{topics, DeviceId, FreezerReading};

pub const JSON_BUFFER_CAPACITY: usize = 256;

pub const FALLBACK_TOPIC: &str = "sentinel/unknown/telemetry";

pub const MQTT_CONNECT_PACKET: [u8; 28] = [
    0x10, 0x1A,
    0x00, 0x04, b'M', b'Q', b'T', b'T',
    0x04,
    0x02,
    0x00, 0x3C,
    0x00, 0x0E,
    b's', b'e', b'n', b't', b'i', b'n', b'e', b'l', b'-', b'e', b's', b'p', b'3', b'2',
];

pub fn encode_publish_frame(topic: &str, payload: &str, out: &mut [u8]) -> Option<usize> {
    let rem_len = 2 + topic.len() + payload.len();
    let rem_len_bytes = if rem_len < 128 { 1 } else { 2 };
    let total_len = 1 + rem_len_bytes + 2 + topic.len() + payload.len();

    if out.len() < total_len {
        return None;
    }

    let mut idx = 0;
    out[idx] = 0x30;
    idx += 1;

    if rem_len < 128 {
        out[idx] = rem_len as u8;
        idx += 1;
    } else {
        out[idx] = ((rem_len & 0x7F) as u8) | 0x80;
        idx += 1;
        out[idx] = (rem_len >> 7) as u8;
        idx += 1;
    }

    let topic_len = topic.len() as u16;
    out[idx] = (topic_len >> 8) as u8;
    idx += 1;
    out[idx] = (topic_len & 0xFF) as u8;
    idx += 1;

    out[idx..idx + topic.len()].copy_from_slice(topic.as_bytes());
    idx += topic.len();

    out[idx..idx + payload.len()].copy_from_slice(payload.as_bytes());
    idx += payload.len();

    Some(idx)
}

pub const LOG_PREFIX: &[u8; 9] = b"sentinel/";
pub const LOG_SUFFIX: &[u8; 5] = b"/logs";
pub const LOG_TOPIC_LEN: usize = LOG_PREFIX.len() + 8 + LOG_SUFFIX.len();

#[inline]
pub const fn topic_for_device(device_id: DeviceId) -> [u8; topics::LEN] {
    topics::telemetry(device_id)
}

#[inline]
pub fn topic_for_log(device_id: DeviceId) -> [u8; LOG_TOPIC_LEN] {
    let hex = device_id.as_hex();
    let mut buf = [0u8; LOG_TOPIC_LEN];
    let mut i = 0;
    while i < LOG_PREFIX.len() {
        buf[i] = LOG_PREFIX[i];
        i += 1;
    }
    let mut j = 0;
    while j < 8 {
        buf[LOG_PREFIX.len() + j] = hex[j];
        j += 1;
    }
    let mut k = 0;
    while k < LOG_SUFFIX.len() {
        buf[LOG_PREFIX.len() + 8 + k] = LOG_SUFFIX[k];
        k += 1;
    }
    buf
}

#[inline]
pub fn resolve_topic_str(raw: &[u8]) -> &str {
    let len = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    core::str::from_utf8(&raw[..len]).unwrap_or(FALLBACK_TOPIC)
}

#[inline]
pub fn serialize_compact_json(r: &FreezerReading) -> Option<String<JSON_BUFFER_CAPACITY>> {
    serialize_compact_json_dual(r, None)
}

#[inline]
pub fn serialize_compact_json_dual(
    r: &FreezerReading,
    ambient_cc: Option<i16>,
) -> Option<String<JSON_BUFFER_CAPACITY>> {
    let mut buffer = String::new();
    if let Some(amb) = ambient_cc {
        write!(
            buffer,
            r#"{{"d":{},"ts":{},"tc":{},"ac":{},"do":{},"ds":{},"hs":{},"rs":{},"fv":{}}}"#,
            r.device_id.as_u32(),
            r.timestamp_secs,
            r.temp.as_centi(),
            amb,
            r.door_open,
            r.door_open_secs,
            r.health_score.as_u16(),
            r.rssi_dbm,
            r.firmware_version.as_u32(),
        )
        .ok()?;
    } else {
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
    }
    Some(buffer)
}

#[cfg(target_arch = "xtensa")]
#[task]
pub async fn mqtt_publish_task(stack: Stack<'static>) {
    println!("[MQTT] Publisher pipeline online. Waiting for network link...");

    while !stack.is_link_up() || stack.config_v4().is_none() {
        embassy_time::Timer::after(embassy_time::Duration::from_millis(500)).await;
    }
    println!("[MQTT] Network link ready. Starting broker connection loop...");
    if let Some(cfg) = stack.config_v4() {
        let octets = cfg.address.address().octets();
        println!("[NET]  🌐 Assigned DHCP IPv4: {}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3]);
        let base_mac = esp_hal::efuse::Efuse::read_base_mac_address();
        let device_id = DeviceId::from_mac_tail(base_mac);
        let mut msg = heapless::String::<128>::new();
        let _ = core::fmt::write(
            &mut msg,
            format_args!(
                "[NET] IP: {}.{}.{}.{} | Wireless OTA Port: 8032 ready",
                octets[0], octets[1], octets[2], octets[3]
            ),
        );
        let _ = TELEMETRY_CHANNEL.try_send(MqttPayload::Log {
            device_id,
            message: msg,
        });
    }

    let broker_endpoint = (
        IpAddress::v4(
            config::BROKER_IP[0],
            config::BROKER_IP[1],
            config::BROKER_IP[2],
            config::BROKER_IP[3],
        ),
        config::BROKER_PORT,
    );

    loop {
        let mut rx_buf = [0u8; 512];
        let mut tx_buf = [0u8; 512];
        let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
        socket.set_timeout(Some(embassy_time::Duration::from_secs(10)));

        println!(
            "[MQTT] Connecting TCP socket to Mosquitto at {}.{}.{}.{}:{}...",
            config::BROKER_IP[0],
            config::BROKER_IP[1],
            config::BROKER_IP[2],
            config::BROKER_IP[3],
            config::BROKER_PORT
        );

        match socket.connect(broker_endpoint).await {
            Ok(()) => {
                println!("[MQTT] TCP connected! Performing MQTT handshake...");

                if let Err(e) = socket.write(&MQTT_CONNECT_PACKET).await {
                    println!("[MQTT] Failed to send CONNECT packet: {e:?}");
                    embassy_time::Timer::after(embassy_time::Duration::from_secs(3)).await;
                    continue;
                }

                let mut connack = [0u8; 4];
                match socket.read(&mut connack).await {
                    Ok(4) if connack[0] == 0x20 && connack[3] == 0x00 => {
                        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                        println!("  [MQTT] Live connection established to Mosquitto broker!  ");
                        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    }
                    Ok(n) => {
                        println!(
                            "[MQTT] Unexpected CONNACK reply ({} bytes): {:02X?}",
                            n,
                            &connack[..n]
                        );
                        embassy_time::Timer::after(embassy_time::Duration::from_secs(3)).await;
                        continue;
                    }
                    Err(e) => {
                        println!("[MQTT] Error reading CONNACK: {e:?}");
                        embassy_time::Timer::after(embassy_time::Duration::from_secs(3)).await;
                        continue;
                    }
                }

                loop {
                    let packet = TELEMETRY_CHANNEL.receive().await;
                    match packet {
                        MqttPayload::Telemetry { reading, ambient_cc } => {
                            let topic_bytes = topic_for_device(reading.device_id);
                            let topic = resolve_topic_str(&topic_bytes);

                            match serialize_compact_json_dual(&reading, ambient_cc) {
                                Some(payload) => {
                                    let mut wire_buffer = [0u8; 384];
                                    if let Some(wire_len) =
                                        encode_publish_frame(topic, payload.as_str(), &mut wire_buffer)
                                    {
                                        match socket.write(&wire_buffer[..wire_len]).await {
                                            Ok(_) => {
                                                println!(
                                                    "[MQTT] 🚀 Wire Frame Published -> {} ({}B)",
                                                    topic, wire_len
                                                );
                                            }
                                            Err(e) => {
                                                println!(
                                                    "[MQTT] Transmission failure: {e:?}. Reconnecting..."
                                                );
                                                break;
                                            }
                                        }
                                    } else {
                                        println!("[MQTT] Packet encoder buffer overflow");
                                    }
                                }
                                None => {
                                    println!("[MQTT] JSON serialization buffer overflow");
                                }
                            }
                        }
                        MqttPayload::Log { device_id, message } => {
                            let topic_bytes = topic_for_log(device_id);
                            let topic = resolve_topic_str(&topic_bytes);
                            let mut wire_buffer = [0u8; 384];
                            if let Some(wire_len) =
                                encode_publish_frame(topic, message.as_str(), &mut wire_buffer)
                            {
                                if let Err(e) = socket.write(&wire_buffer[..wire_len]).await {
                                    println!("[MQTT] Log write failure: {e:?}. Reconnecting...");
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("[MQTT] TCP connection failed: {e:?}. Retrying in 5s...");
                embassy_time::Timer::after(embassy_time::Duration::from_secs(5)).await;
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
        let raw = b"sentinel/00A4F3B2/telemetry\0\0\0extra";
        assert_eq!(resolve_topic_str(raw), "sentinel/00A4F3B2/telemetry");

        let plain = b"sentinel/test/telemetry";
        assert_eq!(resolve_topic_str(plain), "sentinel/test/telemetry");

        let bad_utf8 = [0xFF, 0xFE, 0xFD, 0x00];
        assert_eq!(resolve_topic_str(&bad_utf8), FALLBACK_TOPIC);
    }

    #[test]
    fn test_serialize_compact_json_nominal() {
        let reading = FreezerReading {
            device_id: DeviceId::new(0x1234_5678),
            timestamp_secs: 1_710_000_000,
            temp: TempCentiC::from_centi(-2000),
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
            temp: TempCentiC::from_centi(450),
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
        assert!(json.len() < JSON_BUFFER_CAPACITY / 2);
    }

    #[test]
    fn test_encode_publish_frame_nominal() {
        let topic = "sentinel/0A892648/telemetry";
        let payload = r#"{"d":176760392,"ts":10,"tc":2450}"#;
        let mut buffer = [0u8; 256];
        let len = encode_publish_frame(topic, payload, &mut buffer).expect("should encode");

        assert_eq!(buffer[0], 0x30);

        let rem_len = 2 + topic.len() + payload.len();
        assert_eq!(buffer[1], rem_len as u8);

        let topic_len = ((buffer[2] as usize) << 8) | (buffer[3] as usize);
        assert_eq!(topic_len, topic.len());

        assert_eq!(&buffer[4..4 + topic.len()], topic.as_bytes());

        assert_eq!(&buffer[4 + topic.len()..len], payload.as_bytes());
    }
}
