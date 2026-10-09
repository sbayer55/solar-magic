//! What the eight `.mwl` sections mean: exporting a level into them and
//! building a level back out of them. `docs/formats/mwl.md`

use snes_rom::{Rom, RomError, SnesAddr};

use super::mwl::{Mwl, MwlError, Section};
use crate::gfx_bypass::{self, GfxBypass};
use crate::level::{
    BG_ROWS, BG_ROWS_16BIT, BG_TILEMAP_LEN, BG_TILEMAP_LEN_16BIT, ExitId, Layer2, Level,
    LevelAddresses, LmLevelFlags, LmLevelInfo, ObjectStream, PrimaryHeader, SecondaryExit,
    SecondaryHeader, SpriteList, encode_bg_rle,
};
use crate::palette::{Bgr555, Palette, level_palette};
use crate::{LevelId, RomLayout};

/// Length of the level-info section.
const INFO_LEN: usize = 0x40;
/// Prefix every data section carries: a flags byte, padding, and the
/// source ROM address (informational).
const PREFIX_LEN: usize = 8;
/// Tiles per screen in the exported background (32 rows × 16 columns).
const EXPORT_SCREEN_TILES: usize = 0x200;
/// Screens in an exported background.
const EXPORT_SCREENS: usize = 2;
/// Bytes per exported secondary exit.
const EXIT_ENTRY_LEN: usize = 8;
/// Colours in the palette section: 256 plus the back area colour.
const PALETTE_COLORS: usize = 257;
/// Layer 2 flag bits meaning "background tilemap" (either kind).
const TILEMAP_BITS: u8 = 0x0A;
/// Layer 2 flag bit meaning "16-bit tilemap".
const SIXTEEN_BIT: u8 = 0x04;
/// Layer 2 flag bit Lunar Magic 3.x uses for a vanilla background.
const VANILLA_BANK_BIT: u8 = 0x08;
/// Layer 1 prefix bit: the level uses a custom palette.
const CUSTOM_PALETTE_BIT: u8 = 0x01;

/// Export settings.
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// Comment written into the header (48 bytes, space padded).
    pub comment: Vec<u8>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self { comment: b"Solar Magic  .mwl in the Lunar Magic 3.71 format".to_vec() }
    }
}

/// Exports `level` from `rom` (which supplies the palette, secondary exits
/// and Lunar Magic's per-level bytes).
///
/// # Errors
///
/// ROM read failures.
pub fn export_level(
    rom: &Rom,
    layout: &RomLayout,
    level: &Level,
    opts: &ExportOptions,
) -> Result<Mwl, RomError> {
    let mut mwl = Mwl::new(&opts.comment);
    mwl.set_section(Section::LevelInfo, level_info(rom, layout, level)?);
    let custom = level.lm.and_then(|i| i.custom_palette);
    let mut layer1 = prefix(u8::from(custom.is_some()), Some(level.addresses.layer1));
    layer1.extend(level.layer1_bytes());
    mwl.set_section(Section::Layer1, layer1);
    mwl.set_section(Section::Layer2, layer2_section(level));
    let mut sprites = prefix(0, Some(level.addresses.sprites));
    sprites.extend(level.sprite_bytes());
    mwl.set_section(Section::Sprites, sprites);
    let mut palette = prefix(0, custom);
    palette.extend(palette_words(&level_palette(rom, layout, level)?));
    mwl.set_section(Section::Palette, palette);
    mwl.set_section(Section::SecondaryExits, secondary_exits_section(rom, layout, level.id)?);
    let exanim = layout.lm.map_or(Ok(0), |lm| lm.exanim_flag(rom, level.id))?;
    mwl.set_section(Section::ExAnimation, prefix(exanim, None));
    let row = GfxBypass::load(rom, layout, level.id)?.unwrap_or_else(GfxBypass::default_row);
    mwl.set_section(Section::GfxBypass, row.words.iter().flat_map(|w| w.to_le_bytes()).collect());
    Ok(mwl)
}

