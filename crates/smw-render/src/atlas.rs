//! The 8×8 character table a level sees, mirroring the layer VRAM layout.

use smw_data::gfx::{Bpp, GfxFile, GfxId, Tile8Pixels};

/// Number of 8×8 character slots addressable by a tilemap entry.
pub const CHAR_COUNT: usize = 0x400;

/// Characters per GFX file slot.
pub const CHARS_PER_SLOT: usize = 0x80;

/// One 128-character block of the atlas, in the order the game uploads the
/// level's GFX files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharSlot {
    /// Characters 0x000–0x07F: first foreground file (`FG1`).
    Fg1,
    /// Characters 0x080–0x0FF: second foreground file (`FG2`).
    Fg2,
    /// Characters 0x100–0x17F: background file (`BG1`).
    Bg1,
    /// Characters 0x180–0x1FF: third foreground file (`FG3`).
    Fg3,
}

impl CharSlot {
    /// The four layer slots in upload order.
    pub const LAYER_SLOTS: [Self; 4] = [Self::Fg1, Self::Fg2, Self::Bg1, Self::Fg3];

    /// First character number of this slot.
    #[must_use]
    pub const fn base(self) -> usize {
        match self {
            Self::Fg1 => 0x000,
            Self::Fg2 => 0x080,
            Self::Bg1 => 0x100,
            Self::Fg3 => 0x180,
        }
    }
}

/// 1024 8×8 characters, indexed by tilemap character number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharAtlas {
    chars: Vec<Tile8Pixels>,
}

impl Default for CharAtlas {
    fn default() -> Self {
        Self::new()
    }
}

impl CharAtlas {
    /// An atlas of empty characters.
    #[must_use]
    pub fn new() -> Self {
        Self { chars: vec![Tile8Pixels::EMPTY; CHAR_COUNT] }
    }

    /// Builds the layer atlas from the tileset's four files (`FG1`, `FG2`,
    /// `BG1`, `FG3`). Files with fewer than 128 tiles leave the rest empty.
    #[must_use]
    pub fn from_layer_files(files: &[GfxFile; 4]) -> Self {
        Self::from_slots([Some(&files[0]), Some(&files[1]), Some(&files[2]), Some(&files[3])])
    }

    /// Like [`CharAtlas::from_layer_files`], leaving `None` slots empty.
    #[must_use]
    pub fn from_slots(files: [Option<&GfxFile>; 4]) -> Self {
        let mut atlas = Self::new();
        for (slot, file) in CharSlot::LAYER_SLOTS.iter().zip(files) {
            if let Some(file) = file {
                atlas.load_slot(*slot, file);
            }
        }
        atlas
    }

    /// Copies a file's first 128 tiles into a slot, applying the game's
    /// upload quirk: in GFX01 and GFX17, tiles 0x6E, 0x6F, 0x7E and 0x7F get
    /// their fourth bitplane forced on so non-zero pixels use colours 8–15
    /// (`UploadGFXFile`, `$00:AA5F`). See `docs/formats/gfx.md`.
    ///
    /// The quirk is part of the 3bpp→4bpp conversion, so it only applies to
    /// 3bpp files; Lunar Magic stores its files as 4bpp already converted.
    pub fn load_slot(&mut self, slot: CharSlot, file: &GfxFile) {
        for (i, tile) in file.tiles.iter().take(CHARS_PER_SLOT).enumerate() {
            if let Some(dst) = self.chars.get_mut(slot.base().saturating_add(i)) {
                *dst = if file.bpp == Bpp::Three && uses_high_colors(file.id, i) {
                    raise_to_high_colors(tile)
                } else {
                    *tile
                };
            }
        }
    }

    /// Overwrites one character.
    pub fn set(&mut self, num: usize, tile: Tile8Pixels) {
        if let Some(dst) = self.chars.get_mut(num) {
            *dst = tile;
        }
    }

    /// Character `num`, or `None` above 0x3FF.
    #[must_use]
    pub fn get(&self, num: usize) -> Option<&Tile8Pixels> {
        self.chars.get(num)
    }
}

/// Whether the game forces plane 3 on for this tile of this file.
const fn uses_high_colors(id: GfxId, tile_index: usize) -> bool {
    matches!(id.get(), 0x01 | 0x17) && matches!(tile_index, 0x6E | 0x6F | 0x7E | 0x7F)
}

/// Adds 8 to every non-zero palette index.
fn raise_to_high_colors(tile: &Tile8Pixels) -> Tile8Pixels {
    let mut out = *tile;
    for px in &mut out.pixels {
        if *px != 0 {
            *px |= 0x08;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use smw_data::gfx::{Bpp, GfxFile, GfxId, Tile8Pixels};

    use super::{CharAtlas, CharSlot};

    fn file_with_first_pixel(v: u8) -> GfxFile {
        let mut raw = vec![0u8; 0xC00];
        raw[0] = if v & 1 == 1 { 0x80 } else { 0 };
        GfxFile::decode(GfxId::new(0), Bpp::Three, &raw).expect("valid")
    }

    #[test]
    fn slots_land_at_their_bases() {
        let files = [
            file_with_first_pixel(1),
            file_with_first_pixel(0),
            file_with_first_pixel(1),
            file_with_first_pixel(0),
        ];
        let atlas = CharAtlas::from_layer_files(&files);
        assert_eq!(atlas.get(0x000).map(|t| t.pixels[0]), Some(1));
        assert_eq!(atlas.get(0x080).map(|t| t.pixels[0]), Some(0));
        assert_eq!(atlas.get(0x100).map(|t| t.pixels[0]), Some(1));
        assert_eq!(atlas.get(0x180).map(|t| t.pixels[0]), Some(0));
        assert_eq!(atlas.get(0x200), Some(&Tile8Pixels::EMPTY));
        assert_eq!(atlas.get(0x400), None);
        assert_eq!(CharSlot::Fg3.base(), 0x180);
    }

    #[test]
    fn gfx17_special_tiles_use_high_colors() {
        let mut raw = vec![0u8; 0xC00];
        raw[0x7E * 24] = 0x80; // tile 0x7E, pixel (0,0) = colour 1
        raw[0x10 * 24] = 0x80; // tile 0x10, pixel (0,0) = colour 1
        let file = GfxFile::decode(GfxId::new(0x17), Bpp::Three, &raw).expect("valid");
        let mut atlas = CharAtlas::new();
        atlas.load_slot(CharSlot::Fg2, &file);
        assert_eq!(atlas.get(0x080 + 0x7E).map(|t| t.pixels[0]), Some(9));
        assert_eq!(atlas.get(0x080 + 0x10).map(|t| t.pixels[0]), Some(1));
        let plain = GfxFile::decode(GfxId::new(0x14), Bpp::Three, &raw).expect("valid");
        atlas.load_slot(CharSlot::Fg1, &plain);
        assert_eq!(atlas.get(0x7E).map(|t| t.pixels[0]), Some(1));
    }
}
