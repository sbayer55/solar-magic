//! Capturing what the machine shows for a level: launch it, wait for the
//! fade-in, and snapshot VRAM, CGRAM, OAM, the PPU registers and tile RAM
//! into plain data the editor's renderer can use. `docs/formats/level-view.md`

use smw_data::RomLayout;
use smw_data::gfx::Bpp;
use smw_data::level::{Level, fields::secondary};
use smw_data::map16::Map16Tile;
use smw_render::assets::{
    ANIMATED_TILES, BG_CHARS, BgBase, CapturedFrame, LevelAssets, PpuSummary, SPRITE_CHARS,
    TILE_RAM_LEN, TileRam, decode_chars, palette_from_cgram,
};
use snes_core::{Console, Hit, Input};
use snes_rom::Rom;

use crate::boot::BootCache;
use crate::launcher::{LaunchError, Launcher};
use crate::state::{LaunchRequest, MarioState};
use crate::{WRAM, game_mode};

/// Frames allowed for the fade-in.
const FADE_BUDGET: u32 = 300;
/// Full brightness.
const FULL_BRIGHTNESS: u8 = 15;
/// Layer mask bits: BG1-4 without OBJ.
const LAYERS_NO_OBJ: u8 = 0x0F;
/// All layers.
const LAYERS_ALL: u8 = 0x1F;
/// Level tile RAM, low and high bytes.
const TILE_RAM_LOW: u32 = 0x7E_C800;
const TILE_RAM_HIGH: u32 = 0x7F_C800;
/// Bank of the Map16 tile records the pointer table names.
const MAP16_BANK: u32 = 0x0D_0000;

/// How to capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureOptions {
    /// Disable the "No Yoshi" intro so castles fade straight in.
    pub skip_intro: bool,
    /// Consecutive fully-lit level frames required before capturing.
    pub settle_frames: u32,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self { skip_intro: true, settle_frames: 2 }
    }
}

/// Why a capture failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CaptureError {
    /// The launch failed.
    #[error(transparent)]
    Launch(#[from] LaunchError),
    /// The level never reached full brightness.
    #[error("the level did not fade in within {0} frames")]
    NoFadeIn(u32),
    /// The CPU stopped.
    #[error("the CPU stopped: {0:?}")]
    Stopped(Hit),
    /// The level runs a background mode the editor does not compose.
    #[error("background mode {0} is not supported")]
    UnsupportedMode(u8),
}

/// Launches `level` from a booted console and captures its assets.
///
/// # Errors
///
/// See [`CaptureError`].
pub fn capture_level(
    cache: &mut BootCache,
    rom: &Rom,
    layout: &RomLayout,
    mut level: Level,
    options: CaptureOptions,
) -> Result<LevelAssets, CaptureError> {
    if options.skip_intro {
        let _ = level.set_secondary_field(secondary::DISABLE_NO_YOSHI_INTRO, 1);
    }
    let mut console = cache.booted(rom, layout)?;
    let request = LaunchRequest { level, mario: MarioState::default(), pixel: None };
    let mut launcher = Launcher::start_booted(&mut console, rom, layout, request);
    launcher.run_to_play(&mut console, layout)?;
    wait_faded_in(&mut console, layout, options.settle_frames)?;
    let mode = console.bus().ppu.regs.bg_mode;
    if mode != 1 {
        return Err(CaptureError::UnsupportedMode(mode));
    }
    Ok(snapshot(&mut console, layout))
}

/// Runs frames until the level has been at full brightness for `settle`
/// consecutive frames; returns at a frame start.
fn wait_faded_in(
    console: &mut Console,
    layout: &RomLayout,
    settle: u32,
) -> Result<(), CaptureError> {
    let mut lit = 0u32;
    for _ in 0..FADE_BUDGET {
        let regs = &console.bus().ppu.regs;
        let ready = game_mode(console, layout) == layout.launch.modes.level
            && regs.brightness == FULL_BRIGHTNESS
            && !regs.forced_blank;
        lit = if ready { lit.saturating_add(1) } else { 0 };
        if lit >= settle {
            return Ok(());
        }
        if let Some(hit) = console.run_frame(Input::default()).hit {
            return Err(CaptureError::Stopped(hit));
        }
    }
    Err(CaptureError::NoFadeIn(FADE_BUDGET))
}

