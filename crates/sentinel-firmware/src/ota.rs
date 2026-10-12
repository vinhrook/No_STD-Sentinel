#[cfg(target_arch = "xtensa")]
use {
    embedded_storage::nor_flash::{NorFlash, ReadNorFlash},
    embassy_executor::task,
    embassy_net::{tcp::TcpSocket, Stack},
    esp_println::println,
    esp_storage::FlashStorage,
};

pub const OTADATA_OFFSET: u32 = 0x0000_F000;
pub const SLOT_0_OFFSET: u32 = 0x0002_0000;
pub const SLOT_1_OFFSET: u32 = 0x0020_0000;
pub const SLOT_MAX_SIZE: u32 = 0x001E_0000;
pub const FLASH_SECTOR_SIZE: u32 = 4096;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EspOtaSelectEntry {
    pub ota_seq: u32,
    pub seq_label: [u8; 20],
    pub ota_state: u32,
    pub crc: u32,
}

impl Default for EspOtaSelectEntry {
    fn default() -> Self {
        Self {
            ota_seq: 0,
            seq_label: [0xFF; 20],
            ota_state: 0,
            crc: 0,
        }
    }
}

pub const fn crc32_ieee(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    let mut i = 0;
    while i < data.len() {
        crc ^= data[i] as u32;
        let mut bit = 0;
        while bit < 8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ 0xEDB8_8320;
            } else {
                crc >>= 1;
            }
            bit += 1;
        }
        i += 1;
    }
    !crc
}

pub fn parse_ota_entry(bytes: &[u8; 32]) -> Option<EspOtaSelectEntry> {
    let ota_seq = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let mut seq_label = [0u8; 20];
    seq_label.copy_from_slice(&bytes[4..24]);
    let ota_state = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let stored_crc = u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]);

    let calculated_crc = crc32_ieee(&bytes[0..4]);
    if calculated_crc == stored_crc && ota_seq != 0xFFFF_FFFF && ota_seq > 0 {
        Some(EspOtaSelectEntry {
            ota_seq,
            seq_label,
            ota_state,
            crc: stored_crc,
        })
    } else {
        None
    }
}

pub fn encode_ota_entry(ota_seq: u32) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[0..4].copy_from_slice(&ota_seq.to_le_bytes());
    buf[4..24].copy_from_slice(&[0xFF; 20]);
    buf[24..28].copy_from_slice(&2u32.to_le_bytes());
    let crc = crc32_ieee(&buf[0..4]);
    buf[28..32].copy_from_slice(&crc.to_le_bytes());
    buf
}

#[cfg(target_arch = "xtensa")]
pub struct ActiveSlotInfo {
    pub current_slot: u8,
    pub target_slot: u8,
    pub target_offset: u32,
    pub next_seq: u32,
}

#[repr(align(4))]
struct AlignedBuffer<const N: usize>(pub [u8; N]);

#[cfg(target_arch = "xtensa")]
pub fn detect_target_slot(flash: &mut FlashStorage) -> ActiveSlotInfo {
    let mut sector0 = AlignedBuffer([0u8; 32]);
    let mut sector1 = AlignedBuffer([0u8; 32]);

    let _ = flash.read(OTADATA_OFFSET, &mut sector0.0);
    let _ = flash.read(OTADATA_OFFSET + FLASH_SECTOR_SIZE, &mut sector1.0);

    let entry0 = parse_ota_entry(&sector0.0);
    let entry1 = parse_ota_entry(&sector1.0);

    let (current_slot, next_seq) = match (entry0, entry1) {
        (Some(e0), Some(e1)) => {
            if e0.ota_seq >= e1.ota_seq {
                (((e0.ota_seq - 1) % 2) as u8, e0.ota_seq + 1)
            } else {
                (((e1.ota_seq - 1) % 2) as u8, e1.ota_seq + 1)
            }
        }
        (Some(e0), None) => (((e0.ota_seq - 1) % 2) as u8, e0.ota_seq + 1),
        (None, Some(e1)) => (((e1.ota_seq - 1) % 2) as u8, e1.ota_seq + 1),
        (None, None) => (0, 2),
    };

    let target_slot = if current_slot == 0 { 1 } else { 0 };
    let target_offset = if target_slot == 0 { SLOT_0_OFFSET } else { SLOT_1_OFFSET };

    ActiveSlotInfo {
        current_slot,
        target_slot,
        target_offset,
        next_seq,
    }
}

