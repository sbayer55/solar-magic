//! Mouse tools and drag state for the level view.

use smw_data::level::TilePos;

/// What a click in the level view does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tool {
    /// Select and move things.
    Select,
    /// Place a standard or extended object.
    PlaceObject {
        /// Object number.
        number: u8,
        /// Size byte or extended object number.
        third: u8,
    },
    /// Place a sprite.
    PlaceSprite {
        /// Sprite number.
        number: u8,
        /// Extra bits.
        extra_bits: u8,
    },
}

/// A drag in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Drag {
    /// No drag.
    None,
    /// Rubber-band selection.
    Band {
        /// Where the drag started.
        anchor: TilePos,
        /// Current corner.
        current: TilePos,
    },
    /// Moving the selection.
    Move {
        /// Where the drag started.
        anchor: TilePos,
        /// Delta already applied to the level.
        applied: (i32, i32),
        /// History group so the steps merge into one undo entry.
        group: u64,
    },
}
