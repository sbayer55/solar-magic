//! Editing state: the level being edited, its history, selection and tools.

mod clipboard;
mod commands;
mod history;
mod selection;
mod tools;

pub(crate) use clipboard::Clipboard;
pub(crate) use commands::EditCommand;
pub(crate) use history::History;
pub(crate) use selection::{Hit, Selection, band_select, hit_test, rect_between, render_layer};
pub(crate) use tools::{Drag, Tool};

use smw_data::level::{EditError, Level};

/// Everything that changes while a level is being edited.
#[derive(Debug)]
pub(crate) struct EditSession {
    /// The level as currently edited.
    pub(crate) level: Level,
    /// Undo/redo stacks.
    pub(crate) history: History,
    /// Selected objects and sprites.
    pub(crate) selection: Selection,
    /// The active mouse tool.
    pub(crate) tool: Tool,
    /// An in-progress drag.
    pub(crate) drag: Drag,
    /// Copied items.
    pub(crate) clipboard: Clipboard,
}

impl EditSession {
    /// Starts a session on a freshly loaded level.
    pub(crate) fn new(level: Level) -> Self {
        Self {
            level,
            history: History::default(),
            selection: Selection::default(),
            tool: Tool::Select,
            drag: Drag::None,
            clipboard: Clipboard::default(),
        }
    }

    /// Applies a command, recording it for undo.
    pub(crate) fn apply(&mut self, command: EditCommand) -> Result<(), EditError> {
        self.history.apply(&mut self.level, command, None)
    }

    /// Applies a command that merges with the previous one of the same
    /// `group` (used for the steps of one drag).
    pub(crate) fn apply_grouped(
        &mut self,
        command: EditCommand,
        group: u64,
    ) -> Result<(), EditError> {
        self.history.apply(&mut self.level, command, Some(group))
    }
}
