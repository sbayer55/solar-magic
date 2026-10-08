//! Writing a level back into a ROM in "vanilla-safe" mode: the game's own
//! pointer tables are updated, relocated data is RATS-tagged in expanded
//! space, and no Lunar Magic hijack is required to read the result.
//! See `docs/formats/rats.md` and `docs/formats/level-header.md`.

use std::collections::BTreeMap;

use snes_rom::rats::{self, AllocRequest, RatsError, RatsTag};
use snes_rom::{Rom, RomError, SnesAddr};

use super::layer2::SECOND_PAGE_OFFSET;
use super::{
    LAYER2_BACKGROUND_BANK, LAYER2_BACKGROUND_REAL_BANK, Layer2, Level, LevelAddresses,
    ObjectStream, PrimaryHeader, SecondaryHeader, SpriteList, decode_bg_rle,
};
use crate::{LevelId, RomLayout};

/// Options for [`save_level`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveOptions {
    /// Size the ROM is grown to when relocated data needs expanded space.
    pub expand_to: usize,
    /// Relocate layer data even when it is unchanged (used by tests and
    /// `solar relocate-level`).
    pub force_relocate: bool,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self { expand_to: 0x10_0000, force_relocate: false }
    }
}

/// Where one part of the level ended up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    variant_size_differences,
    reason = "a tag is three words; the enum is never stored in bulk"
)]
pub enum Placement {
    /// The bytes in ROM already matched; nothing was written.
    Unchanged,
    /// Overwritten at its existing address.
    InPlace(SnesAddr),
    /// Another level already stores identical bytes; the pointer now shares
    /// that block, as many vanilla levels do.
    Shared(SnesAddr),
    /// Written to a new RATS-tagged block and the pointer table updated.
    Relocated {
        /// The new block's tag.
        tag: RatsTag,
        /// The address the pointer table now holds.
        addr: SnesAddr,
    },
}

impl Placement {
    /// The address the level's pointer refers to after the save, if changed.
    #[must_use]
    pub const fn new_addr(self) -> Option<SnesAddr> {
        match self {
            Self::Unchanged => None,
            Self::InPlace(addr) | Self::Shared(addr) | Self::Relocated { addr, .. } => Some(addr),
        }
    }
}

/// What [`save_level`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveReport {
    /// Layer 1 data.
    pub layer1: Placement,
    /// Layer 2 data (objects or background tilemap).
    pub layer2: Placement,
    /// Sprite data.
    pub sprites: Placement,
    /// Whether any secondary header byte changed.
    pub secondary_header: bool,
    /// Whether the image was expanded.
    pub expanded: bool,
    /// Old blocks released.
    pub freed: Vec<RatsTag>,
    /// The checksum written.
    pub checksum: u16,
}

/// Errors from saving.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SaveError {
    /// Data does not fit the free space it is confined to (bank `$FF` means
    /// expanded space).
    #[error("{part}: {len} bytes do not fit the free space of bank ${bank:02X}")]
    NoRoomInBank {
        /// Which part.
        part: &'static str,
        /// The bank.
        bank: u8,
        /// Bytes needed.
        len: usize,
    },
    /// Sprite data must live in the bank the game hardcodes.
    #[error("sprite data at {0} is not in bank ${1:02X}")]
    SpriteBank(SnesAddr, u8),
    /// Saving into a Lunar Magic ROM needs its relocated tables and hijacks,
    /// which are not written yet.
    #[error("saving into Lunar Magic ROMs is not supported yet")]
    LunarMagicRom,
    /// Allocation failed.
    #[error(transparent)]
    Rats(#[from] RatsError),
    /// ROM access failed.
    #[error(transparent)]
    Rom(#[from] RomError),
}

/// One of the three independently stored parts of a level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Part {
    name: &'static str,
    kind: Kind,
    slot: Slot,
}

