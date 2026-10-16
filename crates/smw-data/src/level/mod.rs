//! Level data: headers, object streams, layer 2, sprites.
//!
//! Everything parsed here re-serializes to the exact bytes it came from
//! ([`Level::layer1_bytes`] and friends), which is both the round-trip test
//! and the basis for writing levels back later.

mod catalogue;
mod edit;
mod exits;
pub mod fields;
mod header;
mod layer2;
mod lm_info;
pub mod lm_objects;
mod names;
mod objects;
mod save;
mod screens;
mod sprites;

pub use catalogue::{Catalogue, ExitEdge, LevelSummary, Parent};
pub use edit::{EditError, Layer, ObjectSpec, ScreenExitInfo, SpriteSpec, entrance_pixel};
pub use exits::{ExitId, SecondaryExit};
pub use fields::{BitField, FieldError};
pub use header::{LevelMode, PrimaryHeader, SecondaryHeader};
pub use layer2::{
    BG_ROWS, BG_ROWS_16BIT, BG_TILEMAP_LEN, BG_TILEMAP_LEN_16BIT, BgTilemap, DecodedBackground,
    Layer2, decode_bg_rle, decode_bg_rle_capped, encode_bg_rle,
};
pub use lm_info::{LmLevelFlags, LmLevelInfo};
pub use names::{OverworldNames, tiles_to_text};
pub use objects::{MAX_OBJECT_LEN, ObjectFormat, ObjectKind, ObjectStream, RawObject};
pub use save::{Placement, SaveError, SaveOptions, SaveReport, pointer_refs, save_level};
pub use screens::{SCREEN_TILES, TilePos, join_position, split_position};
pub use sprites::{Sprite, SpriteList};

use snes_rom::{Rom, RomError, SnesAddr};

use crate::{LevelId, RomLayout};

/// Bank byte that marks a layer 2 pointer as a background tilemap.
const LAYER2_BACKGROUND_BANK: u8 = 0xFF;

/// Bank that background tilemaps actually live in.
const LAYER2_BACKGROUND_REAL_BANK: u8 = 0x0C;

/// A fully parsed level.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Level {
    /// Which level this is.
    pub id: LevelId,
    /// Where each part was read from.
    pub addresses: LevelAddresses,
    /// The 5-byte primary header.
    pub header: PrimaryHeader,
    /// The 4 secondary header bytes.
    pub secondary: SecondaryHeader,
    /// Whether the level mode is vertical (from the ROM's mode table).
    pub vertical: bool,
    /// How objects are encoded (new objects are built in this format).
    pub format: ObjectFormat,
    /// Layer 1 objects.
    pub layer1: ObjectStream,
    /// Layer 2 objects or background.
    pub layer2: Layer2,
    /// Sprites.
    pub sprites: SpriteList,
    /// Lunar Magic's per-level records, for Lunar Magic layouts.
    pub lm: Option<LmLevelInfo>,
}

/// Source addresses of a level's parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LevelAddresses {
    /// Layer 1 data (header + objects).
    pub layer1: SnesAddr,
    /// Layer 2 pointer exactly as stored (bank `$FF` for backgrounds).
    pub layer2_raw: SnesAddr,
    /// Layer 2 data as actually read.
    pub layer2: SnesAddr,
    /// Sprite data.
    pub sprites: SnesAddr,
}

impl LevelAddresses {
    /// Reads a level's three pointers from the tables. `layer2` is the
    /// address actually read (bank `$0C` for vanilla backgrounds).
    ///
    /// # Errors
    ///
    /// ROM read failures.
    pub fn load(rom: &Rom, layout: &RomLayout, id: LevelId) -> Result<Self, RomError> {
        let index = u32::from(id.get());
        let layer1 = rom.read_ptr(offset(layout.layer1_ptrs, index.saturating_mul(3))?)?;
        let layer2_raw = rom.read_ptr(offset(layout.layer2_ptrs, index.saturating_mul(3))?)?;
        let sprite_offset =
            rom.read_u16_le(offset(layout.sprite_ptrs, index.saturating_mul(2))?)?;
        let sprite_bank = match layout.sprite_banks {
            Some(table) => rom.read_u8(offset(table, index)?)?,
            None => layout.sprite_bank,
        };
        let layer2 = if layer2_raw.bank() == LAYER2_BACKGROUND_BANK {
            SnesAddr::from_parts(LAYER2_BACKGROUND_REAL_BANK, layer2_raw.offset())
        } else {
            layer2_raw
        };
        Ok(Self {
            layer1,
            layer2_raw,
            layer2,
            sprites: SnesAddr::from_parts(sprite_bank, sprite_offset),
        })
    }
}

