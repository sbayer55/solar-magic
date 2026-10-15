//! Rendering of decoded SMW data to RGBA rasters.
//!
//! Nothing here knows about a GUI toolkit or an image file format: a
//! [`Raster`] is a plain `width × height` array of RGBA8 pixels that the CLI
//! writes as PNG and the editor uploads as a texture.

pub mod assets;
mod atlas;
pub mod emu;
pub mod level;
pub mod map16_ext;
pub mod ownership;
mod raster;
pub mod scene;
pub mod sprites;
mod tiles;

pub use atlas::{CHAR_COUNT, CHARS_PER_SLOT, CharAtlas, CharSlot};
pub use raster::{Raster, Rect, Rgba8, TRANSPARENT};
pub use tiles::{TileContext, draw_map16_tile, draw_tile8};