const LAYER1: Part = Part { name: "layer 1", kind: Kind::Layer1, slot: Slot::Layer1 };
const LAYER2_OBJECTS: Part =
    Part { name: "layer 2", kind: Kind::Layer2Objects, slot: Slot::Layer2 };
const BACKGROUND: Part = Part { name: "background", kind: Kind::Background, slot: Slot::Layer2 };
const SPRITES: Part = Part { name: "sprites", kind: Kind::Sprites, slot: Slot::Sprites };

/// Where bank-confined data may go: a bank and, for backgrounds, the half
/// of it that matches the tilemap's page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BankTarget {
    bank: u8,
    page: Option<u8>,
}

/// Which pointer table entry a part owns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Layer1,
    Layer2,
    Sprites,
}

/// How to measure the data already stored at an address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Layer1,
    Layer2Objects,
    Background,
    Sprites,
}

/// Levels referencing each pointer, for all three tables. Layer 2 is keyed
/// by the pointer exactly as stored (bank `$FF` for backgrounds).
///
/// # Errors
///
/// ROM read failures.
pub fn pointer_refs(
    rom: &Rom,
    layout: &RomLayout,
) -> Result<BTreeMap<SnesAddr, Vec<LevelId>>, RomError> {
    let mut refs: BTreeMap<SnesAddr, Vec<LevelId>> = BTreeMap::new();
    for id in LevelId::all() {
        let index = u32::from(id.get());
        let layer1 = rom.read_ptr(offset(layout.layer1_ptrs, index.saturating_mul(3))?)?;
        let layer2 = rom.read_ptr(offset(layout.layer2_ptrs, index.saturating_mul(3))?)?;
        let sprite_offset =
            rom.read_u16_le(offset(layout.sprite_ptrs, index.saturating_mul(2))?)?;
        let bank = match layout.sprite_banks {
            Some(table) => rom.read_u8(offset(table, index)?)?,
            None => layout.sprite_bank,
        };
        for addr in [layer1, layer2, SnesAddr::from_parts(bank, sprite_offset)] {
            refs.entry(addr).or_default().push(id);
        }
    }
    Ok(refs)
}

/// Writes `level` into `rom`, replacing whatever level `level.id` held.
///
/// The slot's current pointers (not `level.addresses`, which may describe
/// where the data was loaded from) decide what is overwritten, so a level
/// loaded from one slot can be saved into another. The work happens on a
/// copy that replaces `rom` only on success, so a failure leaves the ROM
/// untouched.
///
/// # Errors
///
/// See [`SaveError`].
pub fn save_level(
    rom: &mut Rom,
    layout: &RomLayout,
    level: &Level,
    opts: &SaveOptions,
) -> Result<SaveReport, SaveError> {
    if layout.is_lunar_magic() {
        return Err(SaveError::LunarMagicRom);
    }
    let mut work = rom.clone();
    let report = Saver::new(&mut work, layout, level, opts)?.run()?;
    *rom = work;
    Ok(report)
}

struct Saver<'a> {
    rom: &'a mut Rom,
    layout: &'a RomLayout,
    level: &'a Level,
    opts: &'a SaveOptions,
    /// Where the slot's data currently lives.
    current: LevelAddresses,
    refs: BTreeMap<SnesAddr, Vec<LevelId>>,
    freed: Vec<RatsTag>,
    expanded: bool,
}

impl<'a> Saver<'a> {
    fn new(
        rom: &'a mut Rom,
        layout: &'a RomLayout,
        level: &'a Level,
        opts: &'a SaveOptions,
    ) -> Result<Self, SaveError> {
        let refs = pointer_refs(rom, layout)?;
        let current = LevelAddresses::load(rom, layout, level.id)?;
        Ok(Self { rom, layout, level, opts, current, refs, freed: Vec::new(), expanded: false })
    }

