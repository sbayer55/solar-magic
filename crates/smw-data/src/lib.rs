//! Super Mario World data formats.
//!
//! Every address in this crate is reached through a [`RomLayout`], which is
//! resolved once when a ROM is opened. Vanilla ROMs use the fixed tables in
//! [`layout::VANILLA`]; ROMs saved by Lunar Magic 2.5x add the tables in
//! [`layout::LM_25X`] (see `docs/formats/lunar-magic-ext.md`).

mod error;
pub mod formats;
pub mod gfx;
pub mod gfx_bypass;
pub mod layout;
pub mod level;
mod level_id;
pub mod map16;
pub mod palette;
pub mod tileset;

pub use error::LayoutError;
pub use layout::{
    EntranceTables, FreeRange, Layer2FlagsFormat, LayoutKind, LevelNameTables, LmFeatures,
    LunarMagicTables, RomLayout,
};
pub use level_id::LevelId;
