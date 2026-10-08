//! Level save commands: `apply-level`, `relocate-level`, `copy-level`.

use std::path::Path;

use anyhow::Context as _;
use smw_data::level::{Level, Placement, SaveOptions, SaveReport, save_level};
use smw_data::{LevelId, RomLayout};
use snes_rom::Rom;

fn load(path: &Path) -> anyhow::Result<(Rom, RomLayout)> {
    let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    Ok((rom, layout))
}

fn level_id(n: u16) -> anyhow::Result<LevelId> {
    LevelId::new(n).with_context(|| format!("level {n:#X} is out of range"))
}

fn describe(placement: Placement) -> String {
    match placement {
        Placement::Unchanged => "unchanged".to_owned(),
        Placement::InPlace(addr) => format!("in place at {addr}"),
        Placement::Shared(addr) => format!("shares the identical block at {addr}"),
        Placement::Relocated { tag, addr } => {
            format!("relocated to {addr} (tag at {:#08X}, {} bytes)", tag.tag_pc, tag.data_len)
        }
    }
}

fn print_report(report: &SaveReport) {
    println!("layer 1:          {}", describe(report.layer1));
    println!("layer 2:          {}", describe(report.layer2));
    println!("sprites:          {}", describe(report.sprites));
    println!("secondary header: {}", if report.secondary_header { "written" } else { "unchanged" });
    println!("expanded:         {}", report.expanded);
    println!("freed blocks:     {}", report.freed.len());
    println!("checksum:         ${:04X}", report.checksum);
}

fn save_and_write(
    rom: &mut Rom,
    layout: &RomLayout,
    level: &Level,
    opts: &SaveOptions,
    out: &Path,
) -> anyhow::Result<()> {
    let report = save_level(rom, layout, level, opts).context("saving level")?;
    print_report(&report);
    rom.save(out).with_context(|| format!("writing {}", out.display()))?;
    println!("wrote {}", out.display());
    Ok(())
}

/// Replaces a level with the JSON produced by `solar dump-level --json`.
pub(crate) fn apply(rom_path: &Path, out: &Path, json: &Path) -> anyhow::Result<()> {
    let (mut rom, layout) = load(rom_path)?;
    let text =
        std::fs::read_to_string(json).with_context(|| format!("reading {}", json.display()))?;
    let level: Level = serde_json::from_str(&text).context("parsing level JSON")?;
    save_and_write(&mut rom, &layout, &level, &SaveOptions::default(), out)
}

/// Moves a level's layer data into expanded space without changing it.
pub(crate) fn relocate(rom_path: &Path, out: &Path, level: u16) -> anyhow::Result<()> {
    let (mut rom, layout) = load(rom_path)?;
    let level = Level::load(&rom, &layout, level_id(level)?)?;
    let opts = SaveOptions { force_relocate: true, ..SaveOptions::default() };
    save_and_write(&mut rom, &layout, &level, &opts, out)
}

/// Copies level `src` over level `dst`.
pub(crate) fn copy(rom_path: &Path, out: &Path, src: u16, dst: u16) -> anyhow::Result<()> {
    let (mut rom, layout) = load(rom_path)?;
    let mut level = Level::load(&rom, &layout, level_id(src)?)?;
    level.id = level_id(dst)?;
    save_and_write(&mut rom, &layout, &level, &SaveOptions::default(), out)
}
