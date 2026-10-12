//! `solar diff-rom`: what changed between two images, labelled by the tables
//! the changes fall in. The first step of every Lunar Magic reverse-
//! engineering loop (`docs/ARCHITECTURE.md`).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context as _;
use smw_data::layout::{KnownRange, LM_25X, MarkerPlacement, label_address, scan_markers};
use smw_data::{LayoutError, RomLayout};
use snes_rom::rats::{self, RatsTag};
use snes_rom::{Rom, diff};

/// Options for `solar diff-rom`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Options {
    /// Merge changed ranges closer than this.
    pub(crate) gap: usize,
    /// List markers of both images.
    pub(crate) markers: bool,
    /// Maximum changed ranges printed.
    pub(crate) limit: usize,
}

/// Bytes shown inline for short ranges.
const INLINE_BYTES: usize = 8;

/// Prints the differences between the images at `a` and `b`.
pub(crate) fn run(a: &Path, b: &Path, opts: &Options) -> anyhow::Result<()> {
    let rom_a = Rom::load(a).with_context(|| format!("loading {}", a.display()))?;
    let rom_b = Rom::load(b).with_context(|| format!("loading {}", b.display()))?;
    for (name, rom) in [("a", &rom_a), ("b", &rom_b)] {
        let (layout, note) = best_effort_layout(rom);
        println!(
            "{name}: {} bytes, layout {}{}",
            rom.len(),
            layout.kind,
            note.map_or_else(String::new, |n| format!(" ({n})"))
        );
    }
    let (layout, _) = best_effort_layout(&rom_b);
    let known = layout.known_ranges(&rom_b);
    let tags_a = rats::scan(rom_a.bytes());
    let tags_b = rats::scan(rom_b.bytes());
    print_tag_changes(&tags_a, &tags_b);
    print_changed_ranges(&rom_a, &rom_b, &tags_b, &known, opts);
    if opts.markers {
        for (name, rom) in [("a", &rom_a), ("b", &rom_b)] {
            print_markers(name, rom);
        }
    }
    Ok(())
}

/// The ROM's layout, or the closest usable one when detection fails: the
/// vanilla tables plus Lunar Magic 2.5x's for any other Lunar Magic version,
/// with a note saying so.
pub(crate) fn best_effort_layout(rom: &Rom) -> (RomLayout, Option<String>) {
    match RomLayout::detect(rom) {
        Ok(layout) => (layout, None),
        Err(LayoutError::LunarMagicUnsupported { signature, missing }) => {
            let layout = RomLayout {
                sprite_banks: Some(LM_25X.sprite_banks),
                known_free: &[],
                lm: Some(LM_25X),
                ..smw_data::layout::VANILLA
            };
            let note = format!(
                "{signature}: unsupported version ({missing}); labels assume 2.5x table addresses"
            );
            (layout, Some(note))
        }
        Err(err) => (smw_data::layout::VANILLA, Some(format!("{err}; labels assume vanilla"))),
    }
}

fn print_tag_changes(tags_a: &[RatsTag], tags_b: &[RatsTag]) {
    let by_pc = |tags: &[RatsTag]| -> BTreeMap<usize, RatsTag> {
        tags.iter().map(|t| (t.tag_pc, *t)).collect()
    };
    let (a, b) = (by_pc(tags_a), by_pc(tags_b));
    println!("RATS tags: {} in a, {} in b", a.len(), b.len());
    for (pc, tag) in &b {
        match a.get(pc) {
            None => println!("  + {pc:#08X}  {:>6} bytes", tag.data_len),
            Some(old) if old.data_len != tag.data_len => {
                println!("  ~ {pc:#08X}  {:>6} -> {:>6} bytes", old.data_len, tag.data_len);
            }
            Some(_) => {}
        }
    }
    for (pc, tag) in &a {
        if !b.contains_key(pc) {
            println!("  - {pc:#08X}  {:>6} bytes", tag.data_len);
        }
    }
}

fn print_changed_ranges(
    rom_a: &Rom,
    rom_b: &Rom,
    tags_b: &[RatsTag],
    known: &[KnownRange],
    opts: &Options,
) {
    let ranges = diff::changed_ranges(rom_a.bytes(), rom_b.bytes(), opts.gap);
    let total: usize = ranges.iter().map(|r| r.end.saturating_sub(r.start)).sum();
    println!("changed: {total} bytes in {} ranges (gap {})", ranges.len(), opts.gap);
    for range in ranges.iter().take(opts.limit) {
        let len = range.end.saturating_sub(range.start);
        let addr = rom_b.mapper().to_snes(range.start);
        let label = addr.and_then(|a| label_address(known, a)).unwrap_or_default();
        let tag = tags_b
            .iter()
            .find(|t| range.start >= t.tag_pc && range.start < t.end_pc())
            .map_or_else(String::new, |t| format!("  [tag {:#08X}]", t.tag_pc));
        let bytes = if len <= INLINE_BYTES {
            let hex = |rom: &Rom| -> String {
                rom.bytes().get(range.clone()).map_or_else(
                    || "--".to_owned(),
                    |b| b.iter().map(|x| format!("{x:02X}")).collect::<Vec<_>>().join(" "),
                )
            };
            format!("  {} -> {}", hex(rom_a), hex(rom_b))
        } else {
            String::new()
        };
        println!(
            "  {:#08X}..{:#08X}  {}  {len:>6} bytes  {label}{tag}{bytes}",
            range.start,
            range.end,
            addr.map_or_else(|| "        ".to_owned(), |a| a.to_string())
        );
    }
    if ranges.len() > opts.limit {
        println!("  ... {} more", ranges.len().saturating_sub(opts.limit));
    }
}

fn print_markers(name: &str, rom: &Rom) {
    let markers = scan_markers(rom);
    println!("markers in {name}: {}", markers.len());
    for m in &markers {
        let placement = match m.placement {
            MarkerPlacement::BlockEnd(tag) => {
                format!("ends block {:#08X} ({} bytes)", tag.tag_pc, tag.data_len)
            }
            MarkerPlacement::RoutinePrefix => "routine prefix".to_owned(),
            MarkerPlacement::Loose => "loose".to_owned(),
        };
        println!(
            "  {:#08X} {}  kind {:02X} rev {:02X}  {placement}",
            m.pc,
            m.addr.map_or_else(String::new, |a| a.to_string()),
            m.kind,
            m.revision
        );
    }
}
