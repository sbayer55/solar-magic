//! Primary and secondary level headers. See `docs/formats/level-header.md`.

use snes_rom::{Rom, RomError, SnesAddr};

use crate::palette::PaletteSelection;
use crate::{LevelId, RomLayout};

/// The 5-byte header at the start of layer 1 data.
///
/// Fields are decoded on demand from the raw bytes so the header always
/// re-serializes exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct PrimaryHeader {
    /// The header bytes as stored.
    pub raw: [u8; 5],
}

impl PrimaryHeader {
    /// Header length in bytes.
    pub const LEN: usize = 5;

    /// Takes the first five bytes of `data`.
    #[must_use]
    pub fn parse(data: &[u8]) -> Option<Self> {
        data.get(..Self::LEN).and_then(|b| b.try_into().ok()).map(|raw| Self { raw })
    }

    /// Number of screens, 1–32.
    #[must_use]
    pub const fn screens(self) -> u8 {
        (self.raw[0] & 0x1F).saturating_add(1)
    }

    /// Background palette set, 0–7.
    #[must_use]
    pub const fn background_palette(self) -> u8 {
        self.raw[0] >> 5
    }

    /// Level mode, 0–31.
    #[must_use]
    pub const fn level_mode(self) -> LevelMode {
        LevelMode(self.raw[1] & 0x1F)
    }

    /// Back area colour, 0–7.
    #[must_use]
    pub const fn back_area_color(self) -> u8 {
        self.raw[1] >> 5
    }

    /// Sprite set, 0–15.
    #[must_use]
    pub const fn sprite_set(self) -> u8 {
        self.raw[2] & 0x0F
    }

    /// Music index, 0–7.
    #[must_use]
    pub const fn music(self) -> u8 {
        (self.raw[2] >> 4) & 0x07
    }

    /// Whether layer 3 has priority over sprites.
    #[must_use]
    pub const fn layer3_priority(self) -> bool {
        self.raw[2] & 0x80 != 0
    }

    /// Foreground palette set, 0–7.
    #[must_use]
    pub const fn foreground_palette(self) -> u8 {
        self.raw[3] & 0x07
    }

    /// Sprite palette set, 0–7.
    #[must_use]
    pub const fn sprite_palette(self) -> u8 {
        (self.raw[3] >> 3) & 0x07
    }

    /// Time limit code, 0–3 (0, 200, 300, 400 seconds).
    #[must_use]
    pub const fn time_limit_code(self) -> u8 {
        self.raw[3] >> 6
    }

    /// Time limit in seconds.
    #[must_use]
    pub const fn time_limit_seconds(self) -> u16 {
        match self.time_limit_code() {
            1 => 200,
            2 => 300,
            3 => 400,
            _ => 0,
        }
    }

    /// FG/BG tileset, 0–15.
    #[must_use]
    pub const fn tileset(self) -> u8 {
        self.raw[4] & 0x0F
    }

    /// Horizontal/vertical scroll setting, 0–3 (3 disables both).
    #[must_use]
    pub const fn scroll_setting(self) -> u8 {
        (self.raw[4] >> 4) & 0x03
    }

    /// Item memory setting, 0–3.
    #[must_use]
    pub const fn item_memory(self) -> u8 {
        self.raw[4] >> 6
    }

    /// The palette sets this header selects.
    #[must_use]
    pub const fn palette_selection(self) -> PaletteSelection {
        PaletteSelection {
            background: self.background_palette(),
            foreground: self.foreground_palette(),
            sprite: self.sprite_palette(),
            back_area_color: self.back_area_color(),
            player: crate::palette::PlayerPalette::Mario,
        }
    }
}

/// A level mode (header byte 1, bits 0–4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LevelMode(u8);

impl LevelMode {
    /// The raw mode number.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Whether this mode is a vertical level, per the ROM's `VerticalTable`.
    ///
    /// # Errors
    ///
    /// ROM read failures.
    pub fn is_vertical(self, rom: &Rom, layout: &RomLayout) -> Result<bool, RomError> {
        Ok(self.table_entry(rom, layout)? & 0x01 != 0)
    }

    /// Whether layer 2 scrolls vertically with the level in this mode.
    ///
    /// # Errors
    ///
    /// ROM read failures.
    pub fn is_layer2_vertical(self, rom: &Rom, layout: &RomLayout) -> Result<bool, RomError> {
        Ok(self.table_entry(rom, layout)? & 0x80 != 0)
    }

    /// Boss arenas and the like: modes for which the game loads no objects.
    #[must_use]
    pub const fn loads_no_objects(self) -> bool {
        matches!(self.0, 0x09 | 0x0B | 0x10)
    }

    /// Modes whose layer 2 is not loaded (no layer 2 objects).
    #[must_use]
    pub const fn skips_layer2(self) -> bool {
        matches!(self.0, 0x00 | 0x0A | 0x0C | 0x0D | 0x0E | 0x11 | 0x1E)
    }

    fn table_entry(self, rom: &Rom, layout: &RomLayout) -> Result<u8, RomError> {
        let addr = layout
            .level_mode_table
            .checked_add(u32::from(self.0))
            .ok_or(RomError::Unmapped(layout.level_mode_table))?;
        rom.read_u8(addr)
    }
}

/// The four secondary header bytes, one from each table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct SecondaryHeader {
    /// Bytes from `$05:F000`, `$05:F200`, `$05:F400`, `$05:F600`.
    pub raw: [u8; 4],
}

