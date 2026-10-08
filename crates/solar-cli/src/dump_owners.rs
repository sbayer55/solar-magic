//! `solar dump-owners`: print which object owns each layer 1 tile.

use std::path::Path;

use anyhow::Context as _;
use smw_data::{LevelId, RomLayout};
use smw_render::level::load_level;
use snes_rom::Rom;

/// Prints the owner grid of layer 1 (object index per tile, `..` for none).
pub(crate) fn run(path: &Path, number: u16) -> anyhow::Result<()> {
    let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    let id = LevelId::new(number).with_context(|| format!("level {number:#X} is out of range"))?;
    let loaded = load_level(&rom, &layout, id)?;
    let owners = &loaded.ownership.layer1;
    println!(
        "level {id}: {} objects, grid {}x{}",
        loaded.level.layer1.objects.len(),
        owners.width(),
        owners.height()
    );
    for y in 0..owners.height() {
        let row: Vec<String> = (0..owners.width())
            .map(|x| {
                owners.get(x, y).flatten().map_or_else(|| "..".to_owned(), |o| format!("{o:02X}"))
            })
            .collect();
        println!("{}", row.join(" "));
    }
    let boxed = loaded.ownership.bounds1.iter().flatten().count();
    println!("{boxed} objects own at least one visible tile");
    Ok(())
}
