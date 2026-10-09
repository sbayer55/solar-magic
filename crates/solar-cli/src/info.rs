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

    let markers = smw_data::layout::scan_markers(&rom);
    println!("LM markers:      {}", markers.len());
    for m in markers.iter().take(8) {
        let placement = match m.placement {
            smw_data::layout::MarkerPlacement::BlockEnd(tag) => {
                format!("ends block of {} bytes", tag.data_len)
            }
            smw_data::layout::MarkerPlacement::RoutinePrefix => "routine prefix".to_owned(),
            smw_data::layout::MarkerPlacement::Loose => "loose".to_owned(),
        };
        let addr = m.addr.map_or_else(String::new, |a| a.to_string());
        println!("  {:#08X} {addr}  kind {:02X} rev {:02X}  {placement}", m.pc, m.kind, m.revision);
    }
    if markers.len() > 8 {
        println!("  ... {} more", markers.len().saturating_sub(8));
    }

    match RomLayout::detect(&rom) {
        Ok(layout) => {
            println!("layout:          {}", layout.kind);
            if layout.is_lunar_magic() {
                print_lunar_magic_summary(&rom, &layout)?;
            }
        }
        Err(LayoutError::LunarMagicUnsupported { signature, missing }) => {
            println!("layout:          unsupported Lunar Magic version ({signature}): {missing}");
        }
        Err(err) => println!("layout:          unavailable ({err})"),
    }
    Ok(())
}

/// Counts what Lunar Magic added: edited levels, custom palettes, inserted ExGFX.
fn print_lunar_magic_summary(rom: &Rom, layout: &RomLayout) -> anyhow::Result<()> {
    let Some(lm) = &layout.lm else {
        return Ok(());
    };
    let infos = smw_data::LevelId::all()
        .map(|id| smw_data::level::LmLevelInfo::load(rom, layout, id))
        .collect::<Result<Vec<_>, _>>()?;
    let infos: Vec<_> = infos.into_iter().flatten().collect();
    let relocated = infos.iter().filter(|i| i.flags.relocated_tilemap()).count();
    let sixteen_bit = infos.iter().filter(|i| i.flags.sixteen_bit()).count();
    let palettes = infos.iter().filter(|i| i.custom_palette.is_some()).count();
    let exgfx = (0..0x80u32)
        .filter(|i| {
            lm.exgfx_ptrs
                .checked_add(i.saturating_mul(3))
                .and_then(|a| rom.read_u24_le(a).ok())
                .is_some_and(|p| p != 0 && p != 0xFF_FFFF)
        })
        .count();
    println!("  signature:     {:?}", RomLayout::lunar_magic_signature(rom).unwrap_or_default());
    println!("  features:      {}", lm.features.names().join(" "));
    println!("  flags format:  {:?}", lm.layer2_flags_format);
    println!("  relocated layer 2 tilemaps: {relocated} ({sixteen_bit} 16-bit)");
    println!("  custom palettes: {palettes}");
    println!("  ExGFX 80-FF inserted: {exgfx}");
    Ok(())
}