impl SecondaryHeader {
    /// Reads the level's bytes from the four tables.
    ///
    /// # Errors
    ///
    /// ROM read failures.
    pub fn load(rom: &Rom, layout: &RomLayout, id: LevelId) -> Result<Self, RomError> {
        let mut raw = [0u8; 4];
        for (slot, table) in raw.iter_mut().zip(layout.secondary_header.iter()) {
            let addr: SnesAddr = table
                .checked_add(u32::try_from(id.index()).unwrap_or(0))
                .ok_or(RomError::Unmapped(*table))?;
            *slot = rom.read_u8(addr)?;
        }
        Ok(Self { raw })
    }

    /// Player entrance Y position index, 0–15 (byte 0 bits 0–3).
    #[must_use]
    pub const fn entrance_y_index(self) -> u8 {
        self.raw[0] & 0x0F
    }

    /// Layer 2 scroll setting, 0–15 (byte 0 bits 4–7).
    #[must_use]
    pub const fn layer2_scroll(self) -> u8 {
        self.raw[0] >> 4
    }

    /// Player entrance X position index, 0–7 (byte 1 bits 0–2).
    #[must_use]
    pub const fn entrance_x_index(self) -> u8 {
        self.raw[1] & 0x07
    }

    /// Entrance action, 0–7 (byte 1 bits 3–5).
    #[must_use]
    pub const fn entrance_type(self) -> u8 {
        (self.raw[1] >> 3) & 0x07
    }

    /// Layer 3 setting, 0–3 (byte 1 bits 6–7).
    #[must_use]
    pub const fn layer3_setting(self) -> u8 {
        self.raw[1] >> 6
    }

    /// Initial layer 2 vertical position code, 0–3 (byte 2 bits 0–1).
    #[must_use]
    pub const fn layer2_initial_y(self) -> u8 {
        self.raw[2] & 0x03
    }

    /// Initial layer 1 vertical position code, 0–3 (byte 2 bits 2–3).
    #[must_use]
    pub const fn layer1_initial_y(self) -> u8 {
        (self.raw[2] >> 2) & 0x03
    }

    /// Midway entrance screen, 0–15 (byte 2 bits 4–7).
    #[must_use]
    pub const fn midway_screen(self) -> u8 {
        self.raw[2] >> 4
    }

    /// Main entrance screen, 0–31 (byte 3 bits 0–4).
    #[must_use]
    pub const fn entrance_screen(self) -> u8 {
        self.raw[3] & 0x1F
    }

    /// Screen mode bits (byte 3 bits 5–6).
    #[must_use]
    pub const fn screen_mode(self) -> u8 {
        (self.raw[3] >> 5) & 0x03
    }

    /// Whether the "No Yoshi" intro is disabled (byte 3 bit 7).
    #[must_use]
    pub const fn disable_no_yoshi_intro(self) -> bool {
        self.raw[3] & 0x80 != 0
    }
}

#[cfg(test)]
mod tests {
    use super::{PrimaryHeader, SecondaryHeader};

    // screens 0x0A+1, bg pal 3 | mode 0x1F, back area 7 | sprite set 5, music 6, l3 prio |
    // fg pal 2, sprite pal 4, time 3 | tileset 9, scroll 2, item memory 1
    const RAW: [u8; 5] =
        [0x60 | 0x0A, 0xE0 | 0x1F, 0x80 | 0x60 | 0x05, 0xC0 | 0x20 | 0x02, 0x40 | 0x20 | 0x09];

    #[test]
    fn primary_header_bytes_0_to_2() {
        let h = PrimaryHeader::parse(&RAW).expect("5 bytes");
        assert_eq!(h.screens(), 11);
        assert_eq!(h.background_palette(), 3);
        assert_eq!(h.level_mode().get(), 0x1F);
        assert_eq!(h.back_area_color(), 7);
        assert_eq!(h.sprite_set(), 5);
        assert_eq!(h.music(), 6);
        assert!(h.layer3_priority());
        assert!(PrimaryHeader::parse(&RAW[..4]).is_none());
    }

    #[test]
    fn primary_header_bytes_3_to_4() {
        let h = PrimaryHeader::parse(&RAW).expect("5 bytes");
        assert_eq!(h.foreground_palette(), 2);
        assert_eq!(h.sprite_palette(), 4);
        assert_eq!(h.time_limit_code(), 3);
        assert_eq!(h.time_limit_seconds(), 400);
        assert_eq!(h.tileset(), 9);
        assert_eq!(h.scroll_setting(), 2);
        assert_eq!(h.item_memory(), 1);
        assert_eq!(h.palette_selection().foreground, 2);
    }

    #[test]
    fn secondary_header_fields() {
        let s = SecondaryHeader { raw: [0xA5, 0xD3, 0x7E, 0xF3] };
        assert_eq!(s.entrance_y_index(), 5);
        assert_eq!(s.layer2_scroll(), 0xA);
        assert_eq!(s.entrance_x_index(), 3);
        assert_eq!(s.entrance_type(), 2);
        assert_eq!(s.layer3_setting(), 3);
        assert_eq!(s.layer2_initial_y(), 2);
        assert_eq!(s.layer1_initial_y(), 3);
        assert_eq!(s.midway_screen(), 7);
        assert_eq!(s.entrance_screen(), 0x13);
        assert_eq!(s.screen_mode(), 3);
        assert!(s.disable_no_yoshi_intro());
    }
}
