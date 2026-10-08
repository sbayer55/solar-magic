//! Memento-based undo/redo: every entry stores the level before its command.

use smw_data::level::{EditError, Level};

use super::EditCommand;

#[derive(Debug)]
struct Entry {
    command: EditCommand,
    /// The level to restore when this entry is undone (or redone).
    snapshot: Level,
    group: Option<u64>,
}

/// Undo and redo stacks plus the saved-state marker.
#[derive(Debug, Default)]
pub(crate) struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    saved_depth: usize,
    lost_saved_point: bool,
}

impl History {
    /// Applies `command` to `level` and records it. Commands in the same
    /// `group` as the previous entry are merged into it.
    pub(crate) fn apply(
        &mut self,
        level: &mut Level,
        command: EditCommand,
        group: Option<u64>,
    ) -> Result<(), EditError> {
        let snapshot = level.clone();
        command.apply(level)?;
        self.redo.clear();
        if self.undo.len() < self.saved_depth {
            self.lost_saved_point = true;
        }
        if let Some(last) = self.undo.last_mut()
            && group.is_some()
            && last.group == group
            && last.command.coalesce(&command)
        {
            return Ok(());
        }
        self.undo.push(Entry { command, snapshot, group });
        Ok(())
    }

    /// Reverts the last command. Returns its label if there was one.
    pub(crate) fn undo(&mut self, level: &mut Level) -> Option<&'static str> {
        let entry = self.undo.pop()?;
        let current = core::mem::replace(level, entry.snapshot);
        let label = entry.command.label();
        self.redo.push(Entry { command: entry.command, snapshot: current, group: None });
        Some(label)
    }

    /// Re-applies the last undone command.
    pub(crate) fn redo(&mut self, level: &mut Level) -> Option<&'static str> {
        let entry = self.redo.pop()?;
        let current = core::mem::replace(level, entry.snapshot);
        let label = entry.command.label();
        self.undo.push(Entry { command: entry.command, snapshot: current, group: None });
        Some(label)
    }

    /// Label of the command `undo` would revert.
    #[allow(clippy::missing_const_for_fn, reason = "Vec access is not const")]
    pub(crate) fn undo_label(&self) -> Option<&'static str> {
        self.undo.last().map(|e| e.command.label())
    }

    /// Label of the command `redo` would re-apply.
    #[allow(clippy::missing_const_for_fn, reason = "Vec access is not const")]
    pub(crate) fn redo_label(&self) -> Option<&'static str> {
        self.redo.last().map(|e| e.command.label())
    }

    /// Marks the current state as saved.
    pub(crate) const fn mark_saved(&mut self) {
        self.saved_depth = self.undo.len();
        self.lost_saved_point = false;
    }

    /// Whether the level differs from the last saved state.
    pub(crate) const fn is_dirty(&self) -> bool {
        self.lost_saved_point || self.undo.len() != self.saved_depth
    }
}

#[cfg(test)]
mod tests {
    use smw_data::LevelId;
    use smw_data::level::fields::primary;
    use smw_data::level::{
        Layer2, Level, LevelAddresses, ObjectStream, PrimaryHeader, SecondaryHeader, SpriteList,
    };
    use snes_rom::SnesAddr;

    use super::{EditCommand, History};

    fn level() -> Level {
        let addr = SnesAddr::lit(0x06_8000);
        Level {
            id: LevelId::new(0x105).unwrap(),
            addresses: LevelAddresses {
                layer1: addr,
                layer2_raw: addr,
                layer2: addr,
                sprites: addr,
            },
            header: PrimaryHeader { raw: [3, 0, 0, 0, 0] },
            secondary: SecondaryHeader { raw: [0; 4] },
            vertical: false,
            layer1: ObjectStream::default(),
            layer2: Layer2::Objects { header_copy: [0; 5], stream: ObjectStream::default() },
            sprites: SpriteList::default(),
        }
    }

    #[test]
    fn undo_restores_bytes_and_tracks_dirtiness() {
        let mut lv = level();
        let original = lv.layer1_bytes();
        let mut h = History::default();
        assert!(!h.is_dirty());
        h.apply(&mut lv, EditCommand::SetHeaderField { field: primary::MUSIC, value: 3 }, None)
            .unwrap();
        assert_eq!(lv.header.music(), 3);
        assert!(h.is_dirty());
        h.mark_saved();
        assert!(!h.is_dirty());
        assert_eq!(h.undo(&mut lv), Some("edit header"));
        assert_eq!(lv.layer1_bytes(), original);
        assert!(h.is_dirty());
        assert_eq!(h.redo(&mut lv), Some("edit header"));
        assert_eq!(lv.header.music(), 3);
        assert!(!h.is_dirty());
        assert_eq!(h.undo(&mut lv), Some("edit header"));
        // A new edit after undoing below the saved point can never be "saved".
        h.apply(&mut lv, EditCommand::SetHeaderField { field: primary::MUSIC, value: 1 }, None)
            .unwrap();
        assert!(h.is_dirty());
        assert_eq!(h.redo(&mut lv), None);
    }

    #[test]
    fn grouped_moves_coalesce() {
        let mut lv = level();
        lv.insert_object(
            smw_data::level::Layer::One,
            0,
            smw_data::level::ObjectSpec {
                number: 1,
                third: 0x11,
                pos: smw_data::level::TilePos::new(2, 2),
            },
        )
        .unwrap();
        let mut h = History::default();
        let step = |d| EditCommand::MoveObjects {
            layer: smw_data::level::Layer::One,
            indices: vec![0],
            delta: d,
        };
        h.apply(&mut lv, step((1, 0)), Some(7)).unwrap();
        h.apply(&mut lv, step((1, 1)), Some(7)).unwrap();
        h.apply(&mut lv, step((0, 1)), Some(8)).unwrap();
        assert_eq!(lv.layer1.objects[0].position(false), Some(smw_data::level::TilePos::new(4, 4)));
        assert_eq!(h.undo.len(), 2, "the first two steps merged");
        h.undo(&mut lv);
        assert_eq!(lv.layer1.objects[0].position(false), Some(smw_data::level::TilePos::new(4, 3)));
        h.undo(&mut lv);
        assert_eq!(lv.layer1.objects[0].position(false), Some(smw_data::level::TilePos::new(2, 2)));
    }
}
