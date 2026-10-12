//! `solar mwl-export`, `mwl-import`, `mwl-info`, `export-all`, `import-all`:
//! Lunar Magic `.mwl` level files.

use std::path::Path;

use anyhow::Context as _;
use smw_data::formats::mwl::Mwl;
use smw_data::formats::{ExportOptions, export_level, import_level};
use smw_data::level::{Level, SaveOptions, save_level};
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

/// Writes level `number` of `rom` to `out`.
pub(crate) fn export(rom: &Path, number: u16, out: &Path) -> anyhow::Result<()> {
    let (rom, layout) = load(rom)?;
    let level = Level::load(&rom, &layout, level_id(number)?)?;
    let mwl = export_level(&rom, &layout, &level, &ExportOptions::default())?;
    std::fs::write(out, mwl.to_bytes()).with_context(|| format!("writing {}", out.display()))?;
    println!("wrote {} ({} bytes)", out.display(), mwl.to_bytes().len());
    Ok(())
}

/// Writes every level of `rom` into `dir` as `<prefix><level>.mwl`.
pub(crate) fn export_all(rom: &Path, dir: &Path, prefix: &str) -> anyhow::Result<()> {
    let (rom, layout) = load(rom)?;
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let mut written = 0usize;
    for id in LevelId::all() {
        let level = Level::load(&rom, &layout, id).with_context(|| format!("level {id}"))?;
        let mwl = export_level(&rom, &layout, &level, &ExportOptions::default())?;
        let out = dir.join(format!("{prefix}{id}.mwl"));
        std::fs::write(&out, mwl.to_bytes())
            .with_context(|| format!("writing {}", out.display()))?;
        written = written.saturating_add(1);
    }
    println!("wrote {written} levels to {}", dir.display());
    Ok(())
}

/// Imports `file` into slot `number` (or the file's own level number) and
/// saves the ROM to `out`.
pub(crate) fn import(
    rom: &Path,
    out: &Path,
    file: &Path,
    number: Option<u16>,
) -> anyhow::Result<()> {
    let (mut rom, layout) = load(rom)?;
    let mwl = read(file)?;
    let id = match number {
        Some(n) => level_id(n)?,
        None => level_id(source_level(&mwl))?,
    };
    import_one(&mut rom, &layout, &mwl, id)?;
    rom.save(out).with_context(|| format!("writing {}", out.display()))?;
    println!("wrote {}", out.display());
    Ok(())
}

/// Imports every `.mwl` in `dir` into the level its name or header says.
pub(crate) fn import_all(rom: &Path, out: &Path, dir: &Path) -> anyhow::Result<()> {
    let (mut rom, layout) = load(rom)?;
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mwl")))
        .collect();
    files.sort();
    for file in &files {
        let mwl = read(file)?;
        let id = level_id(source_level(&mwl))?;
        import_one(&mut rom, &layout, &mwl, id).with_context(|| format!("{}", file.display()))?;
    }
    rom.save(out).with_context(|| format!("writing {}", out.display()))?;
    println!("imported {} levels, wrote {}", files.len(), out.display());
    Ok(())
}

/// Prints what a `.mwl` contains.
pub(crate) fn info(file: &Path) -> anyhow::Result<()> {
    let mwl = read(file)?;
    print!("{mwl}");
    println!("  source level: {:#05X}", source_level(&mwl));
    Ok(())
}

fn read(file: &Path) -> anyhow::Result<Mwl> {
    let bytes = std::fs::read(file).with_context(|| format!("reading {}", file.display()))?;
    Mwl::parse(&bytes).with_context(|| format!("parsing {}", file.display()))
}

/// The level number recorded in the file.
fn source_level(mwl: &Mwl) -> u16 {
    let info = mwl.section(smw_data::formats::mwl::Section::LevelInfo);
    u16::from_le_bytes([info.first().copied().unwrap_or(0), info.get(1).copied().unwrap_or(0)])
}

fn import_one(rom: &mut Rom, layout: &RomLayout, mwl: &Mwl, id: LevelId) -> anyhow::Result<()> {
    let imported = import_level(rom, layout, mwl, id)?;
    let report = save_level(rom, layout, &imported.level, &SaveOptions::default())
        .with_context(|| format!("saving level {id}"))?;
    for entry in &imported.secondary_exits {
        entry
            .exit
            .write(rom, layout, entry.id)
            .with_context(|| format!("writing exit {}", entry.id))?;
    }
    rom.fix_checksum();
    println!(
        "level {id}: layer 1 {:?}, layer 2 {:?}, sprites {:?}, {} secondary exits{}",
        report.layer1,
        report.layer2,
        report.sprites,
        imported.secondary_exits.len(),
        if imported.custom_palette { " (custom palette not written)" } else { "" }
    );
    Ok(())
}
