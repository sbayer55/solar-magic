//! Runs krom's open-source SNES test ROMs (<https://github.com/PeterLemon/SNES>,
//! fetched by `cargo xtask fetch-test-roms`, never vendored) and compares the
//! picture with the screenshot shipped next to each ROM.
//!
//! `SNES_TEST_ROMS=<checkout>` enables the test; `SNES_TEST_ROMS_REPORT=1`
//! prints every ROM's best match instead of asserting.

use std::path::{Path, PathBuf};

use snes_core::{Cartridge, Console, Input, Region};
use snes_rom::{LoRom, MapperKind};

/// A ROM and the largest fraction of pixels (in 1/1000) allowed to differ
/// from its screenshot at the best of the sampled frames.
struct Expectation {
    rom: &'static str,
    max_mismatch_permille: u32,
}

/// ROMs whose output must match (static demos and CPU result screens).
const EXPECTED: &[Expectation] = &[
    Expectation { rom: "CPUTest/CPU/MSC/CPUMSC.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/ADC/CPUADC.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/AND/CPUAND.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/ASL/CPUASL.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/BIT/CPUBIT.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/BRA/CPUBRA.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/CMP/CPUCMP.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/DEC/CPUDEC.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/EOR/CPUEOR.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/INC/CPUINC.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/JMP/CPUJMP.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/LDR/CPULDR.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/LSR/CPULSR.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/MOV/CPUMOV.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/ORA/CPUORA.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/PHL/CPUPHL.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/PSR/CPUPSR.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/RET/CPURET.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/ROL/CPUROL.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/ROR/CPUROR.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/SBC/CPUSBC.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/STR/CPUSTR.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "CPUTest/CPU/TRN/CPUTRN.sfc", max_mismatch_permille: 5 },
    Expectation {
        rom: "PPU/BGMAP/8x8/2BPP/8x8BG1Map2BPP32x328PAL/8x8BG1Map2BPP32x328PAL.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/BGMAP/8x8/2BPP/8x8BG2Map2BPP32x328PAL/8x8BG2Map2BPP32x328PAL.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/BGMAP/8x8/2BPP/8x8BG3Map2BPP32x328PAL/8x8BG3Map2BPP32x328PAL.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/BGMAP/8x8/2BPP/8x8BG4Map2BPP32x328PAL/8x8BG4Map2BPP32x328PAL.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/BGMAP/8x8/4BPP/8x8BGMap4BPP32x328PAL/8x8BGMap4BPP32x328PAL.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/BGMAP/8x8/8BPP/TileFlip/8x8BGMapTileFlip.sfc",
        max_mismatch_permille: 5,
    },
    Expectation { rom: "PPU/Blend/HiColor/HiColor3840/HiColor3840.sfc", max_mismatch_permille: 5 },
    Expectation {
        rom: "PPU/Blend/HiColor/HiColor575Myst/HiColor575Myst.sfc",
        max_mismatch_permille: 5,
    },
    Expectation { rom: "PPU/GreenSpace/GreenSpace.sfc", max_mismatch_permille: 5 },
    Expectation {
        rom: "PPU/HDMA/HiColor128PerTileRow/HiColor128PerTileRow.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/HDMA/HiColor128PerTileRow/TEST/DQ128PerTileRow.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/HDMA/HiColor128PerTileRow/TEST/lenna128PerTileRow.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/HDMA/HiColor128PerTileRow/TEST/mandrill128PerTileRow.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/HDMA/HiColor128PerTileRow/TEST/RGB_24bits_palette_color_test_chart128PerTileRow.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/HDMA/HiColor64PerTileRow/HiColor64PerTileRow.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/HDMA/HiColor64PerTileRow/TEST/DQ64PerTileRow.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/HDMA/HiColor64PerTileRow/TEST/lenna64PerTileRow.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/HDMA/HiColor64PerTileRow/TEST/mandrill64PerTileRow.sfc",
        max_mismatch_permille: 5,
    },
    Expectation {
        rom: "PPU/HDMA/HiColor64PerTileRow/TEST/RGB_24bits_palette_color_test_chart64PerTileRow.sfc",
        max_mismatch_permille: 5,
    },
    Expectation { rom: "PPU/HDMA/Mode7HDMA/Mode7HDMA.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "PPU/HDMA/RedSpace9BitHDMA/RedSpace9BitHDMA.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "PPU/HDMA/RedSpaceHDMA/RedSpaceHDMA.sfc", max_mismatch_permille: 5 },
    Expectation {
        rom: "PPU/HDMA/RedSpaceIndirectHDMA/RedSpaceIndirectHDMA.sfc",
        max_mismatch_permille: 5,
    },
    Expectation { rom: "PPU/Rings/Rings.sfc", max_mismatch_permille: 5 },
    Expectation { rom: "PPU/Window/WindowHDMA/WindowHDMA.sfc", max_mismatch_permille: 5 },
];

/// Frames at which the picture is compared; the best one counts.
const SAMPLE_FRAMES: [u32; 4] = [60, 120, 180, 300];

#[allow(clippy::expect_used, reason = "test harness")]
fn load_console(path: &Path) -> Console {
    let bytes = std::fs::read(path).expect("test ROM readable");
    let cart = Cartridge::from_bytes(&bytes, MapperKind::LoRom(LoRom), 0, false);
    Console::with_cartridge(cart, Region::Ntsc)
}

/// Pixels (of the smaller common area) differing by more than 8/255 in any
/// channel, in 1/1000 of the compared pixels.
fn mismatch_permille(ours: &[u8], ours_height: usize, reference: &image::RgbaImage) -> u32 {
    let width = 256usize.min(usize::try_from(reference.width()).unwrap_or(0));
    let height = ours_height.min(usize::try_from(reference.height()).unwrap_or(0));
    let mut bad = 0u64;
    for y in 0..height {
        for x in 0..width {
            let i = y.saturating_mul(256).saturating_add(x).saturating_mul(4);
            let a = ours.get(i..i.saturating_add(3)).unwrap_or(&[0, 0, 0]);
            let p =
                reference.get_pixel(u32::try_from(x).unwrap_or(0), u32::try_from(y).unwrap_or(0)).0;
            if a.iter().zip(p.iter()).any(|(&l, &r)| l.abs_diff(r) > 8) {
                bad = bad.saturating_add(1);
            }
        }
    }
    let total = u64::try_from(width.saturating_mul(height)).unwrap_or(1).max(1);
    u32::try_from(bad.saturating_mul(1000).checked_div(total).unwrap_or(1000)).unwrap_or(1000)
}

/// Runs one ROM and returns its best mismatch over the sampled frames.
/// With `SNES_TEST_ROMS_DUMP=<dir>`, writes the best frame as a PNG.
#[allow(clippy::expect_used, reason = "test harness")]
fn best_mismatch(rom: &Path, reference: &image::RgbaImage) -> u32 {
    let mut console = load_console(rom);
    let mut best = (1000, None);
    let mut frame = 0u32;
    for &target in &SAMPLE_FRAMES {
        while frame < target {
            console.run_frame(Input::default());
            frame = frame.saturating_add(1);
        }
        let picture = console.frame();
        let rgba = picture.to_rgba();
        let score = mismatch_permille(&rgba, picture.height, reference);
        if score < best.0 {
            best = (score, Some((rgba, picture.height)));
        }
    }
    if let (Some(dir), Some((rgba, height))) = (std::env::var_os("SNES_TEST_ROMS_DUMP"), best.1) {
        let name = rom.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let path = PathBuf::from(dir).join(format!("{name}.png"));
        let img: image::RgbaImage =
            image::ImageBuffer::from_raw(256, u32::try_from(height).unwrap_or(224), rgba)
                .expect("frame buffer");
        img.save(path).expect("dump");
    }
    best.0
}

fn reference_for(rom: &Path) -> Option<image::RgbaImage> {
    let png = rom.with_extension("png");
    image::open(png).ok().map(|i| i.to_rgba8())
}

#[test]
#[allow(clippy::print_stderr, reason = "test harness")]
fn test_roms_match_their_screenshots() {
    let Some(root) = std::env::var_os("SNES_TEST_ROMS") else {
        eprintln!("SNES_TEST_ROMS not set; skipping test-ROM goldens");
        return;
    };
    let root = PathBuf::from(root);
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
        let got = best_mismatch(&rom, &reference);
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
        let got = best_mismatch(&rom, &reference);
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
