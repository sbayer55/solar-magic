//! Runs krom's open-source SNES test ROMs (<https://github.com/PeterLemon/SNES>,
//! fetched by `cargo xtask fetch-test-roms`, never vendored) and compares the
//! picture with the screenshot shipped next to each ROM.
//!
//! `SNES_TEST_ROMS=<checkout>` enables the test; `SNES_TEST_ROMS_REPORT=1`
//! prints every ROM's best match instead of asserting.

use std::path::{Path, PathBuf};

use snes_core::{Buttons, Cartridge, Console, Input, Region};
use snes_rom::{LoRom, MapperKind};

/// Button presses that bring a demo to the state its screenshot shows:
/// `count` taps of `button`, each held for eight frames and released for
/// eight (the demos poll the pad once every eight frames).
#[derive(Clone, Copy)]
struct Taps {
    button: Buttons,
    count: u32,
}

/// A ROM and the largest fraction of pixels (in 1/1000) allowed to differ
/// from its screenshot at the best of the sampled frames.
struct Expectation {
    rom: &'static str,
    max_mismatch_permille: u32,
    taps: Option<Taps>,
}

/// An expectation without input.
const fn exact(rom: &'static str) -> Expectation {
    Expectation { rom, max_mismatch_permille: 5, taps: None }
}

/// Frame at which scripted taps start.
const TAPS_START: u32 = 20;
/// Frames per tap (held, then released).
const TAP_HALF: u32 = 8;

/// ROMs whose output must match: static demos, CPU result screens, and
/// demos driven to their screenshot's state by scripted taps.
const EXPECTED: &[Expectation] = &[
    exact("CPUTest/CPU/MSC/CPUMSC.sfc"),
    exact("CPUTest/CPU/ADC/CPUADC.sfc"),
    exact("CPUTest/CPU/AND/CPUAND.sfc"),
    exact("CPUTest/CPU/ASL/CPUASL.sfc"),
    exact("CPUTest/CPU/BIT/CPUBIT.sfc"),
    exact("CPUTest/CPU/BRA/CPUBRA.sfc"),
    exact("CPUTest/CPU/CMP/CPUCMP.sfc"),
    exact("CPUTest/CPU/DEC/CPUDEC.sfc"),
    exact("CPUTest/CPU/EOR/CPUEOR.sfc"),
    exact("CPUTest/CPU/INC/CPUINC.sfc"),
    exact("CPUTest/CPU/JMP/CPUJMP.sfc"),
    exact("CPUTest/CPU/LDR/CPULDR.sfc"),
    exact("CPUTest/CPU/LSR/CPULSR.sfc"),
    exact("CPUTest/CPU/MOV/CPUMOV.sfc"),
    exact("CPUTest/CPU/ORA/CPUORA.sfc"),
    exact("CPUTest/CPU/PHL/CPUPHL.sfc"),
    exact("CPUTest/CPU/PSR/CPUPSR.sfc"),
    exact("CPUTest/CPU/RET/CPURET.sfc"),
    exact("CPUTest/CPU/ROL/CPUROL.sfc"),
    exact("CPUTest/CPU/ROR/CPUROR.sfc"),
    exact("CPUTest/CPU/SBC/CPUSBC.sfc"),
    exact("CPUTest/CPU/STR/CPUSTR.sfc"),
    exact("CPUTest/CPU/TRN/CPUTRN.sfc"),
    exact("PPU/BGMAP/8x8/2BPP/8x8BG1Map2BPP32x328PAL/8x8BG1Map2BPP32x328PAL.sfc"),
    exact("PPU/BGMAP/8x8/2BPP/8x8BG2Map2BPP32x328PAL/8x8BG2Map2BPP32x328PAL.sfc"),
    exact("PPU/BGMAP/8x8/2BPP/8x8BG3Map2BPP32x328PAL/8x8BG3Map2BPP32x328PAL.sfc"),
    exact("PPU/BGMAP/8x8/2BPP/8x8BG4Map2BPP32x328PAL/8x8BG4Map2BPP32x328PAL.sfc"),
    exact("PPU/BGMAP/8x8/4BPP/8x8BGMap4BPP32x328PAL/8x8BGMap4BPP32x328PAL.sfc"),
    exact("PPU/BGMAP/8x8/8BPP/TileFlip/8x8BGMapTileFlip.sfc"),
    exact("PPU/Blend/HiColor/HiColor1241DLair/HiColor1241DLair.sfc"),
    exact("PPU/Blend/HiColor/HiColor3840/HiColor3840.sfc"),
    exact("PPU/Blend/HiColor/HiColor575Myst/HiColor575Myst.sfc"),
    exact("PPU/GreenSpace/GreenSpace.sfc"),
    exact("PPU/HDMA/HiColor128PerTileRow/HiColor128PerTileRow.sfc"),
    exact("PPU/HDMA/HiColor128PerTileRow/TEST/DQ128PerTileRow.sfc"),
    exact("PPU/HDMA/HiColor128PerTileRow/TEST/lenna128PerTileRow.sfc"),
    exact("PPU/HDMA/HiColor128PerTileRow/TEST/mandrill128PerTileRow.sfc"),
    exact(
        "PPU/HDMA/HiColor128PerTileRow/TEST/RGB_24bits_palette_color_test_chart128PerTileRow.sfc",
    ),
    exact("PPU/HDMA/HiColor64PerTileRow/HiColor64PerTileRow.sfc"),
    exact("PPU/HDMA/HiColor64PerTileRow/TEST/DQ64PerTileRow.sfc"),
    exact("PPU/HDMA/HiColor64PerTileRow/TEST/lenna64PerTileRow.sfc"),
    exact("PPU/HDMA/HiColor64PerTileRow/TEST/mandrill64PerTileRow.sfc"),
    exact("PPU/HDMA/HiColor64PerTileRow/TEST/RGB_24bits_palette_color_test_chart64PerTileRow.sfc"),
    exact("PPU/HDMA/HiColor64PerTileRowPseudoHiRes/HiColor64PerTileRowPseudoHiRes.sfc"),
    exact("PPU/HDMA/HiColor64PerTileRowPseudoHiRes/TEST/lenna64PerTileRowHiRes.sfc"),
    exact("PPU/HDMA/HiColor64PerTileRowPseudoHiRes/TEST/mandrill64PerTileRowHiRes.sfc"),
    exact(
        "PPU/HDMA/HiColor64PerTileRowPseudoHiRes/TEST/RGB_24bits_palette_color_test_chart64PerTileRowHiRes.sfc",
    ),
    exact("PPU/HDMA/Mode7HDMA/Mode7HDMA.sfc"),
    exact("PPU/Mode7/Perspective/Perspective.sfc"),
    exact("PPU/HDMA/RedSpace9BitHDMA/RedSpace9BitHDMA.sfc"),
    exact("PPU/HDMA/RedSpaceHDMA/RedSpaceHDMA.sfc"),
    exact("PPU/HDMA/RedSpaceIndirectHDMA/RedSpaceIndirectHDMA.sfc"),
    exact("PPU/Interlace/InterlaceMystHDMA/InterlaceMystHDMA.sfc"),
    exact("PPU/Interlace/InterlaceRPG/InterlaceRPG.sfc"),
    exact("PPU/Interlace/InterlaceSimpsonsHDMA/InterlaceSimpsonsHDMA.sfc"),
    exact("PPU/Rings/Rings.sfc"),
    exact("PPU/Window/WindowHDMA/WindowHDMA.sfc"),
    // Resampled screenshot: the edges of the hi-res picture are blurred.
    Expectation {
        rom: "PPU/Interlace/InterlaceMoogle/InterlaceMoogle.sfc",
        max_mismatch_permille: 40,
        taps: None,
    },
    // 15 R presses take the mosaic from 1×1 to the 16×16 of the screenshot.
    Expectation {
        rom: "PPU/Mosaic/Mode3/MosaicMode3.sfc",
        max_mismatch_permille: 5,
        taps: Some(Taps { button: Buttons::R, count: 15 }),
    },
];

