//! `solar bg-rle`: check the background RLE encoder against every level.

use std::path::Path;

use anyhow::Context as _;
use smw_data::level::{Layer2, Level, encode_bg_rle};
use smw_data::{LevelId, RomLayout};
use snes_rom::Rom;

/// Re-encodes every background tilemap and reports sizes.
pub(crate) fn check(path: &Path) -> anyhow::Result<()> {
    let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    let (mut levels, mut stored, mut encoded_total, mut smaller) = (0usize, 0usize, 0usize, 0usize);
    for id in LevelId::all() {
        let level = Level::load(&rom, &layout, id)?;
        let Layer2::Background { tiles, compressed, rows_per_screen, .. } = &level.layer2 else {
            continue;
        };
        let mut decoded_in: Vec<u8> = tiles.iter().map(|t| t.to_le_bytes()[0]).collect();
        if *rows_per_screen == smw_data::level::BG_ROWS_16BIT {
            decoded_in.extend(tiles.iter().map(|t| t.to_le_bytes()[1]));
        }
        let encoded = encode_bg_rle(&decoded_in);
        let (decoded, _) = smw_data::level::decode_bg_rle_capped(&encoded, decoded_in.len())
            .context("re-decoding")?;
        anyhow::ensure!(decoded == decoded_in, "level {id}: re-encoded tilemap differs");
        levels = levels.saturating_add(1);
        stored = stored.saturating_add(compressed.len());
        encoded_total = encoded_total.saturating_add(encoded.len());
        if encoded.len() < compressed.len() {
            smaller = smaller.saturating_add(1);
        }
    }
    println!(
        "{levels} background levels: stored {stored} bytes, re-encoded {encoded_total} bytes ({smaller} smaller, none larger)"
    );
    Ok(())
}