#[cfg(target_arch = "xtensa")]
#[task]
pub async fn ota_listener_task(stack: Stack<'static>) {
    println!("[OTA] 📡 Wireless OTA updater listening on port 8032...");

    let mut rx_buf = [0u8; 4096];
    let mut tx_buf = [0u8; 512];

    loop {
        let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
        socket.set_timeout(Some(embassy_time::Duration::from_secs(15)));

        if let Ok(()) = socket.accept(8032).await {
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
            println!("  [OTA] 🚀 Wireless OTA connection accepted on port 8032!  ");
            println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

            let mut flash = FlashStorage::new();
            let slot_info = detect_target_slot(&mut flash);

            println!(
                "[OTA] Active Slot: app{} | Target Slot: app{} (Offset: 0x{:08X})",
                slot_info.current_slot, slot_info.target_slot, slot_info.target_offset
            );

            let mut total_bytes_written: u32 = 0;
            let mut chunk = AlignedBuffer([0u8; 4096]);
            let mut chunk_len: usize = 0;
            let mut validated_header = false;
            let mut error_occurred = false;

            loop {
                match socket.read(&mut chunk.0[chunk_len..]).await {
                    Ok(0) => break,
                    Ok(n) => {
                        chunk_len += n;

                        if !validated_header && chunk_len >= 1 {
                            if chunk.0[0] != 0xE9 {
                                println!("[OTA] ❌ Rejected: Invalid ESP32 image magic byte (0x{:02X} != 0xE9)", chunk.0[0]);
                                let _ = socket.write(b"ERR: Invalid ESP32 image\n").await;
                                error_occurred = true;
                                break;
                            }
                            validated_header = true;
                        }

                        if chunk_len == 4096 {
                            let flash_addr = slot_info.target_offset + total_bytes_written;

                            if flash.erase(flash_addr, flash_addr + FLASH_SECTOR_SIZE).is_err() {
                                println!("[OTA] ❌ Flash erase failed at 0x{:08X}", flash_addr);
                                error_occurred = true;
                                break;
                            }

                            if flash.write(flash_addr, &chunk.0).is_err() {
                                println!("[OTA] ❌ Flash write failed at 0x{:08X}", flash_addr);
                                error_occurred = true;
                                break;
                            }

                            total_bytes_written += 4096;
                            chunk_len = 0;

                            if total_bytes_written % (64 * 1024) == 0 {
                                println!("[OTA] ⏳ Flashed {} KB...", total_bytes_written / 1024);
                            }
                        }
                    }
                    Err(e) => {
                        println!("[OTA] ❌ Socket read error: {e:?}");
                        error_occurred = true;
                        break;
                    }
                }
            }

            if !error_occurred && chunk_len > 0 {
                let flash_addr = slot_info.target_offset + total_bytes_written;
                let _ = flash.erase(flash_addr, flash_addr + FLASH_SECTOR_SIZE);

                let padded_len = (chunk_len + 3) & !3;
                for b in &mut chunk.0[chunk_len..padded_len] {
                    *b = 0xFF;
                }

                if flash.write(flash_addr, &chunk.0[..padded_len]).is_err() {
                    println!("[OTA] ❌ Trailing flash write failed");
                    error_occurred = true;
                } else {
                    total_bytes_written += chunk_len as u32;
                }
            }

            if !error_occurred && total_bytes_written > 32 * 1024 {
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                println!(
                    "  [OTA] ✅ Image written successfully! ({} bytes / {} KB)  ",
                    total_bytes_written, total_bytes_written / 1024
                );
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

                let new_entry_bytes = AlignedBuffer(encode_ota_entry(slot_info.next_seq));
                let _ = flash.erase(OTADATA_OFFSET, OTADATA_OFFSET + FLASH_SECTOR_SIZE);
                let _ = flash.write(OTADATA_OFFSET, &new_entry_bytes.0);
                let _ = flash.erase(OTADATA_OFFSET + FLASH_SECTOR_SIZE, OTADATA_OFFSET + 2 * FLASH_SECTOR_SIZE);
                let _ = flash.write(OTADATA_OFFSET + FLASH_SECTOR_SIZE, &new_entry_bytes.0);

                let _ = socket.write(b"OK: OTA Success. Rebooting in 1s...\n").await;
                embassy_time::Timer::after(embassy_time::Duration::from_millis(500)).await;

                println!("[OTA] 🔄 Rebooting into new firmware...");
                esp_hal::reset::software_reset();
            } else if !error_occurred {
                println!("[OTA] ❌ Aborted: Image too small ({} bytes)", total_bytes_written);
                let _ = socket.write(b"ERR: Image too small\n").await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc32_ieee_known_vector() {
        assert_eq!(crc32_ieee(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn test_encode_and_parse_ota_entry() {
        let entry_bytes = encode_ota_entry(42);
        let parsed = parse_ota_entry(&entry_bytes).expect("entry should be valid");
        assert_eq!(parsed.ota_seq, 42);
        assert_eq!(parsed.ota_state, 2);
    }
}
