//! Lunar Magic's foreground Map16 pages 2 and up, resolved by running the
//! ROM's own getter routine. See `docs/formats/map16.md` ("Lunar Magic").

use std::collections::HashMap;

use smw_data::RomLayout;
use smw_data::map16::Map16Tile;
use snes_rom::rats::{RatsTag, TAG_LEN};
use snes_rom::{Rom, SnesAddr};
use w65c816::Cpu;

use crate::emu::SnesBus;
use crate::level::LoadError;

/// First tile the getter handles; pages 0-1 come from the vanilla tables.
pub const FIRST_EXT_TILE: u16 = 0x200;
/// One past the last tile Lunar Magic 2.5x can address.
pub const TILE_LIMIT: u16 = 0x8000;
/// Address the emulated `JSL` returns to.
const SENTINEL: u32 = 0xFF_FFFF;
/// Instruction budget per call; the routine is about 15 instructions.
const STEP_BUDGET: u64 = 10_000;
/// Direct-page byte the routine stores the bank of its result in.
const RESULT_BANK_ADDR: u32 = 0x0C;
/// Bytes per Map16 tile.
const TILE_BYTES: u32 = 8;

/// File-offset range `[start, end)` of one page group's data block, or
/// `None` when the group is unallocated.
type BlockBounds = Option<(usize, usize)>;

/// Resolves foreground tiles 0x200+ through the ROM's getter, caching the
/// result and bounding reads by the RATS block each page group lives in.
pub struct FgPageResolver<'a> {
    rom: &'a Rom,
    getter: SnesAddr,
    bus: SnesBus<'a>,
    cpu: Cpu,
    tiles: HashMap<u16, Option<Map16Tile>>,
    blocks: HashMap<u16, BlockBounds>,
}

impl core::fmt::Debug for FgPageResolver<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FgPageResolver")
            .field("getter", &self.getter)
            .field("cached", &self.tiles.len())
            .finish_non_exhaustive()
    }
}

impl<'a> FgPageResolver<'a> {
    /// A resolver for `rom`, or `None` for a vanilla layout.
    #[must_use]
    pub fn new(rom: &'a Rom, layout: &RomLayout) -> Option<Self> {
        let getter = layout.lm.as_ref()?.fg_map16_getter;
        let mut cpu = Cpu::new();
        cpu.brk_is_fatal = true;
        Some(Self {
            rom,
            getter,
            bus: SnesBus::new(rom),
            cpu,
            tiles: HashMap::new(),
            blocks: HashMap::new(),
        })
    }

    /// Tile `n`, or `None` if `n` is below 0x200, beyond the addressable
    /// range, or in a page group the ROM never allocated.
    ///
    /// # Errors
    ///
    /// Emulation or ROM read failures.
    pub fn tile(&mut self, n: u16) -> Result<Option<Map16Tile>, LoadError> {
        if !(FIRST_EXT_TILE..TILE_LIMIT).contains(&n) {
            return Ok(None);
        }
        if let Some(cached) = self.tiles.get(&n) {
            return Ok(*cached);
        }
        let tile = self.lookup(n)?;
        self.tiles.insert(n, tile);
        Ok(tile)
    }

    /// Every tile of `page` (256 entries, `None` where unallocated).
    ///
    /// # Errors
    ///
    /// See [`FgPageResolver::tile`].
    pub fn page(&mut self, page: u8) -> Result<Vec<Option<Map16Tile>>, LoadError> {
        let first = u16::from(page) << 8;
        (0..=0xFFu16).map(|i| self.tile(first | i)).collect()
    }

    fn lookup(&mut self, n: u16) -> Result<Option<Map16Tile>, LoadError> {
        let Some(addr) = self.call_getter(n)? else {
            return Ok(None);
        };
        let pc = self.rom.pc(addr)?;
        let Some((start, end)) = self.block_bounds(n)? else {
            return Ok(None);
        };
        let tile_end = pc.saturating_add(8);
        if pc < start || tile_end > end {
            return Ok(None);
        }
        let bytes = self.rom.bytes().get(pc..tile_end).ok_or(snes_rom::RomError::Unmapped(addr))?;
        Ok(Map16Tile::from_bytes(bytes))
    }

