//! `solar info`: summarise a ROM image.

use std::path::Path;

use anyhow::Context as _;
use smw_data::{LayoutError, RomLayout};
use snes_rom::{Rom, rats};

/// Prints header, mapping, RATS and layout facts about the ROM at `path`.
pub(crate) fn run(path: &Path) -> anyhow::Result<()> {
    let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
    let header = rom.header();

    println!("file:            {}", path.display());
    println!("size:            {} bytes ({} KiB)", rom.len(), rom.len() / 1024);
    println!("copier header:   {}", if rom.had_copier_header() { "stripped" } else { "none" });
    println!("title:           {:?}", header.title);
    println!(
        "mapping:         {} (map mode ${:02X}, {})",
        rom.mapper().name(),
        header.map_mode,
        if header.is_fast_rom() { "FastROM" } else { "SlowROM" }
    );
    println!(
        "declared size:   {}",
        header.rom_size_bytes().map_or_else(|| "invalid".to_owned(), |n| format!("{n} bytes"))
    );
    println!(
        "sram:            {}",
        header.sram_size_bytes().map_or_else(|| "invalid".to_owned(), |n| format!("{n} bytes"))
    );
    println!("region:          ${:02X}", header.region);
    println!("version:         1.{}", header.version);
    println!(
        "checksum:        ${:04X} (complement ${:04X}, computed ${:04X}, {})",
        header.checksum,
        header.checksum_complement,
        rom.compute_checksum(),
        if rom.checksum_matches() { "ok" } else { "MISMATCH" }
    );

    let tags = rats::scan(rom.bytes());
    println!("RATS tags:       {}", tags.len());
    for tag in tags.iter().take(16) {
        println!("  {:#08X}  {} bytes", tag.tag_pc, tag.data_len);
    }
    if tags.len() > 16 {
        println!("  ... {} more", tags.len().saturating_sub(16));
    }

    match RomLayout::detect(&rom) {
        Ok(layout) => println!("layout:          {:?}", layout.kind),
        Err(LayoutError::LunarMagicUnsupported { signature }) => {
            println!("layout:          Lunar Magic ({signature}) - not supported yet");
        }
        Err(err) => println!("layout:          unavailable ({err})"),
    }
    Ok(())
}
