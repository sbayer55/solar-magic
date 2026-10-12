//! Boots the real game when `SMW_ROM` is set.

use snes_core::{Console, Input, Region};
use snes_rom::Rom;

#[allow(clippy::expect_used, clippy::print_stderr, reason = "test harness")]
fn smw() -> Option<Rom> {
    let Some(path) = std::env::var_os("SMW_ROM") else {
        eprintln!("SMW_ROM not set; skipping boot tests");
        return None;
    };
    Some(Rom::load(std::path::Path::new(&path)).expect("SMW_ROM loads"))
}

fn run_frames(rom: &Rom, frames: u32) -> Console {
    let mut console = Console::new(rom, Region::Ntsc);
    for _ in 0..frames {
        let report = console.run_frame(Input::default());
        assert!(report.hit.is_none(), "unexpected stop: {report:?}");
    }
    console
}

/// The boot-ROM handshake and the sound-driver upload complete, and the game
/// reaches its title screen (game mode `$07`) with the display enabled.
#[test]
fn boot_reaches_the_title_screen() {
    let Some(rom) = smw() else { return };
    let mut console = Console::new(&rom, Region::Ntsc);
    let mut mode = 0;
    for _ in 0..600 {
        console.run_frame(Input::default());
        mode = console.peek(0x7E_0100);
        if mode == 0x07 {
            break;
        }
    }
    assert_eq!(mode, 0x07, "game mode after 600 frames");
    let bus = console.bus();
    assert!(!bus.apu.smp.rom_enabled, "the sound driver should have unmapped the boot ROM");
    assert!(bus.apu.cpu.pc < 0xFFC0, "the SPC700 should be running the uploaded driver");
    assert!(!bus.ppu.regs.forced_blank);
    assert!(bus.regs.irq.nmi_enable);
    assert!(bus.ppu.vram.words().iter().filter(|&&w| w != 0).count() > 10_000);
}

#[test]
fn boot_is_deterministic() {
    let Some(rom) = smw() else { return };
    let a = run_frames(&rom, 20);
    let b = run_frames(&rom, 20);
    assert_eq!(a.cpu(), b.cpu());
    assert_eq!(a.bus().wram.bytes(), b.bus().wram.bytes());
    assert_eq!(a.bus().clock.now, b.bus().clock.now);
}