    /// Runs the getter with A = `n * 2`; returns the 24-bit address it
    /// produced, or `None` when the bank byte is 0 (page group unallocated).
    fn call_getter(&mut self, n: u16) -> Result<Option<SnesAddr>, LoadError> {
        self.cpu.a = n.wrapping_mul(2);
        self.cpu.p.m = false;
        self.cpu.p.x = false;
        self.cpu.s = 0x01FF;
        self.cpu.db = 0;
        self.cpu.d = 0;
        self.cpu.call_long(&mut self.bus, self.getter.get(), SENTINEL, STEP_BUDGET)?;
        let bank = self.bus.wram_at(RESULT_BANK_ADDR).unwrap_or(0);
        if bank == 0 {
            return Ok(None);
        }
        Ok(Some(SnesAddr::from_parts(bank, self.cpu.a)))
    }

    /// File-offset bounds of the block holding tile `n`'s page group: the
    /// RATS block that starts at the group's first tile, or the ROM's end
    /// when the data is untagged.
    fn block_bounds(&mut self, n: u16) -> Result<BlockBounds, LoadError> {
        let first = group_first_tile(n);
        if let Some(bounds) = self.blocks.get(&first) {
            return Ok(*bounds);
        }
        let bounds = match self.call_getter(first)? {
            None => None,
            Some(addr) => {
                let pc = self.rom.pc(addr)?;
                let end = pc
                    .checked_sub(TAG_LEN)
                    .and_then(|tag_pc| RatsTag::parse_at(self.rom.bytes(), tag_pc))
                    .map_or(self.rom.len(), |tag| tag.end_pc());
                Some((pc, end))
            }
        };
        self.blocks.insert(first, bounds);
        Ok(bounds)
    }
}

/// The first tile of the page group the getter places `n` in: tiles
/// 0x200-0xFFF share one group, then every 0x1000 tiles form a group.
const fn group_first_tile(n: u16) -> u16 {
    if n < 0x1000 { FIRST_EXT_TILE } else { n & 0xF000 }
}

/// Unused but documents the record size the bounds check assumes.
const _: () = assert!(TILE_BYTES == 8);

#[cfg(test)]
mod tests {
    use super::{FgPageResolver, group_first_tile};

    #[test]
    fn page_groups() {
        assert_eq!(group_first_tile(0x200), 0x200);
        assert_eq!(group_first_tile(0xFFF), 0x200);
        assert_eq!(group_first_tile(0x1234), 0x1000);
        assert_eq!(group_first_tile(0x7FFF), 0x7000);
    }

    /// Both Quickie World ROMs allocate page 2 (verified 2026-10-06).
    #[test]
    fn lm_rom_resolves_page_2() {
        let Some(path) = std::env::var_os("LM_ROM") else {
            eprintln!("LM_ROM not set; skipping Lunar Magic ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = smw_data::RomLayout::detect(&rom).expect("lunar magic");
        let mut resolver = FgPageResolver::new(&rom, &layout).expect("lunar magic layout");
        let tile = resolver.tile(0x200).expect("runs").expect("allocated");
        assert_eq!(resolver.tile(0x200).expect("runs"), Some(tile), "cached");
        assert!(resolver.tile(0x1FF).expect("runs").is_none(), "pages 0-1 are vanilla");
        assert!(resolver.tile(0x7FFF).expect("runs").is_none(), "unallocated group");
        let page = resolver.page(2).expect("runs");
        assert!(page.iter().filter(|t| t.is_some()).count() > 200);
        assert!(FgPageResolver::new(&rom, &smw_data::layout::VANILLA).is_none());
    }
}
