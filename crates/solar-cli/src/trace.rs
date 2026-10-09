//! `solar trace`: run a routine or a level load in the emulator and report
//! what it touched. The reverse-engineering tool for locating tables.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context as _, bail};
use smw_data::layout::{KnownRange, label_address};
use smw_data::level::Level;
use smw_data::{LevelId, RomLayout};
use smw_render::emu::{SnesBus, Trace};
use smw_render::level::load_level_data_traced;
use snes_rom::Rom;
use w65c816::{Addr, Cpu};

/// Address the emulated `JSL` returns to.
const SENTINEL: u32 = 0xFF_FFFF;

/// Options for `solar trace`.
#[derive(Debug, Clone)]
pub(crate) struct Options {
    /// `level:<hex>` or a SNES address.
    pub(crate) target: String,
    /// `addr=value` WRAM seeds.
    pub(crate) ram: Vec<String>,
    /// Instruction budget.
    pub(crate) steps: u64,
    /// Range merge distance.
    pub(crate) gap: u32,
    /// 8-bit accumulator.
    pub(crate) m8: bool,
    /// 8-bit index registers.
    pub(crate) x8: bool,
    /// Hide ROM ranges a known table accounts for.
    pub(crate) unknown_only: bool,
    /// WRAM regions to hex-dump after the run, as `addr:len`.
    pub(crate) dump: Vec<String>,
    /// Run until this address instead of calling the target as a subroutine.
    pub(crate) stop: Option<String>,
    /// ROM-to-WRAM copies to perform before the run, as `rom:ram:len`.
    pub(crate) copy: Vec<String>,
}

/// Runs the trace and prints the report.
pub(crate) fn run(path: &Path, opts: &Options) -> anyhow::Result<()> {
    let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
    let (layout, note) = crate::diff_rom::best_effort_layout(&rom);
    if let Some(note) = note {
        println!("note: {note}");
    }
    let (trace, wram) = if let Some(level) = opts.target.strip_prefix("level:") {
        if !layout.is_lunar_magic() && RomLayout::lunar_magic_signature(&rom).is_some() {
            bail!("level loads need a supported layout; this Lunar Magic version is not");
        }
        trace_level(&rom, &layout, level)?
    } else {
        trace_routine(&rom, opts)?
    };
    let known = layout.known_ranges(&rom);
    print_trace(&trace, opts.gap, &known, opts.unknown_only);
    for region in &opts.dump {
        dump_wram(&wram, region)?;
    }
    Ok(())
}

/// WRAM after a run (`$7E:0000` is index 0).
type Wram = Vec<u8>;

fn trace_level(rom: &Rom, layout: &RomLayout, level: &str) -> anyhow::Result<(Trace, Wram)> {
    let number = parse_hex(level)?;
    let id = LevelId::new(u16::try_from(number).unwrap_or(u16::MAX))
        .with_context(|| format!("level {number:#X} is out of range"))?;
    let parsed = Level::load(rom, layout, id)?;
    let run = load_level_data_traced(rom, layout, parsed)?;
    println!("level {id}: loader ran {} instructions", run.loaded.steps);
    Ok((run.trace, run.wram))
}

/// Prints `len` WRAM bytes from `addr` (both hex, `addr:len`), 16 per row.
fn dump_wram(wram: &[u8], region: &str) -> anyhow::Result<()> {
    let (addr, len) =
        region.split_once(':').with_context(|| format!("{region:?} is not addr:len"))?;
    let (addr, len) = (parse_hex(addr)?, parse_hex(len)?);
    let start = usize::try_from(addr & 0x1_FFFF).unwrap_or(0);
    let bytes = wram.get(start..start.saturating_add(usize::try_from(len).unwrap_or(0)));
    let bytes = bytes.with_context(|| format!("{region:?} is outside WRAM"))?;
    println!("WRAM {}:", snes(0x7E_0000 | (addr & 0x1_FFFF)));
    for (i, row) in bytes.chunks(16).enumerate() {
        let hex: Vec<String> = row.iter().map(|b| format!("{b:02X}")).collect();
        let at = (addr & 0x1_FFFF).saturating_add(u32::try_from(i).unwrap_or(0).saturating_mul(16));
        println!("  {}  {}", snes(0x7E_0000 | at), hex.join(" "));
    }
    Ok(())
}

