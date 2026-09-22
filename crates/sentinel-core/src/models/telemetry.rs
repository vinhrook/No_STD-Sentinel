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