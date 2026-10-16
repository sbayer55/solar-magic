//! A catalogue of every table a [`RomLayout`] names, so tools can label an
//! address (`layer1_ptrs[0x105]`) instead of printing a bare number.

use snes_rom::{Rom, SnesAddr};

use super::RomLayout;
use crate::gfx_bypass;
use crate::map16::{TILES_PER_SET, TILESET_COUNT};
use crate::tileset::SPRITE_SET_COUNT;

/// Bytes per Map16 tile record.
const TILE_LEN: usize = 8;
/// Tiles in the common Map16 table (`docs/formats/map16.md`).
const COMMON_TILES: usize = 366;
/// Tileset-specific tiles per tileset.
const TILESET_TILES: usize = 178;
/// Bytes of a packed 24-bit pointer.
const PTR_LEN: usize = 3;
/// Entries in the per-level tables.
const LEVELS: usize = 0x200;

/// One named byte range of the ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownRange {
    /// What the range holds (a layout field name, or a description).
    pub name: String,
    /// First byte.
    pub addr: SnesAddr,
    /// Length in bytes (1 for a code entry point).
    pub len: usize,
    /// Entry size when the range is a table, so an address can be labelled
    /// with its index.
    pub stride: Option<usize>,
}

impl KnownRange {
    fn new(name: impl Into<String>, addr: SnesAddr, len: usize, stride: Option<usize>) -> Self {
        Self { name: name.into(), addr, len, stride }
    }

    /// Whether `addr` lies inside the range.
    #[must_use]
    pub fn contains(&self, addr: SnesAddr) -> bool {
        self.offset_of(addr).is_some()
    }

    /// Byte offset of `addr` from the start, when inside.
    #[must_use]
    pub fn offset_of(&self, addr: SnesAddr) -> Option<usize> {
        let offset = usize::try_from(addr.get().checked_sub(self.addr.get())?).ok()?;
        (offset < self.len).then_some(offset)
    }

    /// `name`, or `name[index]` for tables.
    #[must_use]
    pub fn label(&self, addr: SnesAddr) -> String {
        match (self.offset_of(addr), self.stride) {
            (Some(offset), Some(stride)) if stride > 0 => {
                format!("{}[{:#X}]", self.name, offset.checked_div(stride).unwrap_or(0))
            }
            _ => self.name.clone(),
        }
    }
}

/// The label of the first known range containing `addr`.
#[must_use]
pub fn label_address(ranges: &[KnownRange], addr: SnesAddr) -> Option<String> {
    ranges.iter().find(|r| r.contains(addr)).map(|r| r.label(addr))
}

impl RomLayout {
    /// Every table the layout names, including the ones only reachable by
    /// reading a pointer out of `rom` (tileset Map16 blocks, Lunar Magic's
    /// relocated tables).
    #[must_use]
    pub fn known_ranges(&self, rom: &Rom) -> Vec<KnownRange> {
        let mut out = self.known_ranges_static();
        out.extend(self.tileset_tile_ranges(rom));
        out.extend(self.lunar_magic_pointed_ranges(rom));
        out
    }

    /// The tables whose addresses the layout holds directly.
    #[must_use]
    pub fn known_ranges_static(&self) -> Vec<KnownRange> {
        let r = KnownRange::new;
        let mut out = vec![
            r("layer1_ptrs", self.layer1_ptrs, LEVELS.saturating_mul(PTR_LEN), Some(PTR_LEN)),
            r("layer2_ptrs", self.layer2_ptrs, LEVELS.saturating_mul(PTR_LEN), Some(PTR_LEN)),
            r("sprite_ptrs", self.sprite_ptrs, LEVELS.saturating_mul(2), Some(2)),
        ];
        if let Some(table) = self.sprite_banks {
            out.push(r("sprite_banks", table, LEVELS, Some(1)));
        }
        for (i, table) in self.secondary_header.iter().enumerate() {
            out.push(KnownRange::new(format!("secondary_header[{i}]"), *table, LEVELS, Some(1)));
        }
        for (i, table) in self.secondary_exits.iter().enumerate() {
            out.push(KnownRange::new(format!("secondary_exits[{i}]"), *table, LEVELS, Some(1)));
        }
        let gfx = self.gfx_ptrs;
        out.push(r("gfx_ptrs.low", gfx.low, gfx.count, Some(1)));
        out.push(r("gfx_ptrs.high", gfx.high, gfx.count, Some(1)));
        out.push(r("gfx_ptrs.bank", gfx.bank, gfx.count, Some(1)));
        out.extend(self.palette_ranges());
        out.extend(self.map16_ranges());
        out.push(r(
            "layer_gfx_list",
            self.layer_gfx_list,
            usize::from(TILESET_COUNT).saturating_mul(4),
            Some(4),
        ));
        out.push(r(
            "sprite_gfx_list",
            self.sprite_gfx_list,
            usize::from(SPRITE_SET_COUNT).saturating_mul(4),
            Some(4),
        ));
        out.push(r("level_mode_table", self.level_mode_table, 0x20, Some(1)));
        out.push(r("loader.entry (code)", self.loader.entry, 1, None));
        out.push(r("loader.tile_ram_ptrs_low", self.loader.tile_ram_ptrs_low, 4, Some(2)));
        out.push(r("loader.tile_ram_ptrs_high", self.loader.tile_ram_ptrs_high, 4, Some(2)));
        out.push(r("entrance.y_low", self.entrance.y_low, 16, Some(1)));
        out.push(r("entrance.y_high", self.entrance.y_high, 16, Some(1)));
        out.push(r("entrance.x_low", self.entrance.x_low, 8, Some(1)));
        out.push(r("entrance.x_high", self.entrance.x_high, 8, Some(1)));
        out.extend(self.level_name_ranges());
        out.extend(self.lunar_magic_static_ranges());
        out
    }

