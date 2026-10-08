//! `solar dump-exits`: print secondary exits.

use std::path::Path;

use anyhow::Context as _;
use smw_data::RomLayout;
use smw_data::level::{ExitId, SecondaryExit};
use snes_rom::Rom;

/// Prints one exit, or every non-blank exit when `wanted` is `None`.
pub(crate) fn run(path: &Path, wanted: Option<u16>) -> anyhow::Result<()> {
    let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    let ids: Vec<ExitId> = match wanted {
        Some(n) => vec![ExitId::new(n).with_context(|| format!("exit {n:#X} is out of range"))?],
        None => ExitId::all().collect(),
    };
    println!("  exit  bytes        dest  screen  x  y  type");
    for id in ids {
        let exit = SecondaryExit::load(&rom, &layout, id)?;
        if exit.is_blank() && wanted.is_none() {
            continue;
        }
        println!(
            "  {id}   {:02X} {:02X} {:02X} {:02X}  {:02X}    {:>2}      {}  {:>2}  {}",
            exit.raw[0],
            exit.raw[1],
            exit.raw[2],
            exit.raw[3],
            exit.destination_low(),
            exit.entrance_screen(),
            exit.entrance_x_index(),
            exit.entrance_y_index(),
            exit.entrance_type()
        );
    }
    Ok(())
}
