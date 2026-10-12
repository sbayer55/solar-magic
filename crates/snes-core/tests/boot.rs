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

/// FNV-1a over the frame's RGBA bytes.
fn frame_hash(rgba: &[u8]) -> u64 {
    rgba.iter()
        .fold(0xcbf2_9ce4_8422_2325u64, |h, &b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// The title screen renders real content, and matches the local golden
/// hash when `SMW_GOLDENS_DIR` is set (`UPDATE_GOLDENS=1` writes it).
#[test]
#[allow(clippy::expect_used, clippy::print_stderr, reason = "test harness")]
fn title_frame_renders() {
    let Some(rom) = smw() else { return };
    let console = run_frames(&rom, 600);
    let frame = console.frame();
    assert_eq!(frame.height, 224);
    let rgba = frame.to_rgba();
    let mut colors: Vec<&[u8]> = rgba.chunks(4).collect();
    colors.sort_unstable();
    colors.dedup();
    assert!(colors.len() > 16, "only {} distinct colours", colors.len());
    assert!(rgba.chunks(4).any(|p| p != [0, 0, 0, 0xFF]), "all black");
    let Some(dir) = std::env::var_os("SMW_GOLDENS_DIR") else {
        eprintln!("SMW_GOLDENS_DIR not set; skipping golden comparison");
        return;
    };
    let path = std::path::Path::new(&dir).join("title600.hash");
    let hash = format!("{:016x}", frame_hash(&rgba));
    if std::env::var_os("UPDATE_GOLDENS").is_some() {
        std::fs::create_dir_all(&dir).expect("goldens dir");
        std::fs::write(&path, &hash).expect("write golden");
        return;
    }
    match std::fs::read_to_string(&path) {
        Ok(want) => {
            assert_eq!(hash, want.trim(), "title frame changed; UPDATE_GOLDENS=1 to accept");
        }
        Err(_) => eprintln!("no golden at {}; run with UPDATE_GOLDENS=1", path.display()),
    }
}