/// Everything the editor needs, taken at a frame start.
fn snapshot(console: &mut Console, layout: &RomLayout) -> LevelAssets {
    let bus = console.bus();
    let vram = bus.ppu.vram.words().to_vec();
    let palette = palette_from_cgram(bus.ppu.cgram.colors());
    let mut oam = bus.ppu.oam.low().to_vec();
    oam.extend_from_slice(bus.ppu.oam.high());
    let top = summarize(console);
    let animated = animated_tiles(console, layout);
    let tile_ram = TileRam {
        low: peek_range(console, TILE_RAM_LOW),
        high: peek_range(console, TILE_RAM_HIGH),
    };
    let frame_number = bus.clock.frame;

    // Both frames come from this exact state: one plain, one without sprites.
    let mut without_sprites = console.clone();
    without_sprites.bus_mut().ppu.set_layer_mask(LAYERS_NO_OBJ);
    without_sprites.run_frame(Input::default());
    let frame_no_obj = captured_frame(&without_sprites);
    console.run_frame(Input::default());
    let frame = captured_frame(console);

    // The registers below the status bar: run into the next frame past the IRQ line.
    let irq_line = top.irq_line;
    console.run_while(Input::default(), |c| c.bus().clock.v <= irq_line.saturating_add(1));
    let play = summarize(console);
    console.bus_mut().ppu.set_layer_mask(LAYERS_ALL);

    LevelAssets {
        chars: decode_chars(&vram, play.bg[0].chars, BG_CHARS, Bpp::Four),
        layer3_chars: decode_chars(&vram, play.bg[2].chars, BG_CHARS, Bpp::Two),
        sprite_chars: decode_chars(&vram, play.obj_chars, SPRITE_CHARS, Bpp::Four),
        vram,
        palette,
        oam,
        top,
        play,
        animated,
        tile_ram,
        frame,
        frame_no_obj,
        frame_number,
    }
}

/// The compositing registers as they are now.
fn summarize(console: &Console) -> PpuSummary {
    let bus = console.bus();
    let regs = &bus.ppu.regs;
    let bg = |i: usize| {
        regs.bg.get(i).map_or_else(BgBase::default, |b| BgBase {
            tilemap: b.screen_addr,
            tilemap_size: b.screen_size,
            chars: b.tile_addr,
            hofs: b.hofs,
            vofs: b.vofs,
        })
    };
    PpuSummary {
        bg_mode: regs.bg_mode,
        bg3_priority: regs.bg3_priority,
        main_screen: regs.main_screen,
        sub_screen: regs.sub_screen,
        cgadsub: regs.cgadsub,
        cgwsel: regs.cgwsel,
        fixed_color: regs.fixed_color,
        brightness: regs.brightness,
        obj_chars: regs.obj_tile_addr,
        bg: [bg(0), bg(1), bg(2), bg(3)],
        irq_line: bus.regs.irq.vtime,
        hdma_enabled: bus.dma.hdma_enable,
    }
}

fn captured_frame(console: &Console) -> CapturedFrame {
    let frame = console.frame();
    CapturedFrame {
        width: u32::try_from(frame.width()).unwrap_or(0),
        height: u32::try_from(frame.height()).unwrap_or(0),
        rgba: frame.to_rgba(),
    }
}

fn peek_range(console: &Console, start: u32) -> Vec<u8> {
    (0..TILE_RAM_LEN)
        .map(|i| console.peek(start.wrapping_add(u32::try_from(i).unwrap_or(0))))
        .collect()
}

/// The animated tiles as the game's Map16 pointer table currently resolves
/// them (`docs/formats/map16.md`).
fn animated_tiles(console: &Console, layout: &RomLayout) -> Vec<(u16, Map16Tile)> {
    let table = WRAM | layout.loader.ram_map16_pointers;
    ANIMATED_TILES
        .filter_map(|n| {
            let entry = table.wrapping_add(u32::from(n).saturating_mul(2));
            let offset = console.peek_u16(entry);
            let record = MAP16_BANK | u32::from(offset);
            let bytes: Vec<u8> = (0..8u32).map(|i| console.peek(record.wrapping_add(i))).collect();
            Map16Tile::from_bytes(&bytes).map(|tile| (n, tile))
        })
        .collect()
}
