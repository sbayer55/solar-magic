//! `solar free-space` and `solar rats`: inspect free runs and RATS tags.

use std::path::Path;

use anyhow::Context as _;
use snes_rom::{Rom, rats};

/// Options for `solar free-space`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Options {
    /// Restrict the search to this LoROM bank.
    pub(crate) bank: Option<u8>,
    /// Hide runs shorter than this.
    pub(crate) min_len: usize,
}

/// Prints the free runs of the ROM at `path`.
pub(crate) fn free_space(path: &Path, opts: Options) -> anyhow::Result<()> {
    let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
    let tags = rats::scan(rom.bytes());
    let (min_pc, max_pc) = opts.bank.map_or((0, rom.len()), |bank| {
        let start = usize::from(bank) << 15;
        (start, start.saturating_add(rats::AllocRequest::LOROM_CHUNK))
    });
    let runs = rats::free_runs(rom.bytes(), &tags, min_pc, max_pc);
    let mut total = 0usize;
    println!("free runs in {min_pc:#08X}..{max_pc:#08X} (>= {} bytes):", opts.min_len);
    for run in runs.iter().filter(|r| r.len >= opts.min_len) {
        total = total.saturating_add(run.len);
        let snes = rom.mapper().to_snes(run.pc).map_or_else(String::new, |a| a.to_string());
        println!("  {:#08X}  {:>7} bytes  {snes}", run.pc, run.len);
    }
    println!(
        "total: {total} bytes in {} runs",
        runs.iter().filter(|r| r.len >= opts.min_len).count()
    );
    Ok(())
}

/// Prints every RATS tag of the ROM at `path`.
pub(crate) fn rats_list(path: &Path) -> anyhow::Result<()> {
    let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
    let tags = rats::scan(rom.bytes());
    println!("{} RATS tags:", tags.len());
    for tag in &tags {
        let snes = rom.mapper().to_snes(tag.data_pc).map_or_else(String::new, |a| a.to_string());
        println!(
            "  tag {:#08X}  data {:#08X} ({snes})  {:>6} bytes",
            tag.tag_pc, tag.data_pc, tag.data_len
        );
    }
    Ok(())
}
