use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    let fixups_dir = out.join("fixups");
    fs::create_dir_all(&fixups_dir).unwrap();

    fs::write(
        fixups_dir.join("rodata_dummy.x"),
        r#"
SECTIONS {
  .rodata_dummy (NOLOAD) :
  {
    . = ALIGN(ALIGNOF(.text));
    . = . + SIZEOF(.text);
    . = ALIGN(0x10000) + 0x20;
    _rodata_reserved_start = .;
  } > RODATA
}
INSERT BEFORE .flash.appdesc;
"#,
    )
    .unwrap();

    fs::write(
        out.join("rodata.x"),
        r#"
SECTIONS {
  .flash.appdesc : ALIGN(4)
  {
    KEEP(*(.flash.appdesc))
    KEEP(*(.flash.appdesc.*))
  } > RODATA

  .rodata : ALIGN(4)
  {
    . = ALIGN (4);
    _rodata_start = ABSOLUTE(.);
    *(.rodata .rodata.*)
    *(.srodata .srodata.*)
    . = ALIGN(4);
    _rodata_end = ABSOLUTE(.);
  } > RODATA

  .rodata.wifi : ALIGN(4)
  {
    . = ALIGN(4);
    *( .rodata_wlog_*.* )
    . = ALIGN(4);
  } > RODATA
}
"#,
    )
    .unwrap();

    println!("cargo:rustc-link-search={}", out.display());
}