/// A section prefix: `flags`, three zero bytes, the 24-bit address, zero.
fn prefix(flags: u8, addr: Option<SnesAddr>) -> Vec<u8> {
    let [lo, mid, hi, _] = addr.map_or(0, SnesAddr::get).to_le_bytes();
    vec![flags, 0, 0, 0, lo, mid, hi, 0]
}

fn level_info(rom: &Rom, layout: &RomLayout, level: &Level) -> Result<Vec<u8>, RomError> {
    let mut out = vec![0u8; INFO_LEN];
    out.splice(0..2, level.id.get().to_le_bytes());
    out.splice(2..6, level.secondary.raw);
    if let (Some(lm), Some(info)) = (&layout.lm, &level.lm) {
        let [byte4, byte5, byte6] = info.secondary_ext;
        let dimensions = lm.level_dimensions(rom, level.id)?.unwrap_or(0);
        let fa00 = rom.read_u8(entry(lm.level_ext_fa00, u32::from(level.id.get()))?)?;
        for (i, value) in [(6, byte4), (14, byte5), (15, byte6), (16, dimensions), (17, fa00)] {
            if let Some(slot) = out.get_mut(i) {
                *slot = value;
            }
        }
    }
    Ok(out)
}

/// Index into a tilemap stored `stride` tiles per screen.
const fn tile_index(screen: usize, stride: usize, row: usize, col: usize) -> usize {
    screen.saturating_mul(stride).saturating_add(row.saturating_mul(16)).saturating_add(col)
}

/// Layer 2 as Lunar Magic stores it: objects verbatim, backgrounds as two
/// screens of 32 × 16 little-endian tile words (unused rows zero), with the
/// flags byte describing a 16-bit tilemap.
fn layer2_section(level: &Level) -> Vec<u8> {
    match &level.layer2 {
        Layer2::Objects { .. } => {
            let mut out = prefix(0, Some(level.addresses.layer2));
            out.extend(level.layer2_bytes());
            out
        }
        Layer2::Background { page_set, rows_per_screen, tiles, .. } => {
            let flags = level.lm.map_or(VANILLA_BANK_BIT | (page_set << 4), |i| i.flags.0);
            let mut out = prefix(flags | SIXTEEN_BIT, Some(level.addresses.layer2));
            let rows = usize::from(*rows_per_screen);
            let stride = rows.saturating_mul(16);
            for screen in 0..EXPORT_SCREENS {
                for row in 0..usize::from(BG_ROWS_16BIT) {
                    for col in 0..16 {
                        let tile = (row < rows)
                            .then(|| tiles.get(tile_index(screen, stride, row, col)))
                            .flatten()
                            .copied()
                            .unwrap_or(0);
                        out.extend(tile.to_le_bytes());
                    }
                }
            }
            out
        }
    }
}

/// 256 colours then the back area colour; colour 0 is written as black.
fn palette_words(palette: &Palette) -> Vec<u8> {
    let mut out = Vec::with_capacity(PALETTE_COLORS * 2);
    for index in 0..256usize {
        let color = if index == 0 {
            Bgr555::BLACK
        } else {
            palette.get(index / 16, index % 16).unwrap_or_default()
        };
        out.extend(color.raw().to_le_bytes());
    }
    out.extend(palette.get(0, 0).unwrap_or_default().raw().to_le_bytes());
    out
}

/// Every secondary exit whose destination is `id`, as 8-byte entries after
/// an 8-byte zero header.
fn secondary_exits_section(
    rom: &Rom,
    layout: &RomLayout,
    id: LevelId,
) -> Result<Vec<u8>, RomError> {
    let mut out = vec![0u8; PREFIX_LEN];
    for exit in ExitId::all() {
        let data = SecondaryExit::load(rom, layout, exit)?;
        if data.destination(layout) != id.get() {
            continue;
        }
        let ext =
            layout.lm.map_or(Ok(None), |lm| lm.secondary_exit_ext(rom, exit))?.unwrap_or([0; 2]);
        out.extend(exit.get().to_le_bytes());
        out.extend([data.raw[1], data.raw[2], data.raw[3], ext[0], ext[1], 0]);
    }
    Ok(out)
}

