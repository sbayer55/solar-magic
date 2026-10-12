//! `solar emu-bench`: how fast the in-house emulator runs a ROM.

use std::path::Path;
use std::time::Instant;

use anyhow::Context;
use snes_core::{Console, Input, Region};
use snes_rom::Rom;

/// NTSC frame rate.
const NTSC_FPS: f64 = 60.0988;

/// Boots `rom`, skips `warmup` frames, then times `frames` frames.
pub(crate) fn run(rom: &Path, frames: u32, warmup: u32) -> anyhow::Result<()> {
    let rom = Rom::load(rom).with_context(|| format!("loading {}", rom.display()))?;
    let mut console = Console::new(&rom, Region::Ntsc);
    for _ in 0..warmup {
        console.run_frame(Input::default());
    }
    let start = Instant::now();
    let mut instructions = 0u64;
    for _ in 0..frames {
        instructions = console.run_frame(Input::default()).instructions;
    }
    let seconds = start.elapsed().as_secs_f64();
    let fps = f64::from(frames) / seconds.max(f64::EPSILON);
    println!(
        "{frames} frames in {seconds:.2} s: {fps:.1} fps, {:.2}× real time, {instructions} instructions so far",
        fps / NTSC_FPS
    );
    Ok(())
}
