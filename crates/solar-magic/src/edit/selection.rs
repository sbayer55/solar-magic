//! What is selected, and how clicks and rubber bands find things.

use std::collections::BTreeSet;

use smw_data::level::{Layer, TilePos};
use smw_render::ownership::TileRect;
use smw_render::scene::LevelScene;
use smw_render::sprites::sprite_tile_position;

/// Selected objects (of one layer) and sprites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Selection {
    /// Layer the object indices refer to.
    pub(crate) layer: Layer,
    /// Selected object stream indices.
    pub(crate) objects: BTreeSet<usize>,
    /// Selected sprite indices.
    pub(crate) sprites: BTreeSet<usize>,
}

impl Default for Selection {
    fn default() -> Self {
        Self { layer: Layer::One, objects: BTreeSet::new(), sprites: BTreeSet::new() }
    }
}

impl Selection {
    /// Nothing selected.
    pub(crate) fn is_empty(&self) -> bool {
        self.objects.is_empty() && self.sprites.is_empty()
    }

    /// Deselects everything.
    pub(crate) fn clear(&mut self) {
        self.objects.clear();
        self.sprites.clear();
    }

    /// Whether `hit` is already selected.
    pub(crate) fn contains(&self, hit: Hit) -> bool {
        match hit {
            Hit::Object(i) => self.objects.contains(&i),
            Hit::Sprite(i) => self.sprites.contains(&i),
            Hit::Nothing => false,
        }
    }

    /// Selects `hit`, either replacing the selection or toggling membership.
    pub(crate) fn click(&mut self, hit: Hit, toggle: bool) {
        if !toggle {
            self.clear();
        }
        match hit {
            Hit::Object(i) => {
                if toggle && !self.objects.insert(i) {
                    self.objects.remove(&i);
                } else {
                    self.objects.insert(i);
                }
            }
            Hit::Sprite(i) => {
                if toggle && !self.sprites.insert(i) {
                    self.sprites.remove(&i);
                } else {
                    self.sprites.insert(i);
                }
            }
            Hit::Nothing => {}
        }
    }

    /// Indices in a stable order.
    pub(crate) fn object_indices(&self) -> Vec<usize> {
        self.objects.iter().copied().collect()
    }

    /// Indices in a stable order.
    pub(crate) fn sprite_indices(&self) -> Vec<usize> {
        self.sprites.iter().copied().collect()
    }
}

/// What lies under a tile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    /// An object (stream index on the active layer).
    Object(usize),
    /// A sprite (list index).
    Sprite(usize),
    /// Nothing selectable.
    Nothing,
}

/// Finds the sprite marker or, failing that, the owning object at `tile`.
pub(crate) fn hit_test(
    scene: &LevelScene,
    layer: Layer,
    tile: (usize, usize),
    sprites_visible: bool,
) -> Hit {
    let vertical = scene.loaded.vertical;
    if sprites_visible {
        let found = scene.loaded.level.sprites.sprites.iter().enumerate().rev().find(|(_, s)| {
            let (x, y) = sprite_tile_position(**s, vertical);
            usize::try_from(x).is_ok_and(|x| x == tile.0)
                && usize::try_from(y).is_ok_and(|y| y == tile.1)
        });
        if let Some((index, _)) = found {
            return Hit::Sprite(index);
        }
    }
    scene
        .loaded
        .ownership
        .owner_at(render_layer(layer), tile.0, tile.1)
        .map_or(Hit::Nothing, |i| Hit::Object(usize::from(i)))
}

/// Everything whose visible bounds intersect `rect`.
pub(crate) fn band_select(
    scene: &LevelScene,
    layer: Layer,
    rect: TileRect,
    sprites_visible: bool,
) -> Selection {
    let mut selection = Selection { layer, ..Selection::default() };
    for (index, bounds) in scene.loaded.ownership.bounds(render_layer(layer)).iter().enumerate() {
        if bounds.is_some_and(|b| b.intersects(rect)) {
            selection.objects.insert(index);
        }
    }
    if sprites_visible {
        let vertical = scene.loaded.vertical;
        for (index, sprite) in scene.loaded.level.sprites.sprites.iter().enumerate() {
            let (x, y) = sprite_tile_position(*sprite, vertical);
            let inside = usize::try_from(x)
                .is_ok_and(|x| usize::try_from(y).is_ok_and(|y| rect.contains(x, y)));
            if inside {
                selection.sprites.insert(index);
            }
        }
    }
    selection
}

/// The renderer's layer enum for the data layer.
pub(crate) const fn render_layer(layer: Layer) -> smw_render::level::Layer {
    match layer {
        Layer::One => smw_render::level::Layer::One,
        Layer::Two => smw_render::level::Layer::Two,
    }
}

/// The tile rectangle spanned by two corners (inclusive).
pub(crate) fn rect_between(a: TilePos, b: TilePos) -> TileRect {
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    TileRect {
        x: usize::from(x0),
        y: usize::from(y0),
        w: usize::from(x1.saturating_sub(x0)).saturating_add(1),
        h: usize::from(y1.saturating_sub(y0)).saturating_add(1),
    }
}
