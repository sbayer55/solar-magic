//! The vanilla USA ROM's table addresses.

use snes_rom::SnesAddr;

use super::{LayoutKind, LevelLoader, Map16Tables, PaletteTables, RomLayout, SplitPointerTable};

/// The vanilla USA ROM layout.
pub const VANILLA: RomLayout = RomLayout {
    kind: LayoutKind::Vanilla,
    layer1_ptrs: SnesAddr::lit(0x05_E000),
    layer2_ptrs: SnesAddr::lit(0x05_E600),
    sprite_ptrs: SnesAddr::lit(0x05_EC00),
    sprite_bank: 0x07,
    sprite_banks: None,
    secondary_header: [
        SnesAddr::lit(0x05_F000),
        SnesAddr::lit(0x05_F200),
        SnesAddr::lit(0x05_F400),
        SnesAddr::lit(0x05_F600),
    ],
    gfx_ptrs: SplitPointerTable {
        low: SnesAddr::lit(0x00_B992),
        high: SnesAddr::lit(0x00_B9C4),
        bank: SnesAddr::lit(0x00_B9F6),
        count: 0x32,
    },
    palettes: PaletteTables {
        back_area_colors: SnesAddr::lit(0x00_B0A0),
        background: SnesAddr::lit(0x00_B0B0),
        status_bar: SnesAddr::lit(0x00_B170),
        foreground: SnesAddr::lit(0x00_B190),
        standard: SnesAddr::lit(0x00_B250),
        player: SnesAddr::lit(0x00_B2C8),
        sprite: SnesAddr::lit(0x00_B318),
        berry: SnesAddr::lit(0x00_B674),
    },
    map16: Map16Tables {
        common: SnesAddr::lit(0x0D_8000),
        bg_tiles: SnesAddr::lit(0x0D_9100),
        tileset_locations: SnesAddr::lit(0x05_8000),
        tileset_bank: 0x0D,
        tileset_bitmask: SnesAddr::lit(0x05_81BB),
        appearance_table: SnesAddr::lit(0x05_8776),
        diagonal_pipes: SnesAddr::lit(0x0D_8A70),
    },
    layer_gfx_list: SnesAddr::lit(0x00_A92B),
    sprite_gfx_list: SnesAddr::lit(0x00_A8C3),
    level_mode_table: SnesAddr::lit(0x05_8417),
    loader: LevelLoader {
        entry: SnesAddr::lit(0x05_801E),
        tile_ram_ptrs_low: SnesAddr::lit(0x00_BEA8),
        tile_ram_ptrs_high: SnesAddr::lit(0x00_BEAC),
        ram_layer1_ptr: 0x00_0065,
        ram_layer2_ptr: 0x00_0068,
        ram_sprite_ptr: 0x00_00CE,
        ram_game_mode: 0x00_0100,
        ram_level_number: 0x00_000E,
        ram_level_number_copy: 0x00_010B,
        ram_bg_tilemap_low: 0x7E_B900,
        ram_bg_tilemap_high: 0x7E_BD00,
    },
    lm: None,
};
