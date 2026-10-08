//! Per-tile object ownership: which object in the stream stamped each tile.
//!
//! The game's loader fetches every object's bytes through the bus before
//! running its routine, so a bus wrapper can note which object was fetched
//! last and tag every subsequent tile-RAM write with it. The last writer
//! wins, which matches what the editor should select when the user clicks a
//! tile. No breakpoints are needed, so Lunar Magic-patched loaders work too.

use smw_data::level::{Layer2, Level, PrimaryHeader, RawObject};
use w65c816::{Addr, Bus};

use crate::emu::{SnesBus, WRAM_LEN, wram_index};
use crate::level::{Grid, Layer};

/// Grid of owning object indices (`None` = no object wrote the tile).
pub type OwnerGrid = Grid<Option<u16>>;

/// Tag for "no object": the loader's own initialisation writes.
const NONE: u32 = u32::MAX;

/// A rectangle in tile coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TileRect {
    /// Left column.
    pub x: usize,
    /// Top row.
    pub y: usize,
    /// Width in tiles.
    pub w: usize,
    /// Height in tiles.
    pub h: usize,
}

impl TileRect {
    /// Whether `(x, y)` lies inside.
    #[must_use]
    pub const fn contains(self, x: usize, y: usize) -> bool {
        x >= self.x
            && y >= self.y
            && x < self.x.saturating_add(self.w)
            && y < self.y.saturating_add(self.h)
    }

    /// Whether the two rectangles overlap.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.x < other.x.saturating_add(other.w)
            && other.x < self.x.saturating_add(self.w)
            && self.y < other.y.saturating_add(other.h)
            && other.y < self.y.saturating_add(self.h)
    }
}

/// Ownership of both layers plus each object's visible bounding box.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ownership {
    /// Layer 1 owners.
    pub layer1: OwnerGrid,
    /// Layer 2 owners, when layer 2 holds objects.
    pub layer2: Option<OwnerGrid>,
    /// Bounding box of the tiles each layer 1 object still owns.
    pub bounds1: Vec<Option<TileRect>>,
    /// Same for layer 2 objects.
    pub bounds2: Vec<Option<TileRect>>,
}

impl Ownership {
    /// Builds the bounding boxes from the owner grids.
    #[must_use]
    pub fn new(layer1: OwnerGrid, layer2: Option<OwnerGrid>) -> Self {
        let bounds1 = bounds(&layer1);
        let bounds2 = layer2.as_ref().map(bounds).unwrap_or_default();
        Self { layer1, layer2, bounds1, bounds2 }
    }

    /// The owner grid of `layer`.
    #[must_use]
    pub const fn grid(&self, layer: Layer) -> Option<&OwnerGrid> {
        match layer {
            Layer::One => Some(&self.layer1),
            Layer::Two => self.layer2.as_ref(),
        }
    }

    /// Bounding boxes of `layer`'s objects.
    #[must_use]
    pub fn bounds(&self, layer: Layer) -> &[Option<TileRect>] {
        match layer {
            Layer::One => &self.bounds1,
            Layer::Two => &self.bounds2,
        }
    }

    /// The object under tile `(x, y)` of `layer`.
    #[must_use]
    pub fn owner_at(&self, layer: Layer, x: usize, y: usize) -> Option<u16> {
        self.grid(layer)?.get(x, y).flatten()
    }
}

/// Bounding box per object index from an owner grid.
fn bounds(grid: &OwnerGrid) -> Vec<Option<TileRect>> {
    let mut out: Vec<Option<TileRect>> = Vec::new();
    for y in 0..grid.height() {
        for x in 0..grid.width() {
            let Some(index) = grid.get(x, y).flatten() else {
                continue;
            };
            let index = usize::from(index);
            if out.len() <= index {
                out.resize(index.saturating_add(1), None);
            }
            let Some(slot) = out.get_mut(index) else {
                continue;
            };
            *slot = Some(slot.map_or(TileRect { x, y, w: 1, h: 1 }, |r| grow(r, x, y)));
        }
    }
    out
}

