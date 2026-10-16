//! The vanilla USA ROM's table addresses.

use snes_rom::SnesAddr;

use super::{
    EntranceTables, FreeRange, GameModes, LaunchHooks, LaunchRam, LayoutKind, LevelLoader,
    LevelNameTables, Map16Tables, PaletteTables, RomLayout, SplitPointerTable,
};

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
    secondary_exits: [
        SnesAddr::lit(0x05_F800),
        SnesAddr::lit(0x05_FA00),
        SnesAddr::lit(0x05_FC00),
        SnesAddr::lit(0x05_FE00),
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
        ram_map16_pointers: 0x00_0FBE,
    },
    entrance: EntranceTables {
        y_low: SnesAddr::lit(0x05_D730),
        y_high: SnesAddr::lit(0x05_D740),
        x_low: SnesAddr::lit(0x05_D750),
        x_high: SnesAddr::lit(0x05_D758),
    },
    level_names: LevelNameTables {
        names: SnesAddr::lit(0x04_A0FC),
        count: 0x5D,
        strings: SnesAddr::lit(0x04_9AC5),
        first_words: SnesAddr::lit(0x04_9C91),
        second_words: SnesAddr::lit(0x04_9CCF),
        third_words: SnesAddr::lit(0x04_9CED),
        tables_end: SnesAddr::lit(0x04_9D07),
        routine: SnesAddr::lit(0x04_9D28),
        routine_fingerprint: &[
            0xA5, 0x01, 0x29, 0x7F, 0x00, 0x0A, 0xA8, 0xB9, 0x91, 0x9C, 0xA8, 0xE2, 0x20, 0xB9,
            0xC5, 0x9A,
        ],
        max_tiles: 19,
    },
    launch: LaunchHooks {
        level_select_pc: SnesAddr::lit(0x05_D8B7),
        // REP #$30 / LDA $0E / ASL / CLC / ADC $0E / TAY
        level_select_fingerprint: &[0xC2, 0x30, 0xA5, 0x0E, 0x0A, 0x18, 0x65, 0x0E, 0xA8],
        ram: LaunchRam {
            game_mode: 0x0100,
            overworld_override: 0x0109,
            level_number: 0x000E,
            loading_level_number: 0x17BB,
            level_number_copy: 0x010B,
            translevel: 0x13BF,
            player_submap: 0x1F11,
            sublevel_count: 0x141A,
            use_secondary_exit: 0x1B93,
            exit_table_low: 0x19B8,
            exit_table_high: 0x19D8,
            player_x: 0x0094,
            player_y: 0x0096,
            midway_flag: 0x13CE,
            powerup: 0x0019,
            lives: 0x0DBE,
            coins: 0x0DBF,
            item_box: 0x0DC2,
            yoshi_carried: 0x0DC1,
            riding_yoshi: 0x187A,
            yoshi_color: 0x13C7,
            current_player: 0x0DD6,
            ow_level_tile_settings: 0x1EA2,
        },
        modes: GameModes { title: 0x07, overworld: 0x0E, fade_to_level: 0x0F, level: 0x14 },
    },
    expanded_start_pc: 0x08_0000,
    known_free: &VANILLA_FREE_RANGES,
    lm: None,
};

