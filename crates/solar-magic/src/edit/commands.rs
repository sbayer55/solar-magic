//! Undoable edits.

use std::collections::BTreeMap;

use smw_data::level::{
    BitField, EditError, ExitId, Layer, Level, ObjectSpec, RawObject, ScreenExitInfo,
    SecondaryExit, Sprite, SpriteSpec, TilePos,
};

/// Secondary exits edited alongside a level, keyed by exit number. They are
/// ROM-global, so they live next to the level rather than inside it.
pub(crate) type SecondaryExits = BTreeMap<ExitId, SecondaryExit>;

/// One undoable change to a level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EditCommand {
    /// Shift objects by a tile delta.
    MoveObjects {
        /// Layer.
        layer: Layer,
        /// Stream indices.
        indices: Vec<usize>,
        /// Delta in tiles.
        delta: (i32, i32),
    },
    /// Shift sprites by a tile delta.
    MoveSprites {
        /// Sprite indices.
        indices: Vec<usize>,
        /// Delta in tiles.
        delta: (i32, i32),
    },
    /// Remove objects.
    DeleteObjects {
        /// Layer.
        layer: Layer,
        /// Stream indices.
        indices: Vec<usize>,
    },
    /// Remove sprites.
    DeleteSprites {
        /// Sprite indices.
        indices: Vec<usize>,
    },
    /// Add an object at the end of the stream.
    InsertObject {
        /// Layer.
        layer: Layer,
        /// What to insert.
        spec: ObjectSpec,
    },
    /// Add a sprite at the end of the list.
    InsertSprite {
        /// What to insert.
        spec: SpriteSpec,
    },
    /// Replace an object's bytes.
    ReplaceObject {
        /// Layer.
        layer: Layer,
        /// Stream index.
        index: usize,
        /// New object.
        object: RawObject,
    },
    /// Replace a sprite's bytes.
    ReplaceSprite {
        /// Sprite index.
        index: usize,
        /// New sprite.
        sprite: Sprite,
    },
    /// Set a primary header field.
    SetHeaderField {
        /// Field.
        field: BitField,
        /// Value.
        value: u8,
    },
    /// Set a secondary header field.
    SetSecondaryField {
        /// Field.
        field: BitField,
        /// Value.
        value: u8,
    },
    /// Set the screen count.
    SetScreens {
        /// New count.
        screens: u8,
    },
    /// Set a secondary exit's four table bytes.
    SetSecondaryExit {
        /// Exit number.
        id: ExitId,
        /// New bytes.
        exit: SecondaryExit,
    },
    /// Set or remove a screen exit.
    SetScreenExit {
        /// Screen.
        screen: u8,
        /// The exit, or `None` to remove it.
        info: Option<ScreenExitInfo>,
    },
    /// Append copied objects, positioned relative to `at`.
    PasteObjects {
        /// Layer.
        layer: Layer,
        /// Objects with their offsets from the paste origin.
        objects: Vec<(RawObject, TilePos)>,
        /// Paste origin.
        at: TilePos,
    },
    /// Append copied sprites, positioned relative to `at`.
    PasteSprites {
        /// Sprites with their offsets from the paste origin.
        sprites: Vec<(Sprite, TilePos)>,
        /// Paste origin.
        at: TilePos,
    },
}

/// Adds a tile delta to a position, clamping at zero.
fn shifted(pos: TilePos, delta: (i32, i32)) -> TilePos {
    let add = |v: u16, d: i32| u16::try_from(i32::from(v).saturating_add(d).max(0)).unwrap_or(0);
    TilePos::new(add(pos.x, delta.0), add(pos.y, delta.1))
}

