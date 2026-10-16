//! The object and sprite picker's state: filters, sort order, thumbnail
//! size, recently used items, and thumbnails drawn for the open level's
//! tileset.

mod catalog;

use std::collections::VecDeque;
use std::sync::Arc;

use smw_data::level::{ObjectFormat, Sprite, TilePos};
use smw_render::Raster;
use smw_render::scene_assets::SceneAssets;
use smw_render::sprites::draw_sprite_markers;
use smw_render::thumbs::object_thumbnails;

use catalog::{CatalogEntry, DEFAULT_SIZE, FALLBACK_SIZE, catalog};
pub(crate) use catalog::{Category, PickItem};

use crate::document::Document;
use crate::edit::Tool;

/// Objects drawn per loader run (one per frame, so the panel stays live).
const BATCH: usize = 16;
/// How many recently used items to remember.
const RECENT_LEN: usize = 32;
/// Thumbnail cell edge, in points.
pub(crate) const THUMB_RANGE: core::ops::RangeInclusive<f32> = 24.0..=96.0;

/// Which kinds of item to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KindFilter {
    /// Objects and sprites.
    All,
    /// Objects only.
    Objects,
    /// Sprites only.
    Sprites,
}

/// List order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortBy {
    /// Category, then number.
    Id,
    /// Display name.
    Name,
    /// Tile area, smallest first.
    Size,
    /// Most recently placed first.
    Recent,
}

impl SortBy {
    /// Every order, for the combo box.
    pub(crate) const ALL: [Self; 4] = [Self::Id, Self::Name, Self::Size, Self::Recent];

    /// Display name.
    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::Id => "ID",
            Self::Name => "Name",
            Self::Size => "Size",
            Self::Recent => "Recently used",
        }
    }
}

/// A drawn item.
#[derive(Clone)]
pub(crate) struct Thumb {
    /// The picture at 1:1 scale.
    pub(crate) texture: egui::TextureHandle,
    /// Size in tiles.
    pub(crate) tiles: (usize, usize),
    /// Tile offset from the placement point to the picture's top-left.
    pub(crate) offset: (i32, i32),
}

/// A catalog entry with its picture.
#[derive(Debug, Clone)]
pub(crate) struct PickerEntry {
    /// What it places.
    pub(crate) item: PickItem,
    /// Display name.
    pub(crate) name: String,
    /// Group.
    pub(crate) category: Category,
    /// Picture.
    pub(crate) thumb: Thumb,
}

impl core::fmt::Debug for Thumb {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Thumb")
            .field("texture", &self.texture.id())
            .field("tiles", &self.tiles)
            .field("offset", &self.offset)
            .finish()
    }
}

impl PickerEntry {
    /// Tile area.
    const fn area(&self) -> usize {
        self.thumb.tiles.0.saturating_mul(self.thumb.tiles.1)
    }
}

/// Thumbnails for one set of level assets, drawn a batch per frame.
#[derive(Debug, Clone, Default)]
struct ThumbCache {
    /// The assets the thumbnails were drawn with.
    assets: Option<Arc<SceneAssets>>,
    /// Object format the catalog was built for.
    format: ObjectFormat,
    /// Finished entries.
    ready: Vec<PickerEntry>,
    /// Objects still to draw.
    pending: VecDeque<CatalogEntry>,
    /// Catalog size, for progress.
    total: usize,
}

/// Everything the picker remembers between frames.
#[derive(Debug, Clone)]
pub(crate) struct PickerState {
    /// Kinds shown.
    pub(crate) kind: KindFilter,
    /// Search text (matches names and ids).
    pub(crate) query: String,
    /// Category shown, or all.
    pub(crate) category: Option<Category>,
    /// List order.
    pub(crate) sort: SortBy,
    /// Thumbnail cell edge in points.
    pub(crate) thumb_size: f32,
    /// Recently placed items, newest first.
    recent: Vec<PickItem>,
    cache: ThumbCache,
}

impl Default for PickerState {
    fn default() -> Self {
        Self {
            kind: KindFilter::All,
            query: String::new(),
            category: None,
            sort: SortBy::Id,
            thumb_size: 48.0,
            recent: Vec::new(),
            cache: ThumbCache::default(),
        }
    }
}

impl PickerState {
    /// Starts over when the level's tileset or palette changed, then draws
    /// the next batch of objects. Returns whether more remain.
    pub(crate) fn pump(&mut self, ctx: &egui::Context, doc: &Document) -> bool {
        let (Ok(scene), Some(session)) = (&doc.scene, &doc.session) else {
            return false;
        };
        let format = session.level.format;
        let same = self.cache.assets.as_ref().is_some_and(|a| Arc::ptr_eq(a, &scene.assets));
        if !same || self.cache.format != format {
            self.cache = ThumbCache::start(ctx, Arc::clone(&scene.assets), format);
        }
        let take = self.cache.pending.len().min(BATCH);
        let batch: Vec<CatalogEntry> = self.cache.pending.drain(..take).collect();
        if batch.is_empty() {
            return false;
        }
        let requests: Vec<(u8, u8)> = batch.iter().filter_map(|e| object_request(e.item)).collect();
        let thumbs = object_thumbnails(&doc.rom, &doc.layout, scene, &requests);
        for (entry, thumb) in batch.into_iter().zip(thumbs) {
            self.cache.finish(ctx, entry, thumb);
        }
        !self.cache.pending.is_empty()
    }

    /// `(drawn, total)` for the progress line.
    pub(crate) fn progress(&self) -> (usize, usize) {
        let total = self.cache.total;
        (total.saturating_sub(self.cache.pending.len()), total)
    }