    fn run(mut self) -> Result<SaveReport, SaveError> {
        let level = self.level;
        let current = self.current;
        let layer1 = self.place_relocatable(LAYER1, current.layer1, &level.layer1_bytes())?;
        let layer2 = match &level.layer2 {
            Layer2::Objects { .. } if current.layer2_raw.bank() != LAYER2_BACKGROUND_BANK => {
                self.place_relocatable(LAYER2_OBJECTS, current.layer2_raw, &level.layer2_bytes())?
            }
            // The slot currently holds a background (or vice versa): the
            // kinds differ, so the data cannot be compared or reused.
            Layer2::Objects { .. } => {
                let bytes = level.layer2_bytes();
                if let Some(shared) = self.identical_block(&bytes, None, None) {
                    self.write_pointer(Slot::Layer2, shared)?;
                    Placement::Shared(shared)
                } else {
                    self.relocate_fresh(&bytes, Slot::Layer2)?
                }
            }
            Layer2::Background { tiles, .. } => {
                let page = tiles.first().map_or(0, |t| t.to_le_bytes()[1] & 1);
                self.place_background(page)?
            }
        };
        let sprites = self.place_sprites()?;
        let secondary_header = self.write_secondary_header()?;
        let checksum = self.rom.fix_checksum();
        Ok(SaveReport {
            layer1,
            layer2,
            sprites,
            secondary_header,
            expanded: self.expanded,
            freed: self.freed,
            checksum,
        })
    }

    /// Whether only this level points at `addr`.
    fn unshared(&self, addr: SnesAddr) -> bool {
        self.refs.get(&addr).is_none_or(|ids| ids.iter().all(|id| *id == self.level.id))
    }

    /// Whether the bytes at `addr` already equal `bytes`.
    fn unchanged(&self, addr: SnesAddr, bytes: &[u8]) -> bool {
        self.rom.read(addr, bytes.len()).is_ok_and(|current| current == bytes)
    }

    /// An address some level already points at whose bytes equal `bytes`,
    /// restricted to `bank` (and background `page`) when given.
    fn identical_block(
        &self,
        bytes: &[u8],
        bank: Option<u8>,
        page: Option<u8>,
    ) -> Option<SnesAddr> {
        self.refs
            .keys()
            .map(|addr| real_address(*addr))
            .filter(|addr| bank.is_none_or(|b| addr.bank() == b))
            .filter(|addr| {
                page.is_none_or(|p| self.rom.pc(*addr).is_ok_and(|pc| page_matches(pc, p)))
            })
            .find(|addr| self.unchanged(*addr, bytes))
    }

    /// Capacity of the block at `addr`: its tag's size, or the length of the
    /// data currently parsed there.
    fn capacity(&self, addr: SnesAddr, kind: Kind) -> Option<usize> {
        let pc = self.rom.pc(addr).ok()?;
        if let Some(tag) = rats::tag_before(self.rom.bytes(), pc) {
            return Some(tag.data_len);
        }
        let data = self.rom.bytes().get(pc..)?;
        match kind {
            Kind::Layer1 | Kind::Layer2Objects => {
                ObjectStream::parse(data.get(PrimaryHeader::LEN..)?, self.level.format)
                    .map(|s| s.encoded_len().saturating_add(PrimaryHeader::LEN))
            }
            Kind::Background => decode_bg_rle(data).map(|(_, compressed)| compressed.len()),
            Kind::Sprites => SpriteList::parse(data).map(|s| s.to_bytes().len()),
        }
    }

