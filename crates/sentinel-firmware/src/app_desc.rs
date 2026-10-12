#[repr(C, packed)]
pub struct EspAppDesc {
    pub magic_word: u32,
    pub secure_version: u32,
    pub reserv1: [u32; 2],
    pub version: [u8; 32],
    pub project_name: [u8; 32],
    pub time: [u8; 16],
    pub date: [u8; 16],
    pub idf_ver: [u8; 32],
    pub app_elf_sha256: [u8; 32],
    pub min_efuse_blk_rev_full: u16,
    pub max_efuse_blk_rev_full: u16,
    pub mmu_page_size: u8,
    pub reserv3: [u8; 3],
    pub reserv2: [u32; 18],
}

const fn to_fixed<const N: usize>(s: &str) -> [u8; N] {
    let mut out = [0u8; N];
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && i < N - 1 {
        out[i] = bytes[i];
        i += 1;
    }
    out
}

#[no_mangle]
#[used]
#[link_section = ".flash.appdesc"]
pub static esp_app_desc: EspAppDesc = EspAppDesc {
    magic_word: 0xABCD5432,
    secure_version: 0,
    reserv1: [0; 2],
    version: to_fixed(env!("CARGO_PKG_VERSION")),
    project_name: to_fixed("sentinel-firmware"),
    time: to_fixed("00:00:00"),
    date: to_fixed("2026-10-08"),
    idf_ver: to_fixed("5.4.0"),
    app_elf_sha256: [0; 32],
    min_efuse_blk_rev_full: 0,
    max_efuse_blk_rev_full: u16::MAX,
    mmu_page_size: 16,
    reserv3: [0; 3],
    reserv2: [0; 18],
};