/// Unused byte ranges in the vanilla USA ROM, as file offsets.
///
/// Taken from the SMWDisX file `freespace.txt` (which lists SNES addresses);
/// every range was verified all-`0xFF` against the ROM on 2026-10-07. See
/// `docs/formats/rats.md`.
pub const VANILLA_FREE_RANGES: [FreeRange; 47] = [
    FreeRange::new(0x00_3091, 0x00_309F), // $00:B091-B09F (freespace.txt lists B091; only this part is 0xFF)
    FreeRange::new(0x00_3A4D, 0x00_3A5F), // $00:BA4D-BA5F
    FreeRange::new(0x00_4453, 0x00_445F), // $00:C453-C45F
    FreeRange::new(0x00_79F5, 0x00_7A0F), // $00:F9F5-FA0F
    FreeRange::new(0x00_7F93, 0x00_7FBF), // $00:FF93-FFBF
    FreeRange::new(0x00_AD54, 0x00_AD58), // $01:AD54-AD58
    FreeRange::new(0x00_CD1E, 0x00_CD29), // $01:CD1E-CD29
    FreeRange::new(0x00_E2B0, 0x00_E2C7), // $01:E2B0-E2C7
    FreeRange::new(0x00_FFBF, 0x00_FFFF), // $01:FFBF-FFFF
    FreeRange::new(0x01_35EC, 0x01_362F), // $02:B5EC-B62F
    FreeRange::new(0x01_551E, 0x01_557F), // $02:D51E-D57F
    FreeRange::new(0x01_7FE2, 0x01_7FFF), // $02:FFE2-FFFF
    FreeRange::new(0x01_BA02, 0x01_BFFF), // $03:BA02-BFFF
    FreeRange::new(0x01_D6AC, 0x01_D6FF), // $03:D6AC-D6FF
    FreeRange::new(0x01_E05C, 0x01_E3FF), // $03:E05C-E3FF
    FreeRange::new(0x01_FDE0, 0x01_FFFF), // $03:FDE0-FFFF
    FreeRange::new(0x02_21B6, 0x02_23FF), // $04:A1B6-A3FF
    FreeRange::new(0x02_6F3E, 0x02_727F), // $04:EF3E-F27F
    FreeRange::new(0x02_7FB1, 0x02_7FFF), // $04:FFB1-FFFF
    FreeRange::new(0x02_8E19, 0x02_8FFF), // $05:8E19-8FFF
    FreeRange::new(0x02_A561, 0x02_A57F), // $05:A561-A57F
    FreeRange::new(0x02_BBA6, 0x02_BBFF), // $05:BBA6-BBFF
    FreeRange::new(0x02_CFEA, 0x02_CFFF), // $05:CFEA-CFFF
    FreeRange::new(0x02_D665, 0x02_D707), // $05:D665-D707
    FreeRange::new(0x02_DC46, 0x02_DFFF), // $05:DC46-DFFF
    FreeRange::new(0x03_25B9, 0x03_25FF), // $06:A5B9-A5FF
    FreeRange::new(0x03_4964, 0x03_4FFF), // $06:C964-CFFF
    FreeRange::new(0x03_7539, 0x03_7FFF), // $06:F539-FFFF
    FreeRange::new(0x03_80ED, 0x03_80FF), // $07:80ED-80FF
    FreeRange::new(0x03_A179, 0x03_A5FF), // $07:A179-A5FF
    FreeRange::new(0x03_C226, 0x03_C2FF), // $07:C226-C2FF
    FreeRange::new(0x03_E76F, 0x03_EFFF), // $07:E76F-EFFF
    FreeRange::new(0x03_FC90, 0x03_FFFF), // $07:FC90-FFFF
    FreeRange::new(0x05_FD0D, 0x05_FFFF), // $0B:FD0D-FFFF
    FreeRange::new(0x06_366F, 0x06_37FF), // $0C:B66F-B7FF
    FreeRange::new(0x06_586F, 0x06_58FF), // $0C:D86F-D8FF
    FreeRange::new(0x06_7FDF, 0x06_7FFF), // $0C:FFDF-FFFF
    FreeRange::new(0x06_98C8, 0x06_98FF), // $0D:98C8-98FF
    FreeRange::new(0x06_A0A0, 0x06_A0FF), // $0D:A0A0-A0FF
    FreeRange::new(0x06_BB68, 0x06_BBFF), // $0D:BB68-BBFF
    FreeRange::new(0x06_C620, 0x06_C7FF), // $0D:C620-C7FF
    FreeRange::new(0x06_D282, 0x06_D3FF), // $0D:D282-D3FF
    FreeRange::new(0x06_E186, 0x06_E2FF), // $0D:E186-E2FF
    FreeRange::new(0x06_F08A, 0x06_F2FF), // $0D:F08A-F2FF
    FreeRange::new(0x06_FE9F, 0x06_FFFF), // $0D:FE9F-FFFF
    FreeRange::new(0x07_70F0, 0x07_7FFF), // $0E:F0F0-FFFF
    FreeRange::new(0x07_EF90, 0x07_FFFF), // $0F:EF90-FFFF
];