/// What an imported `.mwl` holds besides the level itself.
#[derive(Debug, Clone)]
pub struct MwlImport {
    /// The level, addressed at slot `id` of the target ROM.
    pub level: Level,
    /// The exported palette (256 colours; row 0 colour 0 = back area).
    pub palette: Palette,
    /// Whether the source level used a custom palette.
    pub custom_palette: bool,
    /// Secondary exits leading into the level.
    pub secondary_exits: Vec<ImportedExit>,
    /// The GFX bypass row.
    pub bypass: GfxBypass,
    /// The `ExAnimation` flag byte (`$03:FE00`).
    pub exanim_flag: u8,
}

/// A secondary exit carried by a `.mwl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportedExit {
    /// Exit number.
    pub id: ExitId,
    /// The four table bytes, destination filled in.
    pub exit: SecondaryExit,
    /// Lunar Magic 3.x's two extra bytes.
    pub ext: [u8; 2],
}

/// Builds a level for slot `id` of `rom` out of `mwl`.
///
/// # Errors
///
/// [`MwlError::BadSection`] for malformed sections,
/// [`MwlError::NeedsLunarMagic`] when the level needs a feature the ROM's
/// layout lacks.
pub fn import_level(
    rom: &Rom,
    layout: &RomLayout,
    mwl: &Mwl,
    id: LevelId,
) -> Result<MwlImport, MwlError> {
    let info = mwl.section(Section::LevelInfo);
    let secondary =
        SecondaryHeader { raw: [byte(info, 2), byte(info, 3), byte(info, 4), byte(info, 5)] };
    let (header, layer1, custom_palette) = layer1_from(mwl.section(Section::Layer1), layout)?;
    let (mut layer2, flags) = layer2_from(mwl.section(Section::Layer2), layout)?;
    reuse_stored_stream(rom, layout, id, &mut layer2);
    let sprites =
        SpriteList::parse(body(mwl.section(Section::Sprites))).ok_or(MwlError::BadSection {
            section: Section::Sprites,
            reason: "unterminated sprite list",
        })?;
    let addresses = LevelAddresses::load(rom, layout, id).map_err(|_| MwlError::BadSection {
        section: Section::LevelInfo,
        reason: "slot unreadable",
    })?;
    let vertical = header.level_mode().is_vertical(rom, layout).unwrap_or(false);
    let lm = layout.lm.map(|lm| LmLevelInfo {
        flags: LmLevelFlags(flags),
        format: lm.layer2_flags_format,
        custom_palette: None,
        secondary_ext: [byte(info, 6), byte(info, 14), byte(info, 15)],
    });
    let level = Level {
        id,
        addresses,
        header,
        secondary,
        vertical,
        format: layout.object_format(),
        layer1,
        layer2,
        sprites,
        lm,
    };
    let bypass_bytes = mwl.section(Section::GfxBypass);
    let mut words = [gfx_bypass::UNSET; gfx_bypass::SLOT_COUNT];
    for (i, word) in words.iter_mut().enumerate() {
        *word = word_at(bypass_bytes, i);
    }
    Ok(MwlImport {
        level,
        palette: palette_from(mwl.section(Section::Palette)),
        custom_palette,
        secondary_exits: exits_from(mwl.section(Section::SecondaryExits), id),
        bypass: GfxBypass::from_words(words),
        exanim_flag: byte(mwl.section(Section::ExAnimation), 0),
    })
}

fn layer1_from(
    section: &[u8],
    layout: &RomLayout,
) -> Result<(PrimaryHeader, ObjectStream, bool), MwlError> {
    let bad = |reason| MwlError::BadSection { section: Section::Layer1, reason };
    let data = body(section);
    let header = PrimaryHeader::parse(data).ok_or_else(|| bad("missing primary header"))?;
    let stream =
        ObjectStream::parse(data.get(PrimaryHeader::LEN..).unwrap_or(&[]), layout.object_format())
            .ok_or_else(|| bad("unterminated object stream"))?;
    Ok((header, stream, byte(section, 0) & CUSTOM_PALETTE_BIT != 0))
}