/// Errors from loading a level.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LevelError {
    /// The object stream ran off the end of the ROM without a terminator.
    #[error("level {level}: {layer} object stream at {addr} is unterminated")]
    UnterminatedObjects {
        /// Level in question.
        level: LevelId,
        /// `"layer 1"` or `"layer 2"`.
        layer: &'static str,
        /// Where the stream starts.
        addr: SnesAddr,
    },

    /// The sprite list ran off the end of the ROM without a terminator.
    #[error("level {level}: sprite list at {addr} is unterminated")]
    UnterminatedSprites {
        /// Level in question.
        level: LevelId,
        /// Where the list starts.
        addr: SnesAddr,
    },

    /// A background tilemap stream was malformed.
    #[error("level {level}: background tilemap at {addr} is malformed ({reason})")]
    BadBackground {
        /// Level in question.
        level: LevelId,
        /// Where the stream starts.
        addr: SnesAddr,
        /// What went wrong.
        reason: &'static str,
    },

    /// ROM access failed.
    #[error(transparent)]
    Rom(#[from] RomError),
}

impl Level {
    /// Loads and parses a level.
    ///
    /// # Errors
    ///
    /// See [`LevelError`].
    pub fn load(rom: &Rom, layout: &RomLayout, id: LevelId) -> Result<Self, LevelError> {
        let pointers = LevelAddresses::load(rom, layout, id)?;
        let layer1_addr = pointers.layer1;
        let layer2_raw = pointers.layer2_raw;
        let sprites_addr = pointers.sprites;

        let format = layout.object_format();
        let layer1_data = tail(rom, layer1_addr)?;
        let header = PrimaryHeader::parse(layer1_data).ok_or(RomError::Unmapped(layer1_addr))?;
        let layer1 =
            ObjectStream::parse(layer1_data.get(PrimaryHeader::LEN..).unwrap_or(&[]), format)
                .ok_or(LevelError::UnterminatedObjects {
                    level: id,
                    layer: "layer 1",
                    addr: layer1_addr,
                })?;

        let secondary = SecondaryHeader::load(rom, layout, id)?;
        let vertical = header.level_mode().is_vertical(rom, layout)?;
        let lm = LmLevelInfo::load(rom, layout, id)?;
        let flags = lm.map(|info| info.flags).unwrap_or_default();

        let (layer2_addr, layer2) = if layer2_raw.bank() == LAYER2_BACKGROUND_BANK {
            let addr = SnesAddr::from_parts(LAYER2_BACKGROUND_REAL_BANK, layer2_raw.offset());
            let page = u8::from(layer2_raw.offset() >= layer2::SECOND_PAGE_OFFSET);
            (addr, load_background(rom, id, addr, BackgroundShape::EightBit { page })?)
        } else if lm.is_some_and(|info| info.has_tilemap()) {
            let shape = if flags.sixteen_bit() {
                BackgroundShape::SixteenBit { page_set: flags.page() }
            } else {
                BackgroundShape::EightBit { page: flags.page() }
            };
            (layer2_raw, load_background(rom, id, layer2_raw, shape)?)
        } else {
            (layer2_raw, load_layer2_objects(rom, id, layer2_raw, format)?)
        };

        let sprites = SpriteList::parse(tail(rom, sprites_addr)?)
            .ok_or(LevelError::UnterminatedSprites { level: id, addr: sprites_addr })?;

        Ok(Self {
            id,
            addresses: LevelAddresses {
                layer1: layer1_addr,
                layer2_raw,
                layer2: layer2_addr,
                sprites: sprites_addr,
            },
            header,
            secondary,
            vertical,
            format,
            layer1,
            layer2,
            sprites,
            lm,
        })
    }

    /// The layer 1 data exactly as it should appear in ROM.
    #[must_use]
    pub fn layer1_bytes(&self) -> Vec<u8> {
        let mut out = self.header.raw.to_vec();
        out.extend(self.layer1.to_bytes());
        out
    }

    /// The layer 2 data exactly as it should appear in ROM.
    #[must_use]
    pub fn layer2_bytes(&self) -> Vec<u8> {
        self.layer2.to_bytes()
    }

    /// The sprite data exactly as it should appear in ROM.
    #[must_use]
    pub fn sprite_bytes(&self) -> Vec<u8> {
        self.sprites.to_bytes()
    }
}

