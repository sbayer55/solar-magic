//! `solar emu-boot`: boot a ROM in the in-house emulator for N frames and
//! report where the CPU ended up.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Context;
use snes_core::{Console, Input, Region};
use snes_rom::Rom;

/// Boots `rom` for `frames` frames, then samples `trace` instructions.
pub(crate) fn run(rom: &Path, frames: u32, trace: usize) -> anyhow::Result<()> {
    let rom = Rom::load(rom).with_context(|| format!("loading {}", rom.display()))?;
    let mut console = Console::new(&rom, Region::Ntsc);
    let mut report = None;
    for _ in 0..frames {
        let r = console.run_frame(Input::default());
        if r.hit.is_some() {
            report = Some(r);
            break;
        }
        report = Some(r);
    }
    let cpu = console.cpu();
    println!(
        "frames: {}  master cycles: {}  instructions: {}",
        console.bus().clock.frame,
        console.bus().clock.now,
        cpu.steps
    );
    println!(
        "pc: {:02X}:{:04X}  a: {:04X} x: {:04X} y: {:04X} s: {:04X} d: {:04X} db: {:02X} p: {:02X} e: {} waiting: {}",
        cpu.pb,
        cpu.pc,
        cpu.a,
        cpu.x,
        cpu.y,
        cpu.s,
        cpu.d,
        cpu.db,
        cpu.p.to_byte(),
        cpu.e,
        cpu.waiting
    );
    if let Some(hit) = report.and_then(|r| r.hit) {
        println!("stopped: {hit:?}");
    }
    println!("game mode ($7E:0100): {:02X}", console.peek(0x7E_0100));
    let apu = &console.bus().apu;
    println!(
        "apu: pc {:04X} cycles {} ports in {:02X?} out {:02X?} rom_enabled {}",
        apu.cpu.pc,
        apu.cycles(),
        apu.smp.inbox,
        apu.smp.outbox,
        apu.smp.rom_enabled
    );
    let vram_used = console.bus().ppu.vram.words().iter().filter(|&&w| w != 0).count();
    println!(
        "vram nonzero words: {vram_used}  forced blank: {}",
        console.bus().ppu.regs.forced_blank
    );
    println!(
        "nmi enabled: {}  vblank: {}  v: {}",
        console.bus().regs.irq.nmi_enable,
        console.bus().clock.vblank(),
        console.bus().clock.v
    );
    if trace > 0 {
        let mut histogram: BTreeMap<u32, u32> = BTreeMap::new();
        for _ in 0..trace {
            let slot: &mut u32 = histogram.entry(console.cpu().pc24()).or_default();
            *slot = slot.saturating_add(1);
            if console.step_instruction().is_err() {
                break;
            }
        }
        let mut hot: Vec<(u32, u32)> = histogram.into_iter().collect();
        hot.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
        println!("hottest instruction addresses over the next {trace} instructions:");
        for (pc, n) in hot.iter().take(16) {
            println!("  {:02X}:{:04X}  {n}", pc >> 16, pc & 0xFFFF);
        }
    }
    Ok(())
}
