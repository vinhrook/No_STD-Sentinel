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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    fn defalut() -> Self {
        Self {
            device_id: 0,
            timestamp_secs: 0,
            temp_centi_c: -2000,
            door_open: false,
            door_open_secs: 0,
            health_score: 1000,
            ressi_dbm: -50,
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

        topic[..9].copy_from_slice(perfix);
        topic[9..17].copy_from_slice(&hex);
        topic[17..27].copy_from_slice(suffix);
        topic
    }
}