/// Layer 2 and the flags byte the ROM should carry for it.
fn layer2_from(section: &[u8], layout: &RomLayout) -> Result<(Layer2, u8), MwlError> {
    let bad = |reason| MwlError::BadSection { section: Section::Layer2, reason };
    let flags = byte(section, 0);
    let data = body(section);
    if flags & TILEMAP_BITS == 0 {
        let header_copy: [u8; 5] = data
            .get(..5)
            .and_then(|h| h.try_into().ok())
            .ok_or_else(|| bad("missing header copy"))?;
        let stream = ObjectStream::parse(data.get(5..).unwrap_or(&[]), layout.object_format())
            .ok_or_else(|| bad("unterminated object stream"))?;
        return Ok((Layer2::Objects { header_copy, stream }, 0));
    }
    if data.len() < EXPORT_SCREENS.saturating_mul(EXPORT_SCREEN_TILES).saturating_mul(2) {
        return Err(bad("background tilemap is short"));
    }
    let words: Vec<u16> =
        data.chunks_exact(2).map(|c| u16::from_le_bytes([byte(c, 0), byte(c, 1)])).collect();
    match classify_background(&words) {
        None => {
            if layout.lm.is_none() {
                return Err(MwlError::NeedsLunarMagic("16-bit background tilemap"));
            }
            let mut decoded = vec![0u8; BG_TILEMAP_LEN_16BIT];
            for (i, word) in words.iter().take(BG_TILEMAP_LEN).enumerate() {
                let [lo, hi] = word.to_le_bytes();
                if let Some(slot) = decoded.get_mut(i) {
                    *slot = lo;
                }
                if let Some(slot) = decoded.get_mut(i.saturating_add(BG_TILEMAP_LEN)) {
                    *slot = hi;
                }
            }
            let compressed = encode_bg_rle(&decoded);
            let stored = (flags & !VANILLA_BANK_BIT) | 0x02 | SIXTEEN_BIT;
            Ok((Layer2::background_16bit(flags >> 4, &decoded, compressed), stored))
        }
        Some(page) => {
            let mut tilemap = vec![0x25u8; BG_TILEMAP_LEN];
            let stride = usize::from(BG_ROWS).saturating_mul(16);
            for screen in 0..EXPORT_SCREENS {
                for row in 0..usize::from(BG_ROWS) {
                    for col in 0..16 {
                        let word = words
                            .get(tile_index(screen, EXPORT_SCREEN_TILES, row, col))
                            .copied()
                            .unwrap_or(0);
                        if let Some(slot) = tilemap.get_mut(tile_index(screen, stride, row, col)) {
                            *slot = word.to_le_bytes()[0];
                        }
                    }
                }
            }
            let compressed = encode_bg_rle(&tilemap);
            let stored = if layout.lm.is_some() { VANILLA_BANK_BIT | (page << 4) } else { 0 };
            Ok((Layer2::background_8bit(page, &tilemap, compressed), stored))
        }
    }
}

/// `Some(page)` when the exported tilemap is really an 8-bit one: every tile
/// of rows 0–26 sits on one page and rows 27–31 are empty. `None` means a
/// genuine 32-row, multi-page (16-bit) tilemap.
fn classify_background(words: &[u16]) -> Option<u8> {
    let mut page: Option<u8> = None;
    for screen in 0..EXPORT_SCREENS {
        for row in 0..usize::from(BG_ROWS_16BIT) {
            for col in 0..16 {
                let word = words.get(tile_index(screen, EXPORT_SCREEN_TILES, row, col)).copied()?;
                if row >= usize::from(BG_ROWS) {
                    if word != 0 {
                        return None;
                    }
                    continue;
                }
                let high = u8::try_from(word >> 8).unwrap_or(0);
                match page {
                    Some(p) if p != high => return None,
                    _ => page = Some(high),
                }
            }
        }
    }
    page
}

