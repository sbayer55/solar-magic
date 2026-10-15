//! Redrawing only what an edit changed: the difference between two scenes
//! of the same level as a tile rectangle, which [`LevelScene::render_region`]
//! then repaints into an existing raster.

use std::sync::Arc;

use smw_data::level::Sprite;

use crate::level::TileGrid;
use crate::ownership::TileRect;
use crate::scene::{LevelScene, RenderOptions};
use crate::sprites::sprite_tile_position;

/// How much of a picture must be redrawn after its scene changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dirty {
    /// The picture is still exact.
    Nothing,
    /// Only this tile rectangle changed.
    Rect(TileRect),
    /// Start over.
    All,
}

impl Dirty {
    /// Everything either side marks dirty.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        match (self, other) {
            (Self::All, _) | (_, Self::All) => Self::All,
            (Self::Nothing, d) | (d, Self::Nothing) => d,
            (Self::Rect(a), Self::Rect(b)) => Self::Rect(union_rects(a, b)),
        }
    }

    /// Restricts a rectangle to a `width × height` grid; an empty result is
    /// [`Dirty::Nothing`].
    #[must_use]
    pub fn clipped(self, width: usize, height: usize) -> Self {
        let Self::Rect(r) = self else {
            return self;
        };
        let x1 = r.x.saturating_add(r.w).min(width);
        let y1 = r.y.saturating_add(r.h).min(height);
        if r.x >= x1 || r.y >= y1 {
            return Self::Nothing;
        }
        Self::Rect(TileRect {
            x: r.x,
            y: r.y,
            w: x1.saturating_sub(r.x),
            h: y1.saturating_sub(r.y),
        })
    }
}

/// The smallest rectangle containing both.
#[must_use]
pub fn union_rects(a: TileRect, b: TileRect) -> TileRect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let x1 = a.x.saturating_add(a.w).max(b.x.saturating_add(b.w));
    let y1 = a.y.saturating_add(a.h).max(b.y.saturating_add(b.h));
    TileRect { x, y, w: x1.saturating_sub(x), h: y1.saturating_sub(y) }
}

/// What a picture of `old` drawn with `options` must redraw to show `new`.
///
/// Anything but a change confined to tiles and sprite markers (different
/// assets, a different level shape, a background edit, or a changed
/// ownership while tinting by owner) means [`Dirty::All`].
#[must_use]
pub fn dirty_between(old: &LevelScene, new: &LevelScene, options: RenderOptions) -> Dirty {
    if !Arc::ptr_eq(&old.assets, &new.assets) || old.ext_tiles != new.ext_tiles {
        return Dirty::All;
    }
    let (a, b) = (&old.loaded, &new.loaded);
    let shape = |l: &crate::level::LoadedLevel| {
        (l.vertical, l.layer1.width(), l.layer1.height(), l.background_rows)
    };
    if shape(a) != shape(b) || a.background != b.background {
        return Dirty::All;
    }
    if options.owners && a.ownership != b.ownership {
        return Dirty::All;
    }
    let mut dirty = Dirty::Nothing;
    if options.layer1 {
        dirty = dirty.union(grid_diff(&a.layer1, &b.layer1));
    }
    if options.layer2 {
        dirty = dirty.union(match (a.layer2.as_ref(), b.layer2.as_ref()) {
            (Some(x), Some(y)) => grid_diff(x, y),
            (None, None) => Dirty::Nothing,
            (Some(_), None) | (None, Some(_)) => Dirty::All,
        });
    }
    if options.sprites {
        dirty = dirty.union(sprite_diff(
            &a.level.sprites.sprites,
            &b.level.sprites.sprites,
            a.vertical,
        ));
    }
    dirty.clipped(a.layer1.width(), a.layer1.height())
}

/// Bounding box of the cells that differ between two grids of one shape.
fn grid_diff(a: &TileGrid, b: &TileGrid) -> Dirty {
    if (a.width(), a.height()) != (b.width(), b.height()) {
        return Dirty::All;
    }
    let width = a.width().max(1);
    let mut dirty = Dirty::Nothing;
    for (i, (x, y)) in a.tiles().iter().zip(b.tiles()).enumerate() {
        if x != y {
            let cell = TileRect {
                x: i.checked_rem(width).unwrap_or(0),
                y: i.checked_div(width).unwrap_or(0),
                w: 1,
                h: 1,
            };
            dirty = dirty.union(Dirty::Rect(cell));
        }
    }
    dirty
}

/// The old and new marker cells of every sprite entry that differs.
fn sprite_diff(a: &[Sprite], b: &[Sprite], vertical: bool) -> Dirty {
    let mut dirty = Dirty::Nothing;
    for i in 0..a.len().max(b.len()) {
        let (x, y) = (a.get(i), b.get(i));
        if x == y {
            continue;
        }
        for sprite in [x, y].into_iter().flatten() {
            dirty = dirty.union(Dirty::Rect(marker_cell(*sprite, vertical)));
        }
    }
    dirty
}

/// The tile a sprite's marker occupies.
pub(crate) fn marker_cell(sprite: Sprite, vertical: bool) -> TileRect {
    let (x, y) = sprite_tile_position(sprite, vertical);
    let to = |v: u32| usize::try_from(v).unwrap_or(usize::MAX);
    TileRect { x: to(x), y: to(y), w: 1, h: 1 }
}

#[cfg(test)]
mod tests {
    use super::{Dirty, TileRect, union_rects};

    #[test]
    fn unions_and_clips() {
        let a = TileRect { x: 2, y: 3, w: 1, h: 1 };
        let b = TileRect { x: 5, y: 1, w: 2, h: 1 };
        assert_eq!(union_rects(a, b), TileRect { x: 2, y: 1, w: 5, h: 3 });
        assert_eq!(Dirty::Nothing.union(Dirty::Rect(a)), Dirty::Rect(a));
        assert_eq!(Dirty::Rect(a).union(Dirty::All), Dirty::All);
        assert_eq!(
            Dirty::Rect(TileRect { x: 30, y: 0, w: 4, h: 2 }).clipped(32, 27),
            Dirty::Rect(TileRect { x: 30, y: 0, w: 2, h: 2 })
        );
        assert_eq!(
            Dirty::Rect(TileRect { x: 40, y: 0, w: 1, h: 1 }).clipped(32, 27),
            Dirty::Nothing
        );
        assert_eq!(Dirty::All.clipped(1, 1), Dirty::All);
    }
}
