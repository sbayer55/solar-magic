//! Error types.

use snes_rom::RomError;

/// Errors from resolving a [`crate::RomLayout`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum LayoutError {
    /// The internal header title is not Super Mario World's.
    #[error("not a Super Mario World ROM (title is {title:?})")]
    NotSmw {
        /// The title that was found.
        title: String,
    },

    /// The ROM has been modified by Lunar Magic, which is not supported yet.
    #[error("ROM was saved by {signature:?}; Lunar Magic-modified ROMs are not supported yet")]
    LunarMagicUnsupported {
        /// The version string Lunar Magic wrote into the ROM.
        signature: String,
    },

    /// An underlying ROM access failed.
    #[error(transparent)]
    Rom(#[from] RomError),
}