/// Keeps the ROM's own RLE stream when the imported background has exactly
/// the tiles the slot already holds, so saving sees it as unchanged.
fn reuse_stored_stream(rom: &Rom, layout: &RomLayout, id: LevelId, layer2: &mut Layer2) {
    let Layer2::Background { tiles, rows_per_screen, compressed, .. } = layer2 else {
        return;
    };
    let Ok(current) = Level::load(rom, layout, id) else {
        return;
    };
    let Layer2::Background {
        tiles: stored, rows_per_screen: stored_rows, compressed: stream, ..
    } = current.layer2
    else {
        return;
    };
    let used = usize::from(*rows_per_screen).saturating_mul(16).saturating_mul(EXPORT_SCREENS);
    if stored_rows == *rows_per_screen && tiles.get(..used) == stored.get(..used) {
        *compressed = stream;
        *tiles = stored;
    }
}

fn palette_from(section: &[u8]) -> Palette {
    let data = body(section);
    let word = |i: usize| Bgr555::new(word_at(data, i));
    let mut palette = Palette::black();
    for index in 1..256usize {
        palette.set(index / 16, index % 16, word(index));
    }
    palette.set(0, 0, word(256));
    palette
}

fn exits_from(section: &[u8], id: LevelId) -> Vec<ImportedExit> {
    body(section)
        .chunks_exact(EXIT_ENTRY_LEN)
        .filter_map(|e| {
            let exit = ExitId::new(u16::from_le_bytes([byte(e, 0), byte(e, 1)]))?;
            let [dest_low, _] = id.get().to_le_bytes();
            let data = SecondaryExit { raw: [dest_low, byte(e, 2), byte(e, 3), byte(e, 4)] };
            Some(ImportedExit { id: exit, exit: data, ext: [byte(e, 5), byte(e, 6)] })
        })
        .collect()
}

/// A section's bytes after the 8-byte prefix.
fn body(section: &[u8]) -> &[u8] {
    section.get(PREFIX_LEN..).unwrap_or(&[])
}

fn byte(bytes: &[u8], i: usize) -> u8 {
    bytes.get(i).copied().unwrap_or(0)
}

/// The `i`th little-endian word of `bytes` (0 past the end).
fn word_at(bytes: &[u8], i: usize) -> u16 {
    let at = i.saturating_mul(2);
    u16::from_le_bytes([byte(bytes, at), byte(bytes, at.saturating_add(1))])
}

fn entry(table: SnesAddr, index: u32) -> Result<SnesAddr, RomError> {
    table.checked_add(index).ok_or(RomError::Unmapped(table))
}

#[cfg(test)]
mod tests {
    use super::{ExportOptions, export_level, import_level};
    use crate::formats::mwl::{Mwl, Section};
    use crate::level::{ExitId, Layer2, Level};
    use crate::palette::level_palette;
    use crate::{LevelId, RomLayout};

    fn rom(var: &str) -> Option<snes_rom::Rom> {
        let Some(path) = std::env::var_os(var) else {
            eprintln!("{var} not set; skipping");
            return None;
        };
        Some(snes_rom::Rom::load(std::path::Path::new(&path)).expect("loads"))
    }

    fn lm_mwl() -> Option<Mwl> {
        let Some(path) = std::env::var_os("LM_MWL") else {
            eprintln!("LM_MWL not set; skipping");
            return None;
        };
        Some(Mwl::parse(&std::fs::read(path).expect("reads")).expect("parses"))
    }