const fn grow(r: TileRect, x: usize, y: usize) -> TileRect {
    let x0 = if x < r.x { x } else { r.x };
    let y0 = if y < r.y { y } else { r.y };
    let x1 = if x.saturating_add(1) > r.x.saturating_add(r.w) {
        x.saturating_add(1)
    } else {
        r.x.saturating_add(r.w)
    };
    let y1 = if y.saturating_add(1) > r.y.saturating_add(r.h) {
        y.saturating_add(1)
    } else {
        r.y.saturating_add(r.h)
    };
    TileRect { x: x0, y: y0, w: x1.saturating_sub(x0), h: y1.saturating_sub(y0) }
}

/// Where one layer's object bytes sit on the bus, and the objects' offsets.
#[derive(Debug, Clone)]
struct DataRange {
    layer: Layer,
    /// Address of the first object byte (after the 5-byte header).
    start: Addr,
    /// `(offset, len)` per object, in stream order.
    objects: Vec<(usize, usize)>,
}

impl DataRange {
    fn from_objects(layer: Layer, data_start: Addr, objects: &[RawObject]) -> Self {
        let header = u32::try_from(PrimaryHeader::LEN).unwrap_or(5);
        Self {
            layer,
            start: data_start.wrapping_add(header),
            objects: objects.iter().map(|o| (o.offset, usize::from(o.len))).collect(),
        }
    }

    /// Index of the object whose bytes include `addr`.
    fn object_at(&self, addr: Addr) -> Option<u16> {
        let offset = usize::try_from(addr.checked_sub(self.start)?).ok()?;
        let after = self.objects.partition_point(|(start, _)| *start <= offset);
        let index = after.checked_sub(1)?;
        let (start, len) = *self.objects.get(index)?;
        (offset < start.saturating_add(len)).then(|| u16::try_from(index).ok()).flatten()
    }
}

/// A bus that attributes tile-RAM writes to the object last fetched.
pub struct AttributedBus<'a> {
    inner: SnesBus<'a>,
    ranges: Vec<DataRange>,
    current: u32,
    tags: Vec<u32>,
}

impl core::fmt::Debug for AttributedBus<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AttributedBus").field("current", &self.current).finish_non_exhaustive()
    }
}

impl<'a> AttributedBus<'a> {
    /// Wraps `inner`, watching `level`'s layer 1 bytes at `layer1_addr` and
    /// its layer 2 objects (if any) at `layer2_addr`.
    #[must_use]
    pub fn new(inner: SnesBus<'a>, level: &Level, layer1_addr: Addr, layer2_addr: Addr) -> Self {
        let mut ranges =
            vec![DataRange::from_objects(Layer::One, layer1_addr, &level.layer1.objects)];
        if let Layer2::Objects { stream, .. } = &level.layer2 {
            ranges.push(DataRange::from_objects(Layer::Two, layer2_addr, &stream.objects));
        }
        Self { inner, ranges, current: NONE, tags: vec![NONE; WRAM_LEN] }
    }

    /// The wrapped bus.
    #[must_use]
    pub const fn inner(&self) -> &SnesBus<'a> {
        &self.inner
    }

    /// The object of `layer` that last wrote the WRAM byte at `addr`.
    #[must_use]
    pub fn owner_at(&self, layer: Layer, addr: Addr) -> Option<u16> {
        let tag = *self.tags.get(wram_index(addr)?)?;
        if tag == NONE {
            return None;
        }
        let (tagged_layer, index) = decode_tag(tag);
        (tagged_layer == layer).then_some(index)
    }
}

fn encode_tag(layer: Layer, index: u16) -> u32 {
    let layer_bits = match layer {
        Layer::One => 0,
        Layer::Two => 1 << 16,
    };
    layer_bits | u32::from(index)
}

#[allow(clippy::as_conversions, clippy::cast_possible_truncation, reason = "masked to 16 bits")]
const fn decode_tag(tag: u32) -> (Layer, u16) {
    let layer = if tag & (1 << 16) == 0 { Layer::One } else { Layer::Two };
    (layer, (tag & 0xFFFF) as u16)
}

