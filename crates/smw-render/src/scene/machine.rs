//! The machine-exact picture: layers 1–3 as index planes from the captured
//! VRAM characters and tilemaps, composed with the captured registers and
//! CGRAM. `docs/formats/level-view.md`

use std::collections::HashMap;

use smw_data::gfx::Bpp;
use smw_data::map16::{Map16Tile, Tile8};

use super::{LevelScene, Map16Lookup, RenderOptions, background_tile};
use crate::CharAtlas;
use crate::assets::{BgBase, LevelAssets};
use crate::compose::{ComposeRegs, Planes, compose};
use crate::level::TileGrid;
use crate::plane::{Cell, IndexPlane, PlaneContext, draw_map16_indexed};

/// Characters decoded from the capture.
#[derive(Debug)]
pub(super) struct MachineAtlas {
    /// 4bpp background characters.
    bg: CharAtlas,
    /// 2bpp layer 3 characters.
    layer3: CharAtlas,
    /// The animated Map16 tiles at capture time.
    animated: HashMap<u16, Map16Tile>,
}

impl MachineAtlas {
    pub(super) fn new(assets: &LevelAssets) -> Self {
        Self {
            bg: CharAtlas::from_chars(&assets.chars),
            layer3: CharAtlas::from_chars(&assets.layer3_chars),
            animated: assets.animated.iter().copied().collect(),
        }
    }
}

/// `v` modulo `m` (0 when `m` is 0).
#[allow(clippy::arithmetic_side_effects, reason = "guarded against a zero modulus")]
const fn wrap(v: u32, m: u32) -> u32 {
    if m == 0 { 0 } else { v % m }
}

/// Pixel size of a tilemap for `BGnSC` bits 0-1.
const fn tilemap_pixels(size: u8) -> (u32, u32) {
    match size & 3 {
        0 => (256, 256),
        1 => (512, 256),
        2 => (256, 512),
        _ => (512, 512),
    }
}

/// Word address of the tilemap entry at map tile (`tx`, `ty`).
#[allow(clippy::as_conversions, reason = "lossless u16 -> u32 in a const fn")]
const fn map_entry_addr(bg: BgBase, tx: u32, ty: u32) -> u32 {
    let screen = match bg.tilemap_size & 3 {
        0 => 0,
        1 => (tx >> 5) & 1,
        2 => (ty >> 5) & 1,
        _ => ((tx >> 5) & 1) | (((ty >> 5) & 1) << 1),
    };
    (bg.tilemap as u32)
        .wrapping_add(screen.wrapping_mul(0x400))
        .wrapping_add((ty & 31).wrapping_mul(32))
        .wrapping_add(tx & 31)
}

impl LevelScene {
    /// Composes the picture from the captured assets.
    pub(super) fn render_machine(&self, options: RenderOptions) -> Raster {
        let (w, h) = self.pixel_size();
        let (Some(assets), Some(atlas)) = (self.assets.as_deref(), self.machine_atlas.as_ref())
        else {
            return self.render_plain(options);
        };
        let ctx = PlaneContext { atlas: &atlas.bg, bpp: Bpp::Four };
        // Layer 2 is placed where the game shows it relative to layer 1 at
        // the entrance: the scroll difference between BG2 and BG1.
        let shift = scroll_difference(assets.play.bg[1], assets.play.bg[0]);
        let bg1 = options.layer1.then(|| {
            let mut plane = IndexPlane::new(w, h);
            let draw = GridDraw { atlas, ctx, shift: (0, 0) };
            self.draw_grid_indexed(&mut plane, &self.loaded.layer1, draw);
            plane
        });
        let bg2 = options.layer2.then(|| {
            let mut plane = IndexPlane::new(w, h);
            if let Some(grid) = &self.loaded.layer2 {
                self.draw_grid_indexed(&mut plane, grid, GridDraw { atlas, ctx, shift });
            }
            self.draw_background_indexed(&mut plane, ctx, shift);
            plane
        });
        let bg3 = options.layer3.then(|| layer3_plane(assets, &atlas.layer3, (w, h)));
        let regs = ComposeRegs {
            bg3_priority: assets.play.bg3_priority,
            main_screen: assets.play.main_screen,
            sub_screen: assets.play.sub_screen,
            cgadsub: assets.play.cgadsub,
            cgwsel: assets.play.cgwsel,
            fixed_color: assets.play.fixed_color,
            brightness: assets.play.brightness,
        };
        let planes = Planes { bg: [bg1.as_ref(), bg2.as_ref(), bg3.as_ref()] };
        compose(planes, regs, &assets.palette, (w, h))
    }

    /// A Map16 grid into a plane, with the animated tiles as captured. The
    /// grid's pixel `(x, y)` lands at level pixel `(x, y) − shift`.
    fn draw_grid_indexed(&self, plane: &mut IndexPlane, grid: &TileGrid, draw: GridDraw<'_>) {
        let GridDraw { atlas, ctx, shift } = draw;
        let lookup: Map16Lookup<'_> = self.map16();
        for y in 0..grid.height() {
            for x in 0..grid.width() {
                let Some(n) = grid.get(x, y) else { continue };
                let Some(tile) = atlas.animated.get(&n).or_else(|| lookup.get(n)) else {
                    continue;
                };
                let px =
                    i32::try_from(x).unwrap_or(i32::MAX).saturating_mul(16).saturating_sub(shift.0);
                let py =
                    i32::try_from(y).unwrap_or(i32::MAX).saturating_mul(16).saturating_sub(shift.1);
                let (Ok(px), Ok(py)) = (u32::try_from(px), u32::try_from(py)) else { continue };
                draw_map16_indexed(plane, (px, py), tile, ctx);
            }
        }
    }