    /// Our export of level 105 from the 3.71 ROM equals Lunar Magic's own
    /// export of it, section for section.
    #[test]
    fn lm3_export_matches_lm_export() {
        let (Some(rom), Some(theirs)) = (rom("LM3_ROM"), lm_mwl()) else { return };
        let layout = RomLayout::detect(&rom).expect("lunar magic");
        let level = Level::load(&rom, &layout, LevelId::new(0x105).unwrap()).expect("loads");
        let opts = ExportOptions { comment: theirs.comment.to_vec() };
        let ours = export_level(&rom, &layout, &level, &opts).expect("exports");
        for section in Section::ALL {
            assert_eq!(ours.section(section), theirs.section(section), "{section:?}");
        }
        assert_eq!(ours.to_bytes(), theirs.to_bytes());
    }

    /// Importing Lunar Magic's export reproduces the level in the ROM.
    #[test]
    fn lm3_import_reproduces_level() {
        let (Some(rom), Some(mwl)) = (rom("LM3_ROM"), lm_mwl()) else { return };
        let layout = RomLayout::detect(&rom).expect("lunar magic");
        let id = LevelId::new(0x105).unwrap();
        let original = Level::load(&rom, &layout, id).expect("loads");
        let imported = import_level(&rom, &layout, &mwl, id).expect("imports");
        assert_eq!(imported.level.layer1_bytes(), original.layer1_bytes());
        assert_eq!(imported.level.sprite_bytes(), original.sprite_bytes());
        assert_eq!(imported.level.secondary, original.secondary);
        assert_eq!(imported.level.lm.map(|i| i.flags), original.lm.map(|i| i.flags));
        let (Layer2::Background { tiles: a, .. }, Layer2::Background { tiles: b, .. }) =
            (&imported.level.layer2, &original.layer2)
        else {
            panic!("expected backgrounds");
        };
        assert_eq!(&a[..0x360], &b[..0x360]);
        assert!(!imported.custom_palette);
        assert_eq!(imported.palette, level_palette(&rom, &layout, &original).expect("palette"));
        let exits: Vec<_> =
            imported.secondary_exits.iter().map(|e| (e.id, e.exit.raw, e.ext)).collect();
        let exit = ExitId::new(0x1CB).unwrap();
        assert_eq!(exits, vec![(exit, [0x05, 0xA9, 0x08, 0x0E], [0, 0])]);
        assert_eq!(
            imported.bypass.layer3_files().map(|f| f.map(crate::gfx::GfxId::get)),
            [Some(0x2B), Some(0x2A), Some(0x29), Some(0x28)]
        );
        // The slot holds the same tiles, so the ROM's stream is kept.
        assert_eq!(imported.level.layer2_bytes(), original.layer2_bytes());
    }

    /// Every vanilla level survives export then import.
    #[test]
    fn vanilla_levels_round_trip_through_mwl() {
        let Some(rom) = rom("SMW_ROM") else { return };
        let layout = RomLayout::detect(&rom).expect("vanilla");
        for id in LevelId::all() {
            let level = Level::load(&rom, &layout, id).expect("loads");
            let mwl =
                export_level(&rom, &layout, &level, &ExportOptions::default()).expect("exports");
            let back = Mwl::parse(&mwl.to_bytes()).expect("parses");
            let imported = import_level(&rom, &layout, &back, id).expect("imports").level;
            assert_eq!(imported.header, level.header, "level {id}");
            assert_eq!(imported.secondary, level.secondary, "level {id}");
            assert_eq!(imported.layer1_bytes(), level.layer1_bytes(), "level {id}");
            assert_eq!(imported.sprite_bytes(), level.sprite_bytes(), "level {id}");
            match (&imported.layer2, &level.layer2) {
                (Layer2::Objects { .. }, Layer2::Objects { .. }) => {
                    assert_eq!(imported.layer2_bytes(), level.layer2_bytes(), "level {id}");
                }
                (Layer2::Background { tiles: a, .. }, Layer2::Background { tiles: b, .. }) => {
                    assert_eq!(&a[..0x360], &b[..0x360], "level {id}");
                    assert_eq!(imported.layer2_bytes(), level.layer2_bytes(), "level {id}");
                }
                _ => panic!("level {id}: layer 2 kind changed"),
            }
        }
    }
}
