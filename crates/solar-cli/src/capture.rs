//! `solar capture`: launch a level in the emulator and dump what the editor
//! takes from it (frames, VRAM, CGRAM, OAM, tile RAM, registers).

use std::path::Path;

use anyhow::Context as _;
use smw_data::level::Level;
use smw_data::{LevelId, RomLayout};
use smw_play::{BootCache, CaptureOptions, capture_level};
use smw_render::assets::{CapturedFrame, LevelAssets, PpuSummary};
use snes_rom::Rom;

/// Captures level `number` of `rom` into `dir`.
pub(crate) fn run(rom_path: &Path, number: u16, dir: &Path) -> anyhow::Result<()> {
    let rom = Rom::load(rom_path).with_context(|| format!("loading {}", rom_path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    let id = LevelId::new(number).with_context(|| format!("level {number:#X} is out of range"))?;
    let level = Level::load(&rom, &layout, id)?;
    let mut cache = BootCache::new();
    let assets = capture_level(&mut cache, &rom, &layout, level, CaptureOptions::default())
        .with_context(|| format!("capturing level {id}"))?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    write_png(&assets.frame, &dir.join("frame.png"))?;
    write_png(&assets.frame_no_obj, &dir.join("frame-noobj.png"))?;
    let words: Vec<u8> = assets.vram.iter().flat_map(|w| w.to_le_bytes()).collect();
    std::fs::write(dir.join("vram.bin"), words)?;
    let cgram: Vec<u8> =
        assets.palette.colors().iter().flat_map(|c| c.raw().to_le_bytes()).collect();
    std::fs::write(dir.join("cgram.bin"), cgram)?;
    std::fs::write(dir.join("oam.bin"), &assets.oam)?;
    std::fs::write(dir.join("tile-ram-lo.bin"), &assets.tile_ram.low)?;
    std::fs::write(dir.join("tile-ram-hi.bin"), &assets.tile_ram.high)?;
    std::fs::write(dir.join("regs.txt"), regs_text(&assets))?;
    println!(
        "level {id}: captured at frame {} ({} animated tiles, IRQ line {}, HDMA {:#04X}) -> {}",
        assets.frame_number,
        assets.animated.len(),
        assets.play.irq_line,
        assets.play.hdma_enabled,
        dir.display()
    );
    Ok(())
}

fn write_png(frame: &CapturedFrame, path: &Path) -> anyhow::Result<()> {
    let img: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> =
        image::ImageBuffer::from_raw(frame.width, frame.height, frame.rgba.clone())
            .context("frame dimensions")?;
    img.save(path).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn regs_text(assets: &LevelAssets) -> String {
    let mut lines: Vec<String> = Vec::new();
    for (name, regs) in [("top", &assets.top), ("play", &assets.play)] {
        lines.extend(summary_lines(name, regs));
    }
    for (tile, record) in &assets.animated {
        lines.push(format!(
            "animated {tile:#05X}: {:04X} {:04X} {:04X} {:04X}",
            record.top_left.to_word(),
            record.bottom_left.to_word(),
            record.top_right.to_word(),
            record.bottom_right.to_word()
        ));
    }
    lines.push(String::new());
    lines.join("\n")
}

fn summary_lines(name: &str, r: &PpuSummary) -> Vec<String> {
    let mut lines = vec![format!(
        "[{name}] mode {} bg3prio {} TM {:#04X} TS {:#04X} CGADSUB {:#04X} CGWSEL {:#04X} fixed {:#06X} brightness {} obj_chars {:#06X} irq_line {} hdma {:#04X}",
        r.bg_mode,
        r.bg3_priority,
        r.main_screen,
        r.sub_screen,
        r.cgadsub,
        r.cgwsel,
        r.fixed_color,
        r.brightness,
        r.obj_chars,
        r.irq_line,
        r.hdma_enabled
    )];
    for (i, bg) in r.bg.iter().enumerate() {
        lines.push(format!(
            "[{name}] BG{} tilemap {:#06X} size {} chars {:#06X} hofs {} vofs {}",
            i.saturating_add(1),
            bg.tilemap,
            bg.tilemap_size,
            bg.chars,
            bg.hofs,
            bg.vofs
        ));
    }
    lines
}