/// Frames at which the picture is compared; the best one counts.
const SAMPLE_FRAMES: [u32; 4] = [60, 120, 180, 300];

#[allow(clippy::expect_used, reason = "test harness")]
fn load_console(path: &Path) -> Console {
    let bytes = std::fs::read(path).expect("test ROM readable");
    let cart = Cartridge::from_bytes(&bytes, MapperKind::LoRom(LoRom), 0, false);
    Console::with_cartridge(cart, Region::Ntsc)
}

/// Our picture as RGBA with its dimensions.
struct Picture {
    rgba: Vec<u8>,
    width: usize,
    height: usize,
}

/// Pixels differing by more than 8/255 in any channel, in 1/1000 of the
/// compared pixels. Screenshots of hi-res or interlaced demos are 512×448
/// even when only one axis is doubled, so each axis is compared at the
/// nearest integer scale over the common area.
fn mismatch_permille(ours: &Picture, reference: &image::RgbaImage) -> u32 {
    let ref_w = usize::try_from(reference.width()).unwrap_or(0);
    let ref_h = usize::try_from(reference.height()).unwrap_or(0);
    if ours.width == 0 || ours.height == 0 {
        return 1000;
    }
    let scale = |ref_len: usize, our_len: usize| {
        ref_len.saturating_add(our_len / 2).checked_div(our_len).unwrap_or(1).max(1)
    };
    let (sx, sy) = (scale(ref_w, ours.width), scale(ref_h, ours.height));
    let width = ours.width.min(ref_w.checked_div(sx).unwrap_or(0));
    let height = ours.height.min(ref_h.checked_div(sy).unwrap_or(0));
    // A screenshot with spare rows (225 for a 224-line picture) may start a
    // line early; the best alignment counts.
    let spare = ref_h.saturating_sub(height.saturating_mul(sy)).min(2);
    let total = u64::try_from(width.saturating_mul(height)).unwrap_or(1).max(1);
    (0..=spare)
        .map(|dy| {
            let mut bad = 0u64;
            for y in 0..height {
                for x in 0..width {
                    let i = y.saturating_mul(ours.width).saturating_add(x).saturating_mul(4);
                    let a = ours.rgba.get(i..i.saturating_add(3)).unwrap_or(&[0, 0, 0]);
                    let (rx, ry) = (x.saturating_mul(sx), y.saturating_mul(sy).saturating_add(dy));
                    let p = reference
                        .get_pixel(u32::try_from(rx).unwrap_or(0), u32::try_from(ry).unwrap_or(0))
                        .0;
                    if a.iter().zip(p.iter()).any(|(&l, &r)| l.abs_diff(r) > 8) {
                        bad = bad.saturating_add(1);
                    }
                }
            }
            u32::try_from(bad.saturating_mul(1000).checked_div(total).unwrap_or(1000))
                .unwrap_or(1000)
        })
        .min()
        .unwrap_or(1000)
}

