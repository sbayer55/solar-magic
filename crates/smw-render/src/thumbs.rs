//! Object thumbnails for the editor's picker: each object drawn alone by the
//! game's own loader in a level's tileset.
//!
//! A batch of objects is placed in a scratch copy of the level, one object
//! every other screen, and the loader runs once. Each thumbnail is then cut
//! out of the result using the ownership grid: a tile belongs to an object
//! when the object wrote it and it differs from the same level loaded with no
//! objects. (The loader's own writes after the last object are attributed to
//! that object, so ownership alone is not enough.)

use smw_data::RomLayout;
use smw_data::level::{Layer, Level, ObjectSpec, SpriteList, TilePos};
use snes_rom::Rom;

use crate::level::{LoadedLevel, TileGrid, load_level_data};
use crate::ownership::TileRect;
use crate::scene::LevelScene;
use crate::{Raster, TileContext, draw_map16_tile};

/// Room left of (horizontal) or above (vertical) each placement, in tiles,
/// for objects that grow toward the origin.
const LEAD: u16 = 4;
/// Row (horizontal) or column (vertical) objects are placed at.
const CROSS: u16 = 8;
/// Tiles between placements: every other screen, so a wide object does not
/// run into its neighbour.
const SPACING: u16 = 32;

/// One object drawn by itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    /// The tiles the object wrote, at 1:1 scale; tiles it did not write are
    /// the level's backdrop colour.
    pub raster: Raster,
    /// Tile offset from the placement position to the top-left of `raster`
    /// (negative when the object grows up or left).
    pub offset: (i32, i32),
    /// Size in tiles.
    pub tiles: (usize, usize),
}

/// Draws each `(number, third)` object alone in `base`'s tileset and palette.
///
/// Entries are `None` when the object draws nothing, the loader fails on it,
/// or it runs out of its slot (a column taller than the screen wraps into the
/// next screens' tile RAM; try a smaller size).
#[must_use]
pub fn object_thumbnails(
    rom: &Rom,
    layout: &RomLayout,
    base: &LevelScene,
    requests: &[(u8, u8)],
) -> Vec<Option<Thumbnail>> {
    let scratch = scratch_level(&base.loaded.level);
    let Ok(empty) = load_level_data(rom, layout, scratch.clone()) else {
        return vec![None; requests.len()];
    };
    let run = Run { rom, layout, base, scratch: &scratch, empty: &empty.layer1 };
    let per_batch = usize::from(scratch.header.screens() / 2).max(1);
    let mut out = Vec::with_capacity(requests.len());
    for chunk in requests.chunks(per_batch) {
        match run.batch(chunk) {
            Some(thumbs) => out.extend(thumbs),
            // One bad object fails the whole run; retry one at a time so the
            // others still get a picture.
            None => out.extend(
                chunk
                    .iter()
                    .map(|&request| run.batch(&[request]).and_then(|mut v| v.pop()).flatten()),
            ),
        }
    }
    out
}

/// The level with no layer 1 objects or sprites and as many screens as it
/// may have.
fn scratch_level(level: &Level) -> Level {
    let mut scratch = level.clone();
    scratch.layer1.objects.clear();
    scratch.sprites = SpriteList { header: scratch.sprites.header, sprites: Vec::new() };
    // Ignore the error: the level keeps its own screen count, which only
    // makes batches smaller.
    let _ = scratch.set_screens(scratch.max_screens());
    scratch
}

/// Where the `slot`th object of a batch goes.
fn slot_position(slot: usize, vertical: bool) -> Option<TilePos> {
    let along = u16::try_from(slot).ok()?.checked_mul(SPACING)?.checked_add(LEAD)?;
    Some(if vertical { TilePos::new(CROSS, along) } else { TilePos::new(along, CROSS) })
}

/// What every batch shares.
struct Run<'a> {
    rom: &'a Rom,
    layout: &'a RomLayout,
    base: &'a LevelScene,
    scratch: &'a Level,
    /// Layer 1 of `scratch` with no objects.
    empty: &'a TileGrid,
}