    /// Layer 1 and layer 2 objects: rewritten inside their own tagged block
    /// when it fits, otherwise relocated to a new block in expanded space.
    fn place_relocatable(
        &mut self,
        part: Part,
        addr: SnesAddr,
        bytes: &[u8],
    ) -> Result<Placement, SaveError> {
        let Part { name, kind, slot } = part;
        if !self.opts.force_relocate && self.unchanged(addr, bytes) {
            return Ok(Placement::Unchanged);
        }
        let pc = self.rom.pc(addr)?;
        let old_tag = rats::tag_before(self.rom.bytes(), pc);
        let unshared = self.unshared(addr);
        if !self.opts.force_relocate
            && unshared
            && old_tag.is_some()
            && self.capacity(addr, kind).is_some_and(|cap| cap >= bytes.len())
        {
            self.rom.write_pc(pc, bytes)?;
            return Ok(Placement::InPlace(addr));
        }
        if !self.opts.force_relocate
            && let Some(shared) = self.identical_block(bytes, None, None)
        {
            self.write_pointer(slot, shared)?;
            return Ok(Placement::Shared(shared));
        }
        let placement = self.relocate_fresh(bytes, slot).map_err(|e| {
            if let SaveError::Rats(RatsError::NoSpace { len, .. }) = e {
                SaveError::NoRoomInBank { part: name, bank: 0xFF, len }
            } else {
                e
            }
        })?;
        if unshared && let Some(old) = old_tag {
            rats::free(self.rom, old)?;
            self.freed.push(old);
        }
        Ok(placement)
    }

    /// Writes `bytes` to a new block in expanded space and points `slot` at it.
    fn relocate_fresh(&mut self, bytes: &[u8], slot: Slot) -> Result<Placement, SaveError> {
        self.ensure_expanded()?;
        let req = AllocRequest::new(bytes.len(), self.layout.expanded_start_pc, self.rom.len());
        let tag = rats::alloc(self.rom, &req, bytes)?;
        let new_addr = self
            .rom
            .mapper()
            .to_snes(tag.data_pc)
            .ok_or(RomError::Unmapped(self.layout.layer1_ptrs))?;
        self.write_pointer(slot, new_addr)?;
        Ok(Placement::Relocated { tag, addr: new_addr })
    }

    /// Sprites must stay in the bank the game hardcodes.
    fn place_sprites(&mut self) -> Result<Placement, SaveError> {
        let bytes = self.level.sprite_bytes();
        let addr = self.current.sprites;
        let bank = self.layout.sprite_bank;
        if self.layout.sprite_banks.is_none() && addr.bank() != bank {
            return Err(SaveError::SpriteBank(addr, bank));
        }
        self.place_in_bank(SPRITES, addr, &bytes, BankTarget { bank, page: None })
    }

    /// Background tilemaps must stay in bank `$0C`, on the half that matches
    /// their page.
    fn place_background(&mut self, page: u8) -> Result<Placement, SaveError> {
        let bytes = self.level.layer2_bytes();
        if self.current.layer2_raw.bank() != LAYER2_BACKGROUND_BANK {
            // The slot holds objects today; a background needs a fresh block.
            let tag =
                self.alloc_in_bank("background", &bytes, LAYER2_BACKGROUND_REAL_BANK, Some(page))?;
            let new_addr = self
                .rom
                .mapper()
                .to_snes(tag.data_pc)
                .ok_or(RomError::Unmapped(self.layout.layer2_ptrs))?;
            self.write_pointer(Slot::Layer2, new_addr)?;
            return Ok(Placement::Relocated { tag, addr: new_addr });
        }
        let addr = self.current.layer2;
        let target = BankTarget { bank: LAYER2_BACKGROUND_REAL_BANK, page: Some(page) };
        self.place_in_bank(BACKGROUND, addr, &bytes, target)
    }