    /// The entries passing the filters, in the chosen order.
    pub(crate) fn visible(&self) -> Vec<&PickerEntry> {
        let query = self.query.trim().to_lowercase();
        let mut list: Vec<&PickerEntry> = self
            .cache
            .ready
            .iter()
            .filter(|e| match self.kind {
                KindFilter::All => true,
                KindFilter::Objects => e.category.is_object(),
                KindFilter::Sprites => !e.category.is_object(),
            })
            .filter(|e| self.category.is_none_or(|c| c == e.category))
            .filter(|e| query.is_empty() || matches_query(e, &query))
            .collect();
        match self.sort {
            SortBy::Id => list.sort_by_key(|e| (e.category, item_number(e.item))),
            SortBy::Name => list.sort_by(|a, b| a.name.cmp(&b.name)),
            SortBy::Size => list.sort_by_key(|e| (e.area(), e.category, item_number(e.item))),
            SortBy::Recent => list.sort_by_key(|e| {
                let rank = self.recent.iter().position(|r| *r == e.item);
                (rank.unwrap_or(usize::MAX), e.category, item_number(e.item))
            }),
        }
        list
    }

    /// The entry `tool` places, if it has a picture of exactly that item.
    pub(crate) fn entry_for(&self, tool: Tool) -> Option<&PickerEntry> {
        self.cache.ready.iter().find(|e| e.item.tool() == tool_without_extras(tool))
    }

    /// Records that `item` was placed.
    pub(crate) fn note_used(&mut self, item: PickItem) {
        self.recent.retain(|r| *r != item);
        self.recent.insert(0, item);
        self.recent.truncate(RECENT_LEN);
    }

    /// Records a placement made with `tool`.
    pub(crate) fn note_tool_used(&mut self, tool: Tool) {
        let item = self.cache.ready.iter().map(|e| e.item).find(|i| i.matches(tool));
        if let Some(item) = item {
            self.note_used(item);
        }
    }
}

impl ThumbCache {
    /// A fresh cache: sprites drawn at once, objects queued.
    fn start(ctx: &egui::Context, assets: Arc<SceneAssets>, format: ObjectFormat) -> Self {
        let entries = catalog(format);
        let total = entries.len();
        let (sprites, objects): (Vec<_>, Vec<_>) =
            entries.into_iter().partition(|e| !e.category.is_object());
        let ready = sprites
            .into_iter()
            .filter_map(|e| {
                let PickItem::Sprite { number } = e.item else {
                    return None;
                };
                let thumb = sprite_thumb(ctx, number)?;
                Some(PickerEntry { item: e.item, name: e.name, category: e.category, thumb })
            })
            .collect();
        Self { assets: Some(assets), format, ready, pending: objects.into(), total }
    }

    /// Files a drawn object, or queues it again at the fallback size when it
    /// was too big at the default one.
    fn finish(
        &mut self,
        ctx: &egui::Context,
        entry: CatalogEntry,
        drawn: Option<smw_render::thumbs::Thumbnail>,
    ) {
        match (drawn, entry.item) {
            (Some(t), item) => {
                let texture = texture(ctx, &item.id_label(), &t.raster);
                let thumb = Thumb { texture, tiles: t.tiles, offset: t.offset };
                let CatalogEntry { name, category, .. } = entry;
                self.ready.push(PickerEntry { item, name, category, thumb });
            }
            (None, PickItem::Object { number, third: DEFAULT_SIZE }) if number != 0 => {
                let item = PickItem::Object { number, third: FALLBACK_SIZE };
                self.pending.push_back(CatalogEntry { item, ..entry });
                self.total = self.total.saturating_add(1);
            }
            (None, _) => {}
        }
    }
}

/// The `(number, third)` the thumbnail renderer takes.
const fn object_request(item: PickItem) -> Option<(u8, u8)> {
    match item {
        PickItem::Object { number, third } => Some((number, third)),
        PickItem::Sprite { .. } => None,
    }
}

/// Sort key within a category.
const fn item_number(item: PickItem) -> u8 {
    match item {
        PickItem::Object { number: 0, third } => third,
        PickItem::Object { number, .. } | PickItem::Sprite { number } => number,
    }
}

/// `tool` with sprite extra bits cleared, for exact lookups.
const fn tool_without_extras(tool: Tool) -> Tool {
    match tool {
        Tool::PlaceSprite { number, .. } => Tool::PlaceSprite { number, extra_bits: 0 },
        Tool::Select | Tool::PlaceObject { .. } => tool,
    }
}

fn matches_query(entry: &PickerEntry, query: &str) -> bool {
    entry.name.to_lowercase().contains(query)
        || entry.item.id_label().to_lowercase().contains(query)
}

/// The level view's sprite marker, as a picture.
fn sprite_thumb(ctx: &egui::Context, number: u8) -> Option<Thumb> {
    let sprite = Sprite::at(number, 0, TilePos::new(0, 0), false)?;
    let mut raster = Raster::new(16, 16);
    draw_sprite_markers(&mut raster, &[sprite], false);
    let texture = texture(ctx, &format!("spr {number:02X}"), &raster);
    Some(Thumb { texture, tiles: (1, 1), offset: (0, 0) })
}

/// Uploads a raster as a pixel-art texture.
fn texture(ctx: &egui::Context, name: &str, raster: &Raster) -> egui::TextureHandle {
    let size = [
        usize::try_from(raster.width()).unwrap_or(0),
        usize::try_from(raster.height()).unwrap_or(0),
    ];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, &raster.to_rgba_bytes());
    ctx.load_texture(format!("picker {name}"), image, egui::TextureOptions::NEAREST)
}