    /// The repeating background tilemap into a plane, sampled per pixel so
    /// the shift wraps with the tilemap's period (two screens wide,
    /// `rows` tiles tall).
    fn draw_background_indexed(
        &self,
        plane: &mut IndexPlane,
        ctx: PlaneContext<'_>,
        shift: (i32, i32),
    ) {
        let Some(background) = &self.loaded.background else { return };
        let rows = self.loaded.background_rows;
        let period = (512u32, u32::from(rows.max(1)).saturating_mul(16));
        for y in 0..plane.height() {
            for x in 0..plane.width() {
                let bx = wrap(x.wrapping_add_signed(shift.0), period.0);
                let by = wrap(y.wrapping_add_signed(shift.1), period.1);
                let Some(tile) = background_tile(
                    background,
                    rows,
                    usize::try_from(bx / 16).unwrap_or(0),
                    usize::try_from(by / 16).unwrap_or(0),
                )
                .and_then(|n| self.bg_set.get(usize::from(n))) else {
                    continue;
                };
                plane_pixel(plane, (x, y), tile, (bx % 16, by % 16), ctx);
            }
        }
    }
}

/// How a Map16 grid is drawn into a plane.
#[derive(Clone, Copy)]
struct GridDraw<'a> {
    /// Captured characters and animated tiles.
    atlas: &'a MachineAtlas,
    /// Characters and depth.
    ctx: PlaneContext<'a>,
    /// Scroll difference to subtract from the grid's pixel position.
    shift: (i32, i32),
}

/// BG2's scroll minus BG1's: where layer 2 sits relative to layer 1.
fn scroll_difference(bg2: BgBase, bg1: BgBase) -> (i32, i32) {
    let delta = |a: u16, b: u16| i32::from(a).saturating_sub(i32::from(b));
    (delta(bg2.hofs, bg1.hofs), delta(bg2.vofs, bg1.vofs))
}

/// One pixel of a Map16 tile (at tile-local `(tx, ty)`) into the plane.
fn plane_pixel(
    plane: &mut IndexPlane,
    at: (u32, u32),
    tile: &Map16Tile,
    local: (u32, u32),
    ctx: PlaneContext<'_>,
) {
    let (tx, ty) = local;
    let quadrant = match (tx >= 8, ty >= 8) {
        (false, false) => tile.top_left,
        (false, true) => tile.bottom_left,
        (true, false) => tile.top_right,
        (true, true) => tile.bottom_right,
    };
    let (px, py) = (tx % 8, ty % 8);
    let sx = if quadrant.xflip { 7u32.saturating_sub(px) } else { px };
    let sy = if quadrant.yflip { 7u32.saturating_sub(py) } else { py };
    let index = ctx
        .atlas
        .get(usize::from(quadrant.num))
        .and_then(|c| c.get(usize::try_from(sx).unwrap_or(0), usize::try_from(sy).unwrap_or(0)))
        .unwrap_or(0);
    if index != 0 {
        let base = quadrant.palette.wrapping_mul(ctx.bpp.colors());
        plane.put(at.0, at.1, Cell::opaque(base.wrapping_add(index), quadrant.priority));
    }
}

/// Layer 3 as the machine shows it, laid over the level in level pixel
/// coordinates: BG3's tilemap is positioned relative to BG1's camera and
/// repeats with its size; the rows the status bar occupies (the top-of-frame
/// scroll up to the IRQ line) are left transparent.
fn layer3_plane(assets: &LevelAssets, atlas: &CharAtlas, size: (u32, u32)) -> IndexPlane {
    let (w, h) = size;
    let mut plane = IndexPlane::new(w, h);
    let cam = assets.play.bg[0];
    let bg3 = assets.play.bg[2];
    let (map_w, map_h) = tilemap_pixels(bg3.tilemap_size);
    let status_top = wrap(u32::from(assets.top.bg[2].vofs), map_h);
    let status_rows = u32::from(assets.top.irq_line);
    for y in 0..h {
        let my = wrap(y.wrapping_sub(u32::from(cam.vofs)).wrapping_add(u32::from(bg3.vofs)), map_h);
        if wrap(my.wrapping_sub(status_top), map_h) < status_rows {
            continue;
        }
        for x in 0..w {
            let mx =
                wrap(x.wrapping_sub(u32::from(cam.hofs)).wrapping_add(u32::from(bg3.hofs)), map_w);
            let entry = assets
                .vram
                .get(usize::try_from(map_entry_addr(bg3, mx / 8, my / 8) & 0x7FFF).unwrap_or(0))
                .copied()
                .unwrap_or(0);
            let tile = Tile8::from_word(entry);
            let (px, py) = (mx % 8, my % 8);
            let sx = if tile.xflip { 7u32.saturating_sub(px) } else { px };
            let sy = if tile.yflip { 7u32.saturating_sub(py) } else { py };
            let index = atlas
                .get(usize::from(tile.num))
                .and_then(|c| {
                    c.get(usize::try_from(sx).unwrap_or(0), usize::try_from(sy).unwrap_or(0))
                })
                .unwrap_or(0);
            if index != 0 {
                plane.put(
                    x,
                    y,
                    Cell::opaque(tile.palette.wrapping_mul(4).wrapping_add(index), tile.priority),
                );
            }
        }
    }
    plane
}

use crate::Raster;