/// Runs one ROM and returns its best mismatch over the sampled frames.
/// With `SNES_TEST_ROMS_DUMP=<dir>`, writes the best frame as a PNG.
#[allow(clippy::expect_used, reason = "test harness")]
fn best_mismatch(rom: &Path, reference: &image::RgbaImage, taps: Option<Taps>) -> u32 {
    let mut console = load_console(rom);
    let mut best: (u32, Option<Picture>) = (1000, None);
    let mut frame = 0u32;
    for &target in &SAMPLE_FRAMES {
        while frame < target {
            console.run_frame(input_at(frame, taps));
            frame = frame.saturating_add(1);
        }
        let picture = console.frame();
        let ours =
            Picture { rgba: picture.to_rgba(), width: picture.width(), height: picture.height() };
        let score = mismatch_permille(&ours, reference);
        if score < best.0 {
            best = (score, Some(ours));
        }
    }
    if let (Some(dir), Some(ours)) = (std::env::var_os("SNES_TEST_ROMS_DUMP"), best.1) {
        let name = rom.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let path = PathBuf::from(dir).join(format!("{name}.png"));
        let img: image::RgbaImage = image::ImageBuffer::from_raw(
            u32::try_from(ours.width).unwrap_or(256),
            u32::try_from(ours.height).unwrap_or(224),
            ours.rgba,
        )
        .expect("frame buffer");
        img.save(path).expect("dump");
    }
    best.0
}

/// The pad state at `frame` under a tap script.
fn input_at(frame: u32, taps: Option<Taps>) -> Input {
    let Some(taps) = taps else { return Input::default() };
    let since = frame.saturating_sub(TAPS_START);
    let period = TAP_HALF.saturating_mul(2);
    let held = frame >= TAPS_START
        && since.checked_div(period).unwrap_or(0) < taps.count
        && since.checked_rem(period).unwrap_or(0) < TAP_HALF;
    Input { p1: if held { taps.button } else { Buttons::empty() }, p2: Buttons::empty() }
}

/// The screenshot next to the ROM (a few `.png` files are really BMPs).
fn reference_for(rom: &Path) -> Option<image::RgbaImage> {
    let png = rom.with_extension("png");
    let reader = image::ImageReader::open(png).ok()?.with_guessed_format().ok()?;
    reader.decode().ok().map(|i| i.to_rgba8())
}

#[test]
#[allow(clippy::print_stderr, reason = "test harness")]
fn test_roms_match_their_screenshots() {
    let Some(root) = std::env::var_os("SNES_TEST_ROMS") else {
        eprintln!("SNES_TEST_ROMS not set; skipping test-ROM goldens");
        return;
    };
    // Cargo runs the test from the crate directory; a relative path that
    // does not exist there is taken from the workspace root.
    let mut root = PathBuf::from(root);
    if root.is_relative() && !root.exists() {
        root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(root);
    }
    if std::env::var_os("SNES_TEST_ROMS_REPORT").is_some() {
        report_all(&root);
        return;
    }
    let mut failures = Vec::new();
    for e in EXPECTED {
        let rom = root.join(e.rom);
        let Some(reference) = reference_for(&rom) else {
            failures.push(format!("{}: no screenshot", e.rom));
            continue;
        };
        let got = best_mismatch(&rom, &reference, e.taps);
        eprintln!("{}: {got}‰ differ (limit {}‰)", e.rom, e.max_mismatch_permille);
        if got > e.max_mismatch_permille {
            failures.push(format!("{}: {got}‰ > {}‰", e.rom, e.max_mismatch_permille));
        }
    }
    assert!(failures.is_empty(), "test ROMs off their screenshots:\n{}", failures.join("\n"));
}

/// Walks every `.sfc` with a sibling `.png` and prints the best match.
#[allow(clippy::print_stderr, clippy::expect_used, reason = "test harness")]
fn report_all(root: &Path) {
    let mut roms = Vec::new();
    collect(root, &mut roms);
    roms.sort();
    for rom in roms {
        let Some(reference) = reference_for(&rom) else { continue };
        let got = best_mismatch(&rom, &reference, None);
        let name = rom.strip_prefix(root).unwrap_or(&rom).display();
        eprintln!("REPORT {got:4}‰  {name}");
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "sfc") {
            out.push(path);
        }
    }
}
