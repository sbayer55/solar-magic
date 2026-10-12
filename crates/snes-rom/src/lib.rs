//! SNES ROM container with no game-specific knowledge.
//!
//! This crate answers three questions about a ROM image:
//!
//! 1. How do 24-bit SNES bus addresses map to byte offsets in the file
//!    ([`Mapper`]: [`LoRom`], [`ExLoRom`], [`Sa1Rom`])? See `docs/formats/rom-mapping.md`.
//! 2. What does the internal header say ([`InternalHeader`])?
//! 3. Which regions are claimed by RATS tags ([`rats`])? See `docs/formats/rats.md`.
//!
//! Everything Super Mario World specific lives in `smw-data`.

mod addr;
pub mod copier_header;
mod error;
mod header;
mod mapper;
pub mod rats;
mod rom;
#[cfg(test)]
pub mod test_fixtures;

pub use addr::SnesAddr;
pub use error::RomError;
pub use header::{InternalHeader, MapKind};
pub use mapper::{ExLoRom, LoRom, Mapper, MapperKind, Sa1Rom};
pub use rom::Rom;