    /// In place when the data fits and nobody else points at it, otherwise
    /// into the known free space of `bank`.
    fn place_in_bank(
        &mut self,
        part: Part,
        addr: SnesAddr,
        bytes: &[u8],
        target: BankTarget,
    ) -> Result<Placement, SaveError> {
        let Part { name, kind, slot } = part;
        let BankTarget { bank, page } = target;
        if self.unchanged(addr, bytes) {
            return Ok(Placement::Unchanged);
        }
        let pc = self.rom.pc(addr)?;
        let unshared = self.unshared(addr);
        if unshared && self.capacity(addr, kind).is_some_and(|cap| cap >= bytes.len()) {
            self.rom.write_pc(pc, bytes)?;
            return Ok(Placement::InPlace(addr));
        }
        let old_tag = rats::tag_before(self.rom.bytes(), pc);
        let placement = if let Some(shared) = self.identical_block(bytes, Some(bank), page) {
            Placement::Shared(shared)
        } else {
            let tag = self.alloc_in_bank(name, bytes, bank, page)?;
            let addr = self.rom.mapper().to_snes(tag.data_pc).ok_or(RomError::Unmapped(addr))?;
            Placement::Relocated { tag, addr }
        };
        if let Some(new_addr) = placement.new_addr() {
            self.write_pointer(slot, new_addr)?;
        }
        if unshared && let Some(old) = old_tag {
            rats::free(self.rom, old)?;
            self.freed.push(old);
        }
        Ok(placement)
    }

    /// First known-free range of `bank` (and `page` half) that fits.
    fn alloc_in_bank(
        &mut self,
        part: &'static str,
        bytes: &[u8],
        bank: u8,
        page: Option<u8>,
    ) -> Result<RatsTag, SaveError> {
        let ranges: Vec<_> = self
            .layout
            .known_free
            .iter()
            .filter(|r| r.bank() == bank)
            .filter(|r| page.is_none_or(|p| page_matches(r.pc, p)))
            .copied()
            .collect();
        for range in ranges {
            let req = AllocRequest::new(bytes.len(), range.pc, range.end_pc());
            let result = rats::alloc(self.rom, &req, bytes);
            if matches!(result, Err(RatsError::NoSpace { .. })) {
                continue;
            }
            return Ok(result?);
        }
        Err(SaveError::NoRoomInBank { part, bank, len: bytes.len() })
    }

    fn ensure_expanded(&mut self) -> Result<(), SaveError> {
        if self.rom.len() < self.opts.expand_to {
            self.rom.expand_to(self.opts.expand_to)?;
            self.expanded = true;
        }
        Ok(())
    }

    fn write_pointer(&mut self, slot: Slot, addr: SnesAddr) -> Result<(), SaveError> {
        let index = u32::from(self.level.id.get());
        let layout = self.layout;
        match slot {
            Slot::Layer1 => {
                let [lo, mid, hi, _] = addr.get().to_le_bytes();
                self.rom
                    .write(offset(layout.layer1_ptrs, index.saturating_mul(3))?, &[lo, mid, hi])?;
            }
            Slot::Layer2 => {
                let stored = if matches!(self.level.layer2, Layer2::Background { .. }) {
                    SnesAddr::from_parts(LAYER2_BACKGROUND_BANK, addr.offset())
                } else {
                    addr
                };
                let [lo, mid, hi, _] = stored.get().to_le_bytes();
                self.rom
                    .write(offset(layout.layer2_ptrs, index.saturating_mul(3))?, &[lo, mid, hi])?;
            }
            Slot::Sprites => {
                self.rom.write(
                    offset(layout.sprite_ptrs, index.saturating_mul(2))?,
                    &addr.offset().to_le_bytes(),
                )?;
                if let Some(table) = layout.sprite_banks {
                    self.rom.write(offset(table, index)?, &[addr.bank()])?;
                } else if addr.bank() != layout.sprite_bank {
                    return Err(SaveError::SpriteBank(addr, layout.sprite_bank));
                }
            }
        }
        Ok(())
    }

    /// Writes the secondary header bytes that differ. Returns whether any did.
    fn write_secondary_header(&mut self) -> Result<bool, SaveError> {
        let current = SecondaryHeader::load(self.rom, self.layout, self.level.id)?;
        let wanted = self.level.secondary;
        let index = u32::from(self.level.id.get());
        let mut changed = false;
        for ((table, old), new) in
            self.layout.secondary_header.iter().zip(current.raw).zip(wanted.raw)
        {
            if old != new {
                self.rom.write(offset(*table, index)?, &[new])?;
                changed = true;
            }
        }
        Ok(changed)
    }
}

