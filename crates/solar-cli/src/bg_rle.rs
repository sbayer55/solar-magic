//! `solar bg-rle`: check the background RLE encoder against every level.

use std::path::Path;

use anyhow::Context as _;
use smw_data::level::{Layer2, Level, decode_bg_rle, encode_bg_rle};
use smw_data::{LevelId, RomLayout};
use snes_rom::Rom;

/// Re-encodes every background tilemap and reports sizes.
pub(crate) fn check(path: &Path) -> anyhow::Result<()> {
    let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    let (mut levels, mut stored, mut encoded_total, mut smaller) = (0usize, 0usize, 0usize, 0usize);
    for id in LevelId::all() {
        let level = Level::load(&rom, &layout, id)?;
        let Layer2::Background { tilemap, compressed, .. } = &level.layer2 else {
            continue;
        };
        let encoded = encode_bg_rle(tilemap);
        let (decoded, _) = decode_bg_rle(&encoded).context("re-decoding")?;
        anyhow::ensure!(decoded == *tilemap, "level {id}: re-encoded tilemap differs");
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
