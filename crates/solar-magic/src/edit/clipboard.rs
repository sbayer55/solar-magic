//! In-memory copy/paste of objects and sprites.

use smw_data::level::{Layer, Level, RawObject, Sprite, TilePos};

use super::{EditCommand, Selection};

/// Copied items, positioned relative to the selection's top-left tile.
#[derive(Debug, Clone, Default)]
pub(crate) struct Clipboard {
    layer: Option<Layer>,
    objects: Vec<(RawObject, TilePos)>,
    sprites: Vec<(Sprite, TilePos)>,
}

impl Clipboard {
    /// Whether there is anything to paste.
    pub(crate) const fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.sprites.is_empty()
    }

    /// Copies the selection out of `level`.
    pub(crate) fn copy(&mut self, level: &Level, selection: &Selection) {
        let vertical = level.vertical;
        let mut objects: Vec<(RawObject, TilePos)> = Vec::new();
        if let Some(stream) = level.stream(selection.layer) {
            for &index in &selection.objects {
                if let Some(object) = stream.objects.get(index)
                    && let Some(pos) = object.position(vertical)
                {
                    objects.push((*object, pos));
                }
            }
        }
        let sprites: Vec<(Sprite, TilePos)> = selection
            .sprites
            .iter()
            .filter_map(|&i| level.sprites.sprites.get(i).map(|s| (*s, s.position(vertical))))
            .collect();
        let positions = objects.iter().map(|(_, p)| *p).chain(sprites.iter().map(|(_, p)| *p));
        let origin = positions.fold(None, |acc: Option<TilePos>, p| {
            Some(acc.map_or(p, |a| TilePos::new(a.x.min(p.x), a.y.min(p.y))))
        });
        let Some(origin) = origin else {
            return;
        };
        let relative =
            |p: TilePos| TilePos::new(p.x.saturating_sub(origin.x), p.y.saturating_sub(origin.y));
        self.layer = Some(selection.layer);
        self.objects = objects.into_iter().map(|(o, p)| (o, relative(p))).collect();
        self.sprites = sprites.into_iter().map(|(s, p)| (s, relative(p))).collect();
    }

    /// The commands that paste the clipboard with its origin at `at`.
    pub(crate) fn paste_commands(&self, at: TilePos) -> Vec<EditCommand> {
        let mut out = Vec::new();
        if let Some(layer) = self.layer
            && !self.objects.is_empty()
        {
            out.push(EditCommand::PasteObjects { layer, objects: self.objects.clone(), at });
        }
        if !self.sprites.is_empty() {
            out.push(EditCommand::PasteSprites { sprites: self.sprites.clone(), at });
        }
        out
    }
}
