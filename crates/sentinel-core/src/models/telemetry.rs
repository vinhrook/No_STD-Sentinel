 /// ESP32 MAC addres tail.
 #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
 #[serde(transparent)]
 pub struct DeviceId(pub u32);

 impl DeviceId {
    pub const fn from_mac_tail(mac: [u8; 6]) -> Self {
        let id = (mac[2] as u32) << 24
        | (mac[3] as u32) << 16
        | (mac[4] as u32) << 8
        | (mac[5] as u32);
        Self(id)
    }
    /// format as a zero-padded 8 Character hex string (e.g "00A4F3B2")
    pub fn as_hex(&self) -> [u8; 8] {
        let mut buf = [b'0'; 8];
        let hex_chars = b"0123456789ABCDEF";
        let val = self.0;
        for i in 0..8 {
            let nibble = ((val >> (28 - i * 4)) & 0x0F) as usize;
            buf[i] = hex_chars[nibble];
        }
        buf
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FreezerReading {
    pub device_id: u32,
    pub timestamp_secs: u64,
    pub temp_centi_c: i16,
    pub door_open: bool,
    pub door_open_secs: u16,
    pub health_score: u16,
    pub rssi_dbm: i8,
    pub firmware_version: u32,
}

impl FreezerReading {
    pub const fn temp_display_parts(&self) -> (i16, u16) {
        let integer = self.temp_centi_c / 100;
        let remainder = self.temp_centi_c % 100;
        let fractional = if remainder < 0 {
            (-remainder) as u16
        } else {
            remainder as u16
        };
        (integer, fractional)
    }

    pub const fn is_temp_critical(&self) -> bool {
        self.temp_centi_c > -1000
    }

    pub const fn is_temp_warning(&self) -> bool {
        self.temp_centi_c > -1500
    }
}

impl Default for FreezerReading {
    fn default() -> Self {
        Self {
            device_id: 0,
            timestamp_secs: 0,
            temp_centi_c: -2000,
            door_open: false,
            door_open_secs: 0,
            health_score: 1000,
            rssi_dbm: -50,
            firmware_version: 10000,
        }
    }
}

pub struct TopicBuilder;

impl TopicBuilder {
    pub fn telemetry_topic(device_id: DeviceId) -> [u8; 30] {
        let mut topic = [0u8; 30];
        let prefix = b"sentinel/";
        let suffix = b"/telemetry";
        let hex = device_id.as_hex();

        topic[..9].copy_from_slice(prefix);
        topic[9..17].copy_from_slice(&hex);
        topic[17..27].copy_from_slice(suffix);
        topic
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_id_from_mac() {
        let mac = [0xAA, 0xBB, 0x00, 0xA4, 0xF3, 0xB2];
        let id = DeviceId::from_mac_tail(mac);
        assert_eq!(id.0, 0x00A4F3B2);
    }

    #[test]
    fn device_id_hex_format() {
        let id = DeviceId(0x00A4F3B2);
        let hex = id.as_hex();
        assert_eq!(&hex, b"00A4F3B2");
    }

    #[test]
    fn temp_display_negative() {
        let r = FreezerReading { temp_centi_c: -1850, ..Default::default() };
        let (int, frac) = r.temp_display_parts();
        assert_eq!(int, -18);
        assert_eq!(frac, 50);
    }

    #[test]
    fn temp_display_positive() {
        let r = FreezerReading { temp_centi_c: 520, ..Default::default() };
        let (int, frac) = r.temp_display_parts();
        assert_eq!(int, 5);
        assert_eq!(frac, 20);
    }

    #[test]
    fn critical_threshold() {
        let cold = FreezerReading { temp_centi_c: -1200, ..Default::default() };
        assert!(!cold.is_temp_critical());

        let warm = FreezerReading { temp_centi_c: -800, ..Default::default() };
        assert!(warm.is_temp_critical());
    }

    #[test]
    fn topic_builder_format() {
        let id = DeviceId(0x00A4F3B2);
        let topic = TopicBuilder::telemetry_topic(id);
        let topic_str = core::str::from_utf8(&topic[..27]).expect("valid UTF-8");
        assert_eq!(topic_str, "sentinel/00A4F3B2/telemetry");
    }

    #[test]
    fn default_reading_is_healthy() {
        let r = FreezerReading::default();
        assert_eq!(r.health_score, 1000);
        assert!(!r.is_temp_critical());
        assert!(!r.is_temp_warning()); 

    }
}