    fn level_name_ranges(&self) -> Vec<KnownRange> {
        let n = self.level_names;
        let r = KnownRange::new;
        let span = |from: SnesAddr, to: SnesAddr| {
            usize::try_from(to.get().saturating_sub(from.get())).unwrap_or(0)
        };
        vec![
            r("level_names.names", n.names, n.count.saturating_mul(2), Some(2)),
            r("level_names.strings", n.strings, span(n.strings, n.first_words), None),
            r(
                "level_names.first_words",
                n.first_words,
                span(n.first_words, n.second_words),
                Some(2),
            ),
            r(
                "level_names.second_words",
                n.second_words,
                span(n.second_words, n.third_words),
                Some(2),
            ),
            r("level_names.third_words", n.third_words, span(n.third_words, n.tables_end), Some(2)),
        ]
    }

    fn palette_ranges(&self) -> Vec<KnownRange> {
        let p = self.palettes;
        let r = KnownRange::new;
        vec![
            r("palettes.back_area_colors", p.back_area_colors, 16, Some(2)),
            r("palettes.background", p.background, 192, Some(24)),
            r("palettes.status_bar", p.status_bar, 32, Some(2)),
            r("palettes.foreground", p.foreground, 192, Some(24)),
            r("palettes.standard", p.standard, 120, Some(2)),
            r("palettes.player", p.player, 80, Some(20)),
            r("palettes.sprite", p.sprite, 192, Some(24)),
            r("palettes.berry", p.berry, 42, Some(2)),
        ]
    }

    fn map16_ranges(&self) -> Vec<KnownRange> {
        let m = self.map16;
        let r = KnownRange::new;
        vec![
            r("map16.common", m.common, COMMON_TILES.saturating_mul(TILE_LEN), Some(TILE_LEN)),
            r("map16.bg_tiles", m.bg_tiles, TILES_PER_SET.saturating_mul(TILE_LEN), Some(TILE_LEN)),
            r(
                "map16.tileset_locations",
                m.tileset_locations,
                usize::from(TILESET_COUNT).saturating_mul(2),
                Some(2),
            ),
            r("map16.tileset_bitmask", m.tileset_bitmask, TILES_PER_SET / 8, Some(1)),
            // `diagonal_pipes` and the animation frames lie inside `common`.
            r("map16.appearance_table", m.appearance_table, 8, Some(2)),
        ]
    }

    /// One range per distinct tileset-specific Map16 block.
    fn tileset_tile_ranges(&self, rom: &Rom) -> Vec<KnownRange> {
        let m = self.map16;
        let mut blocks: Vec<(u16, Vec<u8>)> = Vec::new();
        for tileset in 0..TILESET_COUNT {
            let Some(entry) = m.tileset_locations.checked_add(u32::from(tileset).saturating_mul(2))
            else {
                continue;
            };
            let Ok(offset) = rom.read_u16_le(entry) else {
                continue;
            };
            match blocks.iter_mut().find(|(o, _)| *o == offset) {
                Some((_, users)) => users.push(tileset),
                None => blocks.push((offset, vec![tileset])),
            }
        }
        blocks
            .into_iter()
            .map(|(offset, users)| {
                let names: Vec<String> = users.iter().map(ToString::to_string).collect();
                KnownRange::new(
                    format!("map16 tileset tiles (tilesets {})", names.join(",")),
                    SnesAddr::from_parts(m.tileset_bank, offset),
                    TILESET_TILES.saturating_mul(TILE_LEN),
                    Some(TILE_LEN),
                )
            })
            .collect()
    }

