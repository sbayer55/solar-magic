//! Feeding an unsaved `Level` to the running game: the serialized bytes go
//! to scratch banks and the ROM's pointer tables are shadowed to point at
//! them, so the game's own loader reads the edited level.

use smw_data::RomLayout;
use smw_data::level::{Layer2, Level};
use snes_core::Console;

/// Scratch bus addresses (unmapped LoROM halves; the bus serves overlays first).
const OVERLAY_LAYER1: u32 = 0x60_8000;
const OVERLAY_LAYER2: u32 = 0x61_8000;
const OVERLAY_SPRITES_LM: u32 = 0x62_8000;
/// Vanilla background tilemaps must sit in bank `$0C` on the half of their page.
const OVERLAY_BACKGROUND: [u32; 2] = [0x0C_8000, 0x0C_E8FE];
/// Bank the vanilla game hardcodes for sprite data.
const VANILLA_SPRITE_BANK: u32 = 0x07;

const fn table_entry(table: snes_rom::SnesAddr, index: u32, stride: u32) -> u32 {
    table.get().wrapping_add(index.wrapping_mul(stride))
}

/// Installs every overlay the game needs to load `level` as edited.
pub(crate) fn install(console: &mut Console, layout: &RomLayout, level: &Level) {
    console.clear_overlays();
    let id = u32::from(level.id.get());
    console.set_overlay(OVERLAY_LAYER1, level.layer1_bytes());
    console.set_overlay(
        table_entry(layout.layer1_ptrs, id, 3),
        OVERLAY_LAYER1.to_le_bytes()[..3].to_vec(),
    );

    let lm_relocated = level.lm.is_some_and(|info| info.flags.relocated_tilemap());
    match &level.layer2 {
        Layer2::Objects { .. } => {
            console.set_overlay(OVERLAY_LAYER2, level.layer2_bytes());
            let ptr = OVERLAY_LAYER2.to_le_bytes()[..3].to_vec();
            console.set_overlay(table_entry(layout.layer2_ptrs, id, 3), ptr);
        }
        Layer2::Background { .. } if lm_relocated => {}
        Layer2::Background { tiles, .. } => {
            let page = tiles.first().map_or(0, |t| usize::from(t.to_le_bytes()[1] & 1));
            let addr = OVERLAY_BACKGROUND.get(page).copied().unwrap_or(OVERLAY_BACKGROUND[0]);
            console.set_overlay(addr, level.layer2_bytes());
            let ptr = (0xFF_0000 | (addr & 0xFFFF)).to_le_bytes()[..3].to_vec();
            console.set_overlay(table_entry(layout.layer2_ptrs, id, 3), ptr);
        }
    }

    let sprites = level.sprite_bytes();
    let sprite_addr = sprite_address(layout, level, sprites.len());
    console.set_overlay(sprite_addr, sprites);
    let [lo, hi, bank, _] = sprite_addr.to_le_bytes();
    console.set_overlay(table_entry(layout.sprite_ptrs, id, 2), vec![lo, hi]);
    if let Some(banks) = layout.sprite_banks {
        console.set_overlay(table_entry(banks, id, 1), vec![bank]);
    }

    for (table, &byte) in layout.secondary_header.iter().zip(level.secondary.raw.iter()) {
        console.set_overlay(table_entry(*table, id, 1), vec![byte]);
    }
}

/// Where the sprite bytes go: Lunar Magic ROMs take a bank byte per level,
/// so a scratch bank works; vanilla hardcodes bank `$07`, so use the
/// largest known-free range there when the data fits, else the level's own
/// slot (shadowing whatever follows it, which only this level session sees).
fn sprite_address(layout: &RomLayout, level: &Level, len: usize) -> u32 {
    if layout.sprite_banks.is_some() {
        return OVERLAY_SPRITES_LM;
    }
    let best = layout
        .known_free
        .iter()
        .filter(|r| u32::from(r.bank()) == VANILLA_SPRITE_BANK && r.len >= len)
        .max_by_key(|r| r.len);
    best.map_or_else(
        || level.addresses.sprites.get(),
        |range| {
            let offset = u32::try_from(range.pc & 0x7FFF).unwrap_or(0) | 0x8000;
            (VANILLA_SPRITE_BANK << 16) | offset
        },
    )
}
