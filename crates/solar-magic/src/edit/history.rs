//! Memento-based undo/redo: every entry stores the level (and the secondary
//! exits edited with it) before its command.

use smw_data::level::{EditError, Level};

use super::EditCommand;
use super::commands::SecondaryExits;

#[derive(Debug)]
struct Entry {
    command: EditCommand,
    /// The state to restore when this entry is undone (or redone).
    snapshot: Snapshot,
    group: Option<u64>,
}

/// Everything a command may change.
#[derive(Debug)]
struct Snapshot {
    level: Level,
    exits: SecondaryExits,
}

impl Snapshot {
    /// Swaps the live state with this snapshot, returning the state that was live.
    fn swap_into(self, level: &mut Level, exits: &mut SecondaryExits) -> Self {
        Self {
            level: core::mem::replace(level, self.level),
            exits: core::mem::replace(exits, self.exits),
        }
    }
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
    /// Applies `command` to `level` and `exits` and records it. Commands in
    /// the same `group` as the previous entry are merged into it.
    pub(crate) fn apply(
        &mut self,
        level: &mut Level,
        exits: &mut SecondaryExits,
        command: EditCommand,
        group: Option<u64>,
    ) -> Result<(), EditError> {
        let snapshot = Snapshot { level: level.clone(), exits: exits.clone() };
        command.apply(level, exits)?;
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
    pub(crate) fn undo(
        &mut self,
        level: &mut Level,
        exits: &mut SecondaryExits,
    ) -> Option<&'static str> {
        let entry = self.undo.pop()?;
        let current = entry.snapshot.swap_into(level, exits);
        let label = entry.command.label();
        self.redo.push(Entry { command: entry.command, snapshot: current, group: None });
        Some(label)
    }

    /// Re-applies the last undone command.
    pub(crate) fn redo(
        &mut self,
        level: &mut Level,
        exits: &mut SecondaryExits,
    ) -> Option<&'static str> {
        let entry = self.redo.pop()?;
        let current = entry.snapshot.swap_into(level, exits);
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
        ExitId, Layer2, Level, LevelAddresses, ObjectStream, PrimaryHeader, SecondaryExit,
        SecondaryHeader, SpriteList,
    };
    use snes_rom::SnesAddr;

    use super::{EditCommand, History, SecondaryExits};

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
            format: smw_data::level::ObjectFormat::Vanilla,
            layer1: ObjectStream::default(),
            layer2: Layer2::Objects { header_copy: [0; 5], stream: ObjectStream::default() },
            sprites: SpriteList::default(),
            lm: None,
        }
    }

    #[test]
    fn undo_restores_bytes_and_tracks_dirtiness() {
        let mut lv = level();
        let original = lv.layer1_bytes();
        let mut ex = SecondaryExits::new();
        let mut h = History::default();
        assert!(!h.is_dirty());
        h.apply(
            &mut lv,
            &mut ex,
            EditCommand::SetHeaderField { field: primary::MUSIC, value: 3 },
            None,
        )
        .unwrap();
        assert_eq!(lv.header.music(), 3);
        assert!(h.is_dirty());
        h.mark_saved();
        assert!(!h.is_dirty());
        assert_eq!(h.undo(&mut lv, &mut ex), Some("edit header"));
        assert_eq!(lv.layer1_bytes(), original);
        assert!(h.is_dirty());
        assert_eq!(h.redo(&mut lv, &mut ex), Some("edit header"));
        assert_eq!(lv.header.music(), 3);
        assert!(!h.is_dirty());
        assert_eq!(h.undo(&mut lv, &mut ex), Some("edit header"));
        // A new edit after undoing below the saved point can never be "saved".
        h.apply(
            &mut lv,
            &mut ex,
            EditCommand::SetHeaderField { field: primary::MUSIC, value: 1 },
            None,
        )
        .unwrap();
        assert!(h.is_dirty());
        assert_eq!(h.redo(&mut lv, &mut ex), None);
    }

    #[test]
    fn secondary_exits_undo_with_the_level() {
        let mut lv = level();
        let mut ex = SecondaryExits::new();
        let mut h = History::default();
        let id = ExitId::new(0x12).unwrap();
        let exit = SecondaryExit { raw: [0x05, 0, 0, 0] };
        h.apply(&mut lv, &mut ex, EditCommand::SetSecondaryExit { id, exit }, None).unwrap();
        assert_eq!(ex.get(&id), Some(&exit));
        assert!(h.is_dirty());
        assert_eq!(h.undo(&mut lv, &mut ex), Some("edit secondary exit"));
        assert!(ex.is_empty());
        assert_eq!(h.redo(&mut lv, &mut ex), Some("edit secondary exit"));
        assert_eq!(ex.get(&id), Some(&exit));
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
        let mut ex = SecondaryExits::new();
        h.apply(&mut lv, &mut ex, step((1, 0)), Some(7)).unwrap();
        h.apply(&mut lv, &mut ex, step((1, 1)), Some(7)).unwrap();
        h.apply(&mut lv, &mut ex, step((0, 1)), Some(8)).unwrap();
        assert_eq!(lv.layer1.objects[0].position(false), Some(smw_data::level::TilePos::new(4, 4)));
        assert_eq!(h.undo.len(), 2, "the first two steps merged");
        h.undo(&mut lv, &mut ex);
        assert_eq!(lv.layer1.objects[0].position(false), Some(smw_data::level::TilePos::new(4, 3)));
        h.undo(&mut lv, &mut ex);
        assert_eq!(lv.layer1.objects[0].position(false), Some(smw_data::level::TilePos::new(2, 2)));
    }
}