    fn lunar_magic_static_ranges(&self) -> Vec<KnownRange> {
        let Some(lm) = self.lm else {
            return Vec::new();
        };
        let r = KnownRange::new;
        let mut out = vec![
            r("lm.signature", lm.signature, 64, None),
            r("lm.level_flags", lm.level_flags, LEVELS, Some(1)),
            r(
                "lm.custom_palettes",
                lm.custom_palettes,
                LEVELS.saturating_mul(PTR_LEN),
                Some(PTR_LEN),
            ),
            r("lm.exgfx_ptrs", lm.exgfx_ptrs, 0x80usize.saturating_mul(PTR_LEN), Some(PTR_LEN)),
            r("lm.exgfx_high_table_ptr (operand)", lm.exgfx_high_table_ptr, PTR_LEN, None),
            r("lm.gfx_bypass_table_ptr (operand)", lm.gfx_bypass_table_ptr, PTR_LEN, None),
            r("lm.bg_page_sets", lm.bg_page_sets, 16usize.saturating_mul(PTR_LEN), Some(PTR_LEN)),
            r("lm.fg_map16_getter (code)", lm.fg_map16_getter, 1, None),
        ];
        if self.sprite_banks.is_none() {
            out.push(r("lm.sprite_banks", lm.sprite_banks, LEVELS, Some(1)));
        }
        for (i, table) in lm.secondary_header_ext.iter().enumerate() {
            out.push(KnownRange::new(
                format!("lm.secondary_header_ext[{i}]"),
                *table,
                LEVELS,
                Some(1),
            ));
        }
        out
    }

    /// Lunar Magic tables found by following a pointer operand.
    fn lunar_magic_pointed_ranges(&self, rom: &Rom) -> Vec<KnownRange> {
        let Some(lm) = self.lm else {
            return Vec::new();
        };
        let follow = |operand: SnesAddr| {
            rom.read_ptr(operand).ok().filter(|p| p.get() != 0 && p.get() != 0xFF_FFFF)
        };
        let mut out = Vec::new();
        if let Some(table) = follow(lm.exgfx_high_table_ptr) {
            out.push(KnownRange::new(
                "lm ExGFX 100-FFF pointers",
                table,
                0xF00usize.saturating_mul(PTR_LEN),
                Some(PTR_LEN),
            ));
        }
        if let Some(table) = follow(lm.gfx_bypass_table_ptr) {
            out.push(KnownRange::new(
                "lm GFX bypass rows",
                table,
                LEVELS.saturating_mul(gfx_bypass::ROW_LEN),
                Some(gfx_bypass::ROW_LEN),
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use snes_rom::SnesAddr;

    use super::label_address;
    use crate::layout::VANILLA;

    #[test]
    fn labels_table_entries() {
        let ranges = VANILLA.known_ranges_static();
        let l1 = VANILLA.layer1_ptrs.checked_add(0x105 * 3 + 1).unwrap();
        assert_eq!(label_address(&ranges, l1).as_deref(), Some("layer1_ptrs[0x105]"));
        let sh = VANILLA.secondary_header[2].checked_add(0x1FF).unwrap();
        assert_eq!(label_address(&ranges, sh).as_deref(), Some("secondary_header[2][0x1FF]"));
        assert_eq!(label_address(&ranges, SnesAddr::lit(0x10_8000)), None);
        assert_eq!(
            label_address(&ranges, VANILLA.loader.entry).as_deref(),
            Some("loader.entry (code)")
        );
    }

    #[test]
    fn static_ranges_do_not_overlap() {
        let ranges = VANILLA.known_ranges_static();
        for (i, a) in ranges.iter().enumerate() {
            for b in ranges.iter().skip(i + 1) {
                let a_end = a.addr.get() + u32::try_from(a.len).unwrap();
                let b_end = b.addr.get() + u32::try_from(b.len).unwrap();
                assert!(
                    a_end <= b.addr.get() || b_end <= a.addr.get(),
                    "{} overlaps {}",
                    a.name,
                    b.name
                );
            }
        }
    }
}