impl EditCommand {
    /// Applies the command to `level` (or to the secondary exits beside it).
    pub(crate) fn apply(
        &self,
        level: &mut Level,
        exits: &mut SecondaryExits,
    ) -> Result<(), EditError> {
        match self {
            Self::SetSecondaryExit { id, exit } => {
                exits.insert(*id, *exit);
                Ok(())
            }
            Self::MoveObjects { layer, indices, delta } => {
                move_objects(level, *layer, indices, *delta)
            }
            Self::MoveSprites { indices, delta } => {
                let vertical = level.vertical;
                for &index in indices {
                    let sprite = level
                        .sprites
                        .sprites
                        .get(index)
                        .copied()
                        .ok_or(EditError::NoSuchSprite { index })?;
                    level.move_sprite(index, shifted(sprite.position(vertical), *delta))?;
                }
                Ok(())
            }
            Self::DeleteObjects { layer, indices } => delete_objects(level, *layer, indices),
            Self::DeleteSprites { indices } => {
                let mut sorted = indices.clone();
                sorted.sort_unstable();
                for index in sorted.into_iter().rev() {
                    level.remove_sprite(index)?;
                }
                Ok(())
            }
            Self::InsertObject { layer, spec } => {
                level.insert_object(*layer, usize::MAX, *spec).map(|_| ())
            }
            Self::InsertSprite { spec } => level.insert_sprite(usize::MAX, *spec).map(|_| ()),
            Self::ReplaceObject { layer, index, object } => {
                level.replace_object(*layer, *index, *object)
            }
            Self::ReplaceSprite { index, sprite } => level.set_sprite_bytes(*index, sprite.bytes),
            Self::SetHeaderField { field, value } => level.set_header_field(*field, *value),
            Self::SetSecondaryField { field, value } => level.set_secondary_field(*field, *value),
            Self::SetScreens { screens } => level.set_screens(*screens),
            Self::SetScreenExit { screen, info } => {
                match info {
                    Some(info) => level.set_screen_exit(*screen, *info),
                    None => level.remove_screen_exit(*screen),
                }
                Ok(())
            }
            Self::PasteObjects { layer, objects, at } => {
                for (object, offset) in objects {
                    let pos =
                        TilePos::new(at.x.saturating_add(offset.x), at.y.saturating_add(offset.y));
                    let spec = ObjectSpec { number: object.number(), third: object.third(), pos };
                    level.insert_object(*layer, usize::MAX, spec)?;
                }
                Ok(())
            }
            Self::PasteSprites { sprites, at } => {
                for (sprite, offset) in sprites {
                    let pos =
                        TilePos::new(at.x.saturating_add(offset.x), at.y.saturating_add(offset.y));
                    let spec = SpriteSpec {
                        number: sprite.number(),
                        extra_bits: sprite.extra_bits(),
                        pos,
                    };
                    level.insert_sprite(usize::MAX, spec)?;
                }
                Ok(())
            }
        }
    }

    /// Merges `next` into this command when both are steps of one drag.
    pub(crate) fn coalesce(&mut self, next: &Self) -> bool {
        match (self, next) {
            (
                Self::MoveObjects { layer, indices, delta },
                Self::MoveObjects { layer: l2, indices: i2, delta: d2 },
            ) if layer == l2 && indices == i2 => {
                *delta = (delta.0.saturating_add(d2.0), delta.1.saturating_add(d2.1));
                true
            }
            (
                Self::MoveSprites { indices, delta },
                Self::MoveSprites { indices: i2, delta: d2 },
            ) if indices == i2 => {
                *delta = (delta.0.saturating_add(d2.0), delta.1.saturating_add(d2.1));
                true
            }
            _ => false,
        }
    }

    /// A short label for menus.
    pub(crate) const fn label(&self) -> &'static str {
        match self {
            Self::MoveObjects { .. } => "move objects",
            Self::MoveSprites { .. } => "move sprites",
            Self::DeleteObjects { .. } => "delete objects",
            Self::DeleteSprites { .. } => "delete sprites",
            Self::InsertObject { .. } => "insert object",
            Self::InsertSprite { .. } => "insert sprite",
            Self::ReplaceObject { .. } => "edit object",
            Self::ReplaceSprite { .. } => "edit sprite",
            Self::SetHeaderField { .. } | Self::SetSecondaryField { .. } => "edit header",
            Self::SetScreens { .. } => "set screens",
            Self::SetScreenExit { .. } => "edit screen exit",
            Self::SetSecondaryExit { .. } => "edit secondary exit",
            Self::PasteObjects { .. } | Self::PasteSprites { .. } => "paste",
        }
    }
}

/// Stream indices shift when screen jumps are regenerated, so multi-object
/// edits address objects by their rank among non-jump objects.
fn ranks(level: &Level, layer: Layer, indices: &[usize]) -> Result<Vec<usize>, EditError> {
    let stream = level.stream(layer).ok_or(EditError::Layer2IsBackground)?;
    indices
        .iter()
        .map(|&index| stream.rank_of(index).ok_or(EditError::NoSuchObject { layer, index }))
        .collect()
}

fn move_objects(
    level: &mut Level,
    layer: Layer,
    indices: &[usize],
    delta: (i32, i32),
) -> Result<(), EditError> {
    let vertical = level.vertical;
    for rank in ranks(level, layer, indices)? {
        let stream = level.stream(layer).ok_or(EditError::Layer2IsBackground)?;
        let index =
            stream.index_of_rank(rank).ok_or(EditError::NoSuchObject { layer, index: rank })?;
        let Some(pos) = stream.objects.get(index).and_then(|o| o.position(vertical)) else {
            continue;
        };
        level.move_object(layer, index, shifted(pos, delta))?;
    }
    Ok(())
}

fn delete_objects(level: &mut Level, layer: Layer, indices: &[usize]) -> Result<(), EditError> {
    let mut ranks = ranks(level, layer, indices)?;
    ranks.sort_unstable();
    for rank in ranks.into_iter().rev() {
        let stream = level.stream(layer).ok_or(EditError::Layer2IsBackground)?;
        let index =
            stream.index_of_rank(rank).ok_or(EditError::NoSuchObject { layer, index: rank })?;
        level.remove_object(layer, index)?;
    }
    Ok(())
}