/// A pointer as stored, with the background marker bank resolved to the bank
/// the data really lives in.
const fn real_address(addr: SnesAddr) -> SnesAddr {
    if addr.bank() == LAYER2_BACKGROUND_BANK {
        SnesAddr::from_parts(LAYER2_BACKGROUND_REAL_BANK, addr.offset())
    } else {
        addr
    }
}

/// Whether a file offset in bank `$0C` lies on the half of the bank that
/// background page `page` requires.
fn page_matches(pc: usize, page: u8) -> bool {
    let within = pc & 0x7FFF;
    let on_second_page = within >= usize::from(SECOND_PAGE_OFFSET & 0x7FFF);
    on_second_page == (page != 0)
}

fn offset(base: SnesAddr, by: u32) -> Result<SnesAddr, RomError> {
    base.checked_add(by).ok_or(RomError::Unmapped(base))
}

#[cfg(test)]
mod tests {
    use super::{Placement, SaveOptions, page_matches, pointer_refs, save_level};
    use crate::level::{Level, SpriteSpec, TilePos};
    use crate::{LevelId, RomLayout};

    fn rom() -> Option<snes_rom::Rom> {
        let path = std::env::var_os("SMW_ROM")?;
        Some(snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads"))
    }

    #[test]
    fn page_halves() {
        assert!(page_matches(0x6_0000, 0));
        assert!(!page_matches(0x6_0000, 1));
        assert!(page_matches(0x6_68FE, 1));
        assert!(!page_matches(0x6_68FD, 1));
    }

    #[test]
    fn real_rom_unchanged_level_writes_nothing() {
        let Some(mut rom) = rom() else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let layout = RomLayout::detect(&rom).expect("vanilla");
        let before = rom.bytes().to_vec();
        let level = Level::load(&rom, &layout, LevelId::new(0x105).expect("valid")).expect("loads");
        let report = save_level(&mut rom, &layout, &level, &SaveOptions::default()).expect("saves");
        assert_eq!(report.layer1, Placement::Unchanged);
        assert_eq!(report.layer2, Placement::Unchanged);
        assert_eq!(report.sprites, Placement::Unchanged);
        assert!(!report.secondary_header);
        assert!(!report.expanded);
        assert_eq!(rom.bytes().len(), before.len());
        // Only the checksum bytes may differ (they are rewritten with the same value).
        assert_eq!(rom.bytes(), &before[..]);
    }

    #[test]
    fn real_rom_relocate_and_reload() {
        let Some(mut rom) = rom() else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let layout = RomLayout::detect(&rom).expect("vanilla");
        let id = LevelId::new(0x105).expect("valid");
        let original = Level::load(&rom, &layout, id).expect("loads");
        let opts = SaveOptions { force_relocate: true, ..SaveOptions::default() };
        let report = save_level(&mut rom, &layout, &original, &opts).expect("saves");
        assert!(report.expanded);
        assert!(matches!(report.layer1, Placement::Relocated { .. }));
        assert_eq!(report.layer2, Placement::Unchanged, "backgrounds are never force-relocated");
        assert_eq!(report.sprites, Placement::Unchanged);
        assert!(rom.checksum_matches());
        let reloaded = Level::load(&rom, &layout, id).expect("reloads");
        assert_ne!(reloaded.addresses.layer1, original.addresses.layer1);
        assert!(rom.pc(reloaded.addresses.layer1).expect("mapped") >= layout.expanded_start_pc);
        assert_eq!(reloaded.layer1_bytes(), original.layer1_bytes());
        assert_eq!(reloaded.layer2_bytes(), original.layer2_bytes());
        assert_eq!(reloaded.sprite_bytes(), original.sprite_bytes());
        assert_eq!(reloaded.secondary, original.secondary);
        // Saving again finds everything in place.
        let again =
            save_level(&mut rom, &layout, &reloaded, &SaveOptions::default()).expect("saves");
        assert_eq!(again.layer1, Placement::Unchanged);
    }

    #[test]
    fn real_rom_edit_sprites() {
        let Some(mut rom) = rom() else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let layout = RomLayout::detect(&rom).expect("vanilla");
        let id = LevelId::new(0x105).expect("valid");
        let refs = pointer_refs(&rom, &layout).expect("reads");
        assert!(refs.values().any(|ids| ids.len() > 1), "vanilla shares some pointers");

        // Dropping a sprite shrinks the list: it fits in place.
        let mut shorter = Level::load(&rom, &layout, id).expect("loads");
        shorter.remove_sprite(0).expect("has sprites");
        let shrink =
            save_level(&mut rom, &layout, &shorter, &SaveOptions::default()).expect("saves");
        assert_eq!(shrink.sprites, Placement::InPlace(shorter.addresses.sprites));
        let mut bigger = Level::load(&rom, &layout, id).expect("reloads");
        assert_eq!(bigger.sprite_bytes(), shorter.sprite_bytes());

        // Adding sprites to a vanilla block cannot fit in place: it moves
        // into bank $07's known free space.
        let spec = SpriteSpec { number: 0x0F, extra_bits: 0, pos: TilePos::new(3, 10) };
        for _ in 0..8 {
            bigger.insert_sprite(0, spec).expect("fits");
        }
        let grow = save_level(&mut rom, &layout, &bigger, &SaveOptions::default()).expect("saves");
        let Placement::Relocated { addr, .. } = grow.sprites else {
            panic!("expected relocation, got {:?}", grow.sprites);
        };
        assert_eq!(addr.bank(), layout.sprite_bank);
        let final_level = Level::load(&rom, &layout, id).expect("reloads");
        assert_eq!(final_level.sprite_bytes(), bigger.sprite_bytes());
        assert!(rom.checksum_matches());
    }

    #[test]
    fn real_rom_copy_level_shares_then_relocates_on_edit() {
        let Some(mut rom) = rom() else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let layout = RomLayout::detect(&rom).expect("vanilla");
        let source_id = LevelId::new(0x105).expect("valid");
        let source = Level::load(&rom, &layout, source_id).expect("loads");

        // Copy level 105 over 1FF: every part differs from what 1FF holds, and
        // every part already exists byte for byte, so the copy shares 105's
        // blocks instead of allocating.
        let mut copy = source.clone();
        copy.id = LevelId::new(0x1FF).expect("valid");
        let copied = save_level(&mut rom, &layout, &copy, &SaveOptions::default()).expect("saves");
        assert_eq!(copied.layer1, Placement::Shared(source.addresses.layer1));
        assert_eq!(copied.layer2, Placement::Shared(source.addresses.layer2));
        assert_eq!(copied.sprites, Placement::Shared(source.addresses.sprites));
        let target = Level::load(&rom, &layout, copy.id).expect("reloads");
        assert_eq!(target.layer1_bytes(), copy.layer1_bytes());
        assert_eq!(target.sprite_bytes(), copy.sprite_bytes());
        assert_eq!(target.secondary, copy.secondary);

        // Editing the copy must not touch the shared block: it relocates.
        let mut edited = target;
        edited.remove_sprite(0).expect("has sprites");
        let moved = save_level(&mut rom, &layout, &edited, &SaveOptions::default()).expect("saves");
        assert!(matches!(moved.sprites, Placement::Relocated { .. }), "{:?}", moved.sprites);
        let untouched = Level::load(&rom, &layout, source_id).expect("reloads");
        assert_eq!(untouched.sprite_bytes(), source.sprite_bytes(), "105 kept its sprites");
        assert!(rom.checksum_matches());
    }
}