impl Run<'_> {
    /// Loads one batch; `None` if the loader fails.
    fn batch(&self, requests: &[(u8, u8)]) -> Option<Vec<Option<Thumbnail>>> {
        let mut level = self.scratch.clone();
        let vertical = level.vertical;
        // (rank among the placed objects, position) per request.
        let mut placed = Vec::with_capacity(requests.len());
        let mut placed_count = 0usize;
        for (slot, &(number, third)) in requests.iter().enumerate() {
            let pos = slot_position(slot, vertical)?;
            let spec = ObjectSpec { number, third, pos };
            let ok = level.insert_object(Layer::One, usize::MAX, spec).is_ok();
            placed.push(ok.then_some((placed_count, pos)));
            placed_count = placed_count.saturating_add(usize::from(ok));
        }
        let loaded = load_level_data(self.rom, self.layout, level).ok()?;
        let thumbs = placed
            .iter()
            .map(|entry| {
                let (rank, pos) = (*entry)?;
                let index = loaded.level.layer1.index_of_rank(rank)?;
                self.cut_out(&loaded, index, pos)
            })
            .collect();
        Some(thumbs)
    }

    /// Whether object `owner` changed tile `(x, y)`.
    fn drew(&self, loaded: &LoadedLevel, owner: u16, (x, y): (usize, usize)) -> bool {
        loaded.ownership.layer1.get(x, y).flatten() == Some(owner)
            && loaded.layer1.get(x, y) != self.empty.get(x, y)
    }

    /// The bounding box of the tiles object `owner` changed.
    fn drawn_rect(&self, loaded: &LoadedLevel, owner: u16) -> Option<TileRect> {
        let rough = (*loaded.ownership.bounds1.get(usize::from(owner))?)?;
        let mut rect: Option<(usize, usize, usize, usize)> = None;
        for y in rough.y..rough.y.saturating_add(rough.h) {
            for x in rough.x..rough.x.saturating_add(rough.w) {
                if !self.drew(loaded, owner, (x, y)) {
                    continue;
                }
                let (x0, y0, x1, y1) = rect.unwrap_or((x, y, x, y));
                rect = Some((x0.min(x), y0.min(y), x1.max(x), y1.max(y)));
            }
        }
        let (x0, y0, x1, y1) = rect?;
        let w = x1.checked_sub(x0)?.checked_add(1)?;
        let h = y1.checked_sub(y0)?.checked_add(1)?;
        Some(TileRect { x: x0, y: y0, w, h })
    }

    /// The tiles object `index` drew, on the backdrop.
    fn cut_out(&self, loaded: &LoadedLevel, index: usize, pos: TilePos) -> Option<Thumbnail> {
        let owner = u16::try_from(index).ok()?;
        let rect = self.drawn_rect(loaded, owner)?;
        if !in_slot(rect, pos, loaded.vertical) {
            return None;
        }
        let assets = &*self.base.assets;
        let backdrop = assets.palette.get(0, 0).unwrap_or_default().to_rgba8();
        let mut raster = Raster::filled(to_px(rect.w)?, to_px(rect.h)?, backdrop);
        let ctx =
            TileContext { atlas: &assets.atlas, palette: &assets.palette, opaque_zero: false };
        let lookup = self.base.map16();
        for dy in 0..rect.h {
            for dx in 0..rect.w {
                let (x, y) = (rect.x.checked_add(dx)?, rect.y.checked_add(dy)?);
                if !self.drew(loaded, owner, (x, y)) {
                    continue;
                }
                let Some(tile) = loaded.layer1.get(x, y).and_then(|n| lookup.get(n)) else {
                    continue;
                };
                draw_map16_tile(&mut raster, (to_px(dx)?, to_px(dy)?), tile, ctx);
            }
        }
        let offset = (diff(rect.x, pos.x)?, diff(rect.y, pos.y)?);
        Some(Thumbnail { raster, offset, tiles: (rect.w, rect.h) })
    }
}