/// How a background tilemap stream is to be decoded.
#[derive(Debug, Clone, Copy)]
enum BackgroundShape {
    /// 0x400 tile low bytes, all on `page`.
    EightBit {
        /// BG page of every tile.
        page: u8,
    },
    /// 0x400 low bytes followed by 0x400 high bytes (Lunar Magic).
    SixteenBit {
        /// Which 16-page BG Map16 set the tiles index.
        page_set: u8,
    },
}

/// Decodes the background tilemap at `addr`.
fn load_background(
    rom: &Rom,
    id: LevelId,
    addr: SnesAddr,
    shape: BackgroundShape,
) -> Result<Layer2, LevelError> {
    let cap = match shape {
        BackgroundShape::EightBit { .. } => BG_TILEMAP_LEN,
        BackgroundShape::SixteenBit { .. } => BG_TILEMAP_LEN_16BIT,
    };
    let (decoded, compressed) = decode_bg_rle_capped(tail(rom, addr)?, cap)
        .ok_or(LevelError::BadBackground { level: id, addr, reason: "unterminated or overlong" })?;
    Ok(match shape {
        BackgroundShape::EightBit { page } => Layer2::background_8bit(page, &decoded, compressed),
        BackgroundShape::SixteenBit { page_set } => {
            Layer2::background_16bit(page_set, &decoded, compressed)
        }
    })
}

/// Parses the layer 2 object stream (with its unused header copy) at `addr`.
fn load_layer2_objects(
    rom: &Rom,
    id: LevelId,
    addr: SnesAddr,
    format: ObjectFormat,
) -> Result<Layer2, LevelError> {
    let data = tail(rom, addr)?;
    let header_copy: [u8; 5] =
        data.get(..5).and_then(|b| b.try_into().ok()).ok_or(RomError::Unmapped(addr))?;
    let stream = ObjectStream::parse(data.get(5..).unwrap_or(&[]), format)
        .ok_or(LevelError::UnterminatedObjects { level: id, layer: "layer 2", addr })?;
    Ok(Layer2::Objects { header_copy, stream })
}

/// The ROM from `addr` to the end of the image.
fn tail(rom: &Rom, addr: SnesAddr) -> Result<&[u8], RomError> {
    let pc = rom.pc(addr)?;
    rom.bytes().get(pc..).ok_or(RomError::Unmapped(addr))
}

fn offset(base: SnesAddr, by: u32) -> Result<SnesAddr, RomError> {
    base.checked_add(by).ok_or(RomError::Unmapped(base))
}

#[cfg(test)]
mod tests {
    use super::{
        LAYER2_BACKGROUND_BANK, LAYER2_BACKGROUND_REAL_BANK, Layer2, Level, ObjectFormat,
        ObjectStream, PrimaryHeader, layer2,
    };
    use crate::{LevelId, RomLayout};

    /// Every Lunar Magic-tagged block re-serializes to exactly the bytes the
    /// RATS tag protects, which pins every object length rule.
    #[test]
    fn lm_rom_rats_oracle() {
        rats_oracle("LM_ROM", 30, 5);
    }

    #[test]
    fn lm3_rom_rats_oracle() {
        rats_oracle("LM3_ROM", 2, 0);
    }