impl Bus for AttributedBus<'_> {
    fn read(&mut self, addr: Addr) -> u8 {
        if let Some(tag) =
            self.ranges.iter().find_map(|r| r.object_at(addr).map(|i| encode_tag(r.layer, i)))
        {
            self.current = tag;
        }
        self.inner.read(addr)
    }

    fn write(&mut self, addr: Addr, value: u8) {
        if let Some(slot) = wram_index(addr).and_then(|i| self.tags.get_mut(i)) {
            *slot = self.current;
        }
        self.inner.write(addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::{DataRange, TileRect, decode_tag, encode_tag};
    use crate::level::Layer;

    #[test]
    fn object_lookup_by_address() {
        let range = DataRange {
            layer: Layer::One,
            start: 0x60_8005,
            objects: vec![(0, 3), (3, 4), (7, 3)],
        };
        assert_eq!(range.object_at(0x60_8004), None, "header byte");
        assert_eq!(range.object_at(0x60_8005), Some(0));
        assert_eq!(range.object_at(0x60_8007), Some(0));
        assert_eq!(range.object_at(0x60_8008), Some(1));
        assert_eq!(range.object_at(0x60_800B), Some(1));
        assert_eq!(range.object_at(0x60_800E), Some(2));
        assert_eq!(range.object_at(0x60_800F), None, "terminator");
        assert_eq!(decode_tag(encode_tag(Layer::Two, 0x1234)), (Layer::Two, 0x1234));
    }

    #[test]
    fn rect_geometry() {
        let r = TileRect { x: 2, y: 3, w: 4, h: 2 };
        assert!(r.contains(2, 3));
        assert!(r.contains(5, 4));
        assert!(!r.contains(6, 4));
        assert!(r.intersects(TileRect { x: 5, y: 4, w: 10, h: 10 }));
        assert!(!r.intersects(TileRect { x: 6, y: 4, w: 10, h: 10 }));
        assert_eq!(super::grow(r, 0, 10), TileRect { x: 0, y: 3, w: 6, h: 8 });
    }

    /// Every stamped tile of Yoshi's Island 2 has an owner, and the owners
    /// agree with the "load one more object and diff" oracle.
    #[test]
    fn real_rom_ownership_matches_truncation_oracle() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = smw_data::RomLayout::detect(&rom).expect("vanilla");
        let mut ids = vec![smw_data::LevelId::new(0x105).expect("valid")];
        // Plus the first vertical level with a decent number of objects.
        let vertical = smw_data::LevelId::all().find(|id| {
            smw_data::level::Level::load(&rom, &layout, *id)
                .is_ok_and(|l| l.vertical && l.layer1.objects.len() > 10)
        });
        ids.extend(vertical);
        for id in ids {
            check_oracle(&rom, &layout, id);
        }
    }

    fn check_oracle(rom: &snes_rom::Rom, layout: &smw_data::RomLayout, id: smw_data::LevelId) {
        let level = smw_data::level::Level::load(rom, layout, id).expect("parses");
        let full = crate::level::load_level_data(rom, layout, level.clone()).expect("loads");
        let owners = &full.ownership.layer1;
        let blank = 0x25;
        let mut stamped = 0usize;
        for y in 0..full.layer1.height() {
            for x in 0..full.layer1.width() {
                let tile = full.layer1.get(x, y).unwrap();
                let who = owners.get(x, y).flatten();
                if tile != blank {
                    assert!(who.is_some(), "level {id}: tile ({x},{y}) = {tile:03X} has no who");
                    stamped = stamped.saturating_add(1);
                }
            }
        }
        assert!(stamped > 100, "level {id}: only {stamped} owned tiles");

        let mut previous = {
            let mut empty = level.clone();
            empty.layer1.objects.clear();
            crate::level::load_level_data(rom, layout, empty).expect("loads").layer1
        };
        for count in 1..=level.layer1.objects.len() {
            let mut partial = level.clone();
            partial.layer1.objects.truncate(count);
            let grid = crate::level::load_level_data(rom, layout, partial).expect("loads").layer1;
            let index = u16::try_from(count.saturating_sub(1)).unwrap();
            for y in 0..grid.height() {
                for x in 0..grid.width() {
                    let who = owners.get(x, y).flatten();
                    if grid.get(x, y) != previous.get(x, y) {
                        assert!(
                            who.is_some_and(|o| o >= index),
                            "level {id}: object {index} changed ({x},{y}) but who is {who:?}"
                        );
                    }
                    if who == Some(index) {
                        assert_eq!(
                            grid.get(x, y),
                            full.layer1.get(x, y),
                            "level {id}: object {index} owns ({x},{y}) but a later object changed it"
                        );
                    }
                }
            }
            previous = grid;
        }
    }
}
