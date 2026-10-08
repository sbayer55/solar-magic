//! `solar gfx check`: re-compress every GFX file and verify the encoder.

use std::path::Path;

use anyhow::Context as _;
use smw_data::RomLayout;
use smw_data::gfx::GfxId;
use snes_rom::Rom;

/// Decompresses, recompresses and re-decompresses every vanilla GFX file.
pub(crate) fn check(path: &Path) -> anyhow::Result<()> {
    let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    let (mut files, mut stored, mut packed, mut larger) = (0usize, 0usize, 0usize, 0usize);
    for n in 0..u16::try_from(layout.gfx_ptrs.count).unwrap_or(0) {
        let id = GfxId::new(n);
        let Ok(addr) = smw_data::gfx::gfx_address(&rom, &layout, id) else {
            continue;
        };
        let pc = rom.pc(addr)?;
        let tail = rom.bytes().get(pc..).unwrap_or(&[]);
        let original = lc_lz2::decompress(tail).with_context(|| format!("{id}"))?;
        let compressed = lc_lz2::compress(&original.data);
        let again = lc_lz2::decompress(&compressed).with_context(|| format!("{id} re-decode"))?;
        anyhow::ensure!(again.data == original.data, "{id}: round trip differs");
        files = files.saturating_add(1);
        stored = stored.saturating_add(original.consumed);
        packed = packed.saturating_add(compressed.len());
        if compressed.len() > original.consumed {
            larger = larger.saturating_add(1);
            println!("  {id}: {} -> {} bytes (larger)", original.consumed, compressed.len());
        }
    }
    println!(
        "{files} GFX files: stored {stored} bytes, re-compressed {packed} bytes, {larger} larger"
    );
    Ok(())
}