/// Whether `rect` stays inside the band of `SPACING` tiles its object was
/// placed in.
fn in_slot(rect: TileRect, pos: TilePos, vertical: bool) -> bool {
    let (start, len, along) =
        if vertical { (rect.y, rect.h, pos.y) } else { (rect.x, rect.w, pos.x) };
    let band = usize::from(along.saturating_sub(LEAD));
    start >= band && start.saturating_add(len) <= band.saturating_add(usize::from(SPACING))
}

/// Tiles to pixels.
fn to_px(tiles: usize) -> Option<u32> {
    u32::try_from(tiles).ok()?.checked_mul(16)
}

/// `a - b` as a signed tile offset.
fn diff(a: usize, b: u16) -> Option<i32> {
    i32::try_from(a).ok()?.checked_sub(i32::from(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use smw_data::LevelId;

    fn rom() -> Option<(Rom, RomLayout)> {
        let Ok(path) = std::env::var("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping");
            return None;
        };
        let rom = Rom::load(std::path::Path::new(&path)).expect("rom loads");
        let layout = RomLayout::detect(&rom).expect("layout");
        Some((rom, layout))
    }

    #[test]
    fn slots_are_a_screen_apart() {
        assert_eq!(slot_position(0, false), Some(TilePos::new(4, 8)));
        assert_eq!(slot_position(2, false), Some(TilePos::new(68, 8)));
        assert_eq!(slot_position(1, true), Some(TilePos::new(8, 36)));
    }

    #[test]
    fn thumbnails_match_objects_placed_alone() {
        let Some((rom, layout)) = rom() else {
            return;
        };
        let id = LevelId::new(0x105).expect("id");
        let scene = LevelScene::load(&rom, &layout, id).expect("scene");
        // Batched and alone must agree: neighbours never leak into a picture.
        let requests = [(0x0F, 0x11), (0x21, 0x11), (0x00, 0x30), (0x12, 0x11), (0x10, 0x22)];
        let batched = object_thumbnails(&rom, &layout, &scene, &requests);
        for (request, thumb) in requests.iter().zip(&batched) {
            let alone = object_thumbnails(&rom, &layout, &scene, &[*request]);
            let same = alone.first() == Some(thumb);
            assert!(
                same,
                "object {request:02X?} alone: {:?}",
                alone.first().map(|a| a.as_ref().map(|t| (t.tiles, t.offset)))
            );
            assert!(thumb.is_some(), "object {request:02X?} drew nothing");
        }
        // A 1-tile extended object is exactly one tile at the placement.
        let block = batched.get(2).cloned().flatten().expect("? block");
        assert_eq!((block.tiles, block.offset), ((1, 1), (0, 0)));
        assert_eq!((block.raster.width(), block.raster.height()), (16, 16));
        // Slopes grow left of where they are placed.
        let slope = batched.get(3).cloned().flatten().expect("slope");
        assert!(slope.offset.0 < 0, "{:?}", slope.offset);
        // A pipe taller than the screen wraps into the next screens: rejected.
        let tall = object_thumbnails(&rom, &layout, &scene, &[(0x0F, 0x03)]);
        assert_eq!(tall, vec![None]);
    }

    #[test]
    fn slot_bands() {
        let pos = TilePos::new(36, 8);
        assert!(in_slot(TileRect { x: 32, y: 0, w: 32, h: 27 }, pos, false));
        assert!(!in_slot(TileRect { x: 31, y: 8, w: 2, h: 2 }, pos, false));
        assert!(!in_slot(TileRect { x: 36, y: 8, w: 29, h: 2 }, pos, false));
        assert!(in_slot(TileRect { x: 8, y: 36, w: 2, h: 28 }, TilePos::new(8, 36), true));
    }
}
