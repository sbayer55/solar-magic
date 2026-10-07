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

    /// The ROM carries a Lunar Magic signature but not the 2.5x routines the
    /// layout depends on.
    #[error(
        "ROM was saved by {signature:?}, but this Lunar Magic version is not supported: {missing} not found"
    )]
    LunarMagicUnsupported {
        /// The version string Lunar Magic wrote into the ROM.
        signature: String,
        /// Which expected routine or table was not where Lunar Magic 2.5x puts it.
        missing: &'static str,
    },

    /// An underlying ROM access failed.
    #[error(transparent)]
    Rom(#[from] RomError),
}