fn trace_routine(rom: &Rom, opts: &Options) -> anyhow::Result<(Trace, Wram)> {
    let target = parse_hex(&opts.target)?;
    let mut bus = SnesBus::new(rom);
    bus.enable_trace();
    for copy in &opts.copy {
        let mut parts = copy.split(':');
        let (Some(src), Some(dst), Some(len), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            bail!("{copy:?} is not rom:ram:len");
        };
        let (src, dst, len) = (parse_hex(src)?, parse_hex(dst)?, parse_hex(len)?);
        for i in 0..len {
            let byte = bus.peek(src.wrapping_add(i));
            bus.set_wram(dst.wrapping_add(i), byte);
        }
    }
    for seed in &opts.ram {
        let (addr, value) =
            seed.split_once('=').with_context(|| format!("{seed:?} is not addr=value"))?;
        let addr = parse_hex(addr)?;
        let digits = value.trim_start_matches("0x").len();
        let value = parse_hex(value)?;
        let [lo, mid, hi, _] = value.to_le_bytes();
        bus.set_wram(addr, lo);
        if digits > 2 {
            bus.set_wram(addr.wrapping_add(1), mid);
        }
        if digits > 4 {
            bus.set_wram(addr.wrapping_add(2), hi);
        }
    }
    let mut cpu = Cpu::new();
    cpu.brk_is_fatal = true;
    cpu.p.m = opts.m8;
    cpu.p.x = opts.x8;
    let result = match &opts.stop {
        Some(stop) => {
            let stop = parse_hex(stop)?;
            cpu.pb = u8::try_from((target >> 16) & 0xFF).unwrap_or(0);
            cpu.pc = u16::try_from(target & 0xFFFF).unwrap_or(0);
            cpu.run_until(&mut bus, stop, opts.steps)
        }
        None => cpu.call_long(&mut bus, target, SENTINEL, opts.steps),
    };
    println!("routine {}: ran {} instructions", snes(target), cpu.steps);
    match result {
        Ok(()) if opts.stop.is_some() => println!("reached the stop address"),
        Ok(()) => println!("returned to the caller"),
        Err(err) => println!("stopped early: {err} (at {})", snes(cpu.pc24())),
    }
    let trace = bus.take_trace().context("tracing was enabled")?;
    Ok((trace, bus.wram().to_vec()))
}

fn print_trace(trace: &Trace, gap: u32, known: &[KnownRange], unknown_only: bool) {
    print_rom_ranges(&trace.rom_reads, gap, known, unknown_only);
    print_ranges("WRAM written", &trace.wram_writes, gap);
    print_ranges("WRAM read before written", &trace.wram_reads_uninit, gap);
    println!("hardware registers: {} distinct", trace.hardware.len());
    for (addr, (reads, writes)) in &trace.hardware {
        println!("  {}  {reads:>6} reads  {writes:>6} writes", snes(*addr));
    }
}

/// ROM ranges, each labelled by the known table it starts in (if any).
fn print_rom_ranges(set: &BTreeSet<Addr>, gap: u32, known: &[KnownRange], unknown_only: bool) {
    let ranges = Trace::ranges(set, gap);
    println!("ROM read: {} bytes in {} ranges", set.len(), ranges.len());
    for (first, last) in ranges {
        let label = snes_rom::SnesAddr::new(first).and_then(|a| label_address(known, a));
        if unknown_only && label.is_some() {
            continue;
        }
        // "..." marks a range that runs past the table it starts in.
        let covered =
            snes_rom::SnesAddr::new(last).is_some_and(|a| known.iter().any(|r| r.contains(a)));
        let len = last.saturating_sub(first).saturating_add(1);
        println!(
            "  {}..{}  {len:>6} bytes  {}{}",
            snes(first),
            snes(last),
            label.as_deref().unwrap_or("?"),
            if label.is_some() && !covered { " ..." } else { "" }
        );
    }
}

fn print_ranges(label: &str, set: &BTreeSet<Addr>, gap: u32) {
    let ranges = Trace::ranges(set, gap);
    println!("{label}: {} bytes in {} ranges", set.len(), ranges.len());
    for (first, last) in ranges {
        let len = last.saturating_sub(first).saturating_add(1);
        println!("  {}..{}  {len:>6} bytes", snes(first), snes(last));
    }
}

/// `$BB:OOOO` for a 24-bit bus address.
fn snes(addr: Addr) -> String {
    format!("${:02X}:{:04X}", (addr >> 16) & 0xFF, addr & 0xFFFF)
}

/// Parses `04:DC57`, `04DC57` or `0x04DC57`.
fn parse_hex(s: &str) -> anyhow::Result<u32> {
    let digits: String = s
        .trim_start_matches("0x")
        .trim_start_matches("0X")
        .chars()
        .filter(|c| *c != ':' && *c != '_')
        .collect();
    if digits.is_empty() || digits.len() > 6 {
        bail!("{s:?} is not a hex address");
    }
    u32::from_str_radix(&digits, 16).with_context(|| format!("{s:?} is not a hex number"))
}

#[cfg(test)]
mod tests {
    use super::{parse_hex, snes};

    #[test]
    fn address_parsing() {
        assert_eq!(parse_hex("04:DC57").unwrap(), 0x04_DC57);
        assert_eq!(parse_hex("0x04dc57").unwrap(), 0x04_DC57);
        assert_eq!(parse_hex("105").unwrap(), 0x105);
        assert!(parse_hex("").is_err());
        assert!(parse_hex("zz").is_err());
        assert_eq!(snes(0x0C_F7DF), "$0C:F7DF");
    }
}
