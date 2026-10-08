//! Super Mario World data formats.
//!
//! Every address in this crate is reached through a [`RomLayout`], which is
//! resolved once when a ROM is opened. Vanilla ROMs use the fixed tables in
//! [`layout::VANILLA`]; ROMs modified by Lunar Magic relocate several of them
//! and will get their own layout once that work lands (see
//! `docs/formats/lunar-magic-ext.md`).

mod error;
pub mod gfx;
pub mod layout;
pub mod level;
mod level_id;
pub mod map16;
pub mod palette;
pub mod tileset;

pub use error::LayoutError;
pub use layout::{EntranceTables, FreeRange, LayoutKind, RomLayout};
pub use level_id::LevelId;