    /// Every level of the Lunar Magic ROM named by `var` re-serializes to
    /// its stored bytes, and every RATS-tagged part fills its tag exactly.
    #[allow(clippy::arithmetic_side_effects, reason = "test bookkeeping")]
    fn rats_oracle(var: &str, min_tagged: usize, min_sixteen_bit: usize) {
        let Some(path) = std::env::var_os(var) else {
            eprintln!("{var} not set; skipping Lunar Magic ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = RomLayout::detect(&rom).expect("lunar magic");
        assert!(layout.is_lunar_magic());
        let (mut tagged, mut sixteen_bit) = (0, 0);
        for id in LevelId::all() {
            let level =
                Level::load(&rom, &layout, id).unwrap_or_else(|e| panic!("level {id}: {e}"));
            let parts = [
                ("layer1", level.addresses.layer1, level.layer1_bytes()),
                ("layer2", level.addresses.layer2, level.layer2_bytes()),
                ("sprites", level.addresses.sprites, level.sprite_bytes()),
            ];
            for (name, addr, bytes) in parts {
                let pc = rom.pc(addr).expect("mapped");
                assert_eq!(bytes, &rom.bytes()[pc..pc + bytes.len()], "level {id} {name}");
                if let Some(tag) = pc
                    .checked_sub(snes_rom::rats::TAG_LEN)
                    .and_then(|tag_pc| snes_rom::rats::RatsTag::parse_at(rom.bytes(), tag_pc))
                {
                    assert_eq!(bytes.len(), tag.data_len, "level {id} {name} vs RATS size");
                    tagged += 1;
                }
            }
            if let Layer2::Background { rows_per_screen: 32, .. } = level.layer2 {
                sixteen_bit += 1;
            }
        }
        assert!(tagged >= min_tagged, "expected at least {min_tagged} tagged blocks, got {tagged}");
        assert!(
            sixteen_bit >= min_sixteen_bit,
            "expected at least {min_sixteen_bit} 16-bit backgrounds, got {sixteen_bit}"
        );
    }

    /// In a 3.x ROM the flags byte of every untouched vanilla level mirrors
    /// the shape of its layer 2 pointer: bit 3 for backgrounds (now stored
    /// with bank `$0C`), bit 4 for the ones on the second page.
    #[test]
    fn lm3_flags_match_pointer_shape() {
        let Some(path) = std::env::var_os("LM3_ROM") else {
            eprintln!("LM3_ROM not set; skipping Lunar Magic ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = RomLayout::detect(&rom).expect("lunar magic");
        let mut backgrounds = 0;
        for id in LevelId::all() {
            let level = Level::load(&rom, &layout, id).expect("loads");
            let info = level.lm.expect("lunar magic info");
            let raw = level.addresses.layer2_raw;
            assert_ne!(raw.bank(), LAYER2_BACKGROUND_BANK, "level {id}: $FF marker survives");
            if info.flags.relocated_tilemap() {
                continue;
            }
            let is_bg = raw.bank() == LAYER2_BACKGROUND_REAL_BANK;
            assert_eq!(
                info.flags.vanilla_bank_tilemap(),
                is_bg,
                "level {id} flags {:#04X}",
                info.flags.0
            );
            if is_bg {
                backgrounds += 1;
                let page1 = raw.offset() >= layer2::SECOND_PAGE_OFFSET;
                assert_eq!(info.flags.page(), u8::from(page1), "level {id}");
                assert!(matches!(level.layer2, Layer2::Background { .. }), "level {id}");
            } else {
                assert!(matches!(level.layer2, Layer2::Objects { .. }), "level {id}");
            }
        }
        assert!(backgrounds > 100, "{backgrounds} background levels");
    }

    /// Vanilla object streams mean the same thing under the Lunar Magic rules.
    #[test]
    fn real_rom_streams_agree_under_lm_format() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = RomLayout::detect(&rom).expect("vanilla");
        for id in LevelId::all() {
            let level = Level::load(&rom, &layout, id).expect("loads");
            let pc = rom.pc(level.addresses.layer1).expect("mapped") + PrimaryHeader::LEN;
            let lm = ObjectStream::parse(&rom.bytes()[pc..], ObjectFormat::LunarMagic)
                .expect("terminated");
            assert_eq!(lm.to_bytes(), level.layer1.to_bytes(), "level {id}");
            assert_eq!(lm.objects.len(), level.layer1.objects.len(), "level {id}");
        }
    }

    /// Every vanilla level parses and re-serializes to its original bytes.
    #[test]
    fn real_rom_all_levels_round_trip() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads");
        let layout = RomLayout::detect(&rom).expect("vanilla");
        let mut backgrounds = 0;
        for id in LevelId::all() {
            let level =
                Level::load(&rom, &layout, id).unwrap_or_else(|e| panic!("level {id}: {e}"));
            let check = |name: &str, addr: snes_rom::SnesAddr, bytes: Vec<u8>| {
                let pc = rom.pc(addr).expect("mapped");
                let original = &rom.bytes()[pc..pc + bytes.len()];
                assert_eq!(bytes, original, "level {id} {name} round trip");
            };
            check("layer1", level.addresses.layer1, level.layer1_bytes());
            check("layer2", level.addresses.layer2, level.layer2_bytes());
            check("sprites", level.addresses.sprites, level.sprite_bytes());
            if matches!(level.layer2, Layer2::Background { .. }) {
                backgrounds += 1;
            }
        }
        assert!(backgrounds > 50, "expected many background levels, got {backgrounds}");

        // Yoshi's Island 2 (level 105) is a well-known horizontal grass level.
        let yi2 = Level::load(&rom, &layout, LevelId::new(0x105).expect("valid")).expect("loads");
        assert_eq!(yi2.header.tileset(), 7, "Yoshi's Island 2 uses tileset 7 (Normal 2)");
        assert!(!yi2.header.level_mode().is_vertical(&rom, &layout).expect("reads"));
        assert!(yi2.layer1.objects.len() > 20);
        assert!(yi2.sprites.sprites.len() > 5);
    }
}
