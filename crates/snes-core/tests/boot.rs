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

/// Without an APU the game waits for the `$BBAA` handshake forever: the CPU
/// must be parked in that loop with the expected signature in A.
#[test]
fn boot_reaches_the_apu_handshake_wait() {
    let Some(rom) = smw() else { return };
    let console = run_frames(&rom, 30);
    assert_eq!(console.cpu().a, 0xBBAA);
    assert_eq!(console.cpu().pb, 0);
    assert!(!console.cpu().e, "the game switches to native mode before the handshake");
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
