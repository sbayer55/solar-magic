//! The open ROM, the level being edited, and the rendered scene.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context as _;
use smw_data::level::{EditError, Level, SaveOptions, SaveReport, save_level};
use smw_data::{LevelId, RomLayout};
use smw_render::Raster;
use smw_render::scene::{LevelScene, RenderOptions, pixel_rect};
use smw_render::scene_patch::{Dirty, dirty_between};
use snes_rom::Rom;

use crate::edit::{EditCommand, EditSession};

/// An open ROM with one level loaded for editing.
pub(crate) struct Document {
    /// Where the ROM came from (and where Save writes).
    pub(crate) path: PathBuf,
    /// The ROM as last saved; edits live in `session` until saved.
    pub(crate) rom: Rom,
    /// Table locations.
    pub(crate) layout: RomLayout,
    /// Selected level.
    pub(crate) level: LevelId,
    /// Editing state, when the level parsed.
    pub(crate) session: Option<EditSession>,
    /// The rendered level, or the error that prevented loading it.
    pub(crate) scene: Result<LevelScene, String>,
    /// Level texture, updated in place when the scene changes.
    pub(crate) texture: Option<egui::TextureHandle>,
    /// The pixels `texture` holds, so an edit can repaint just its tiles.
    raster: Option<Raster>,
    /// What changed in `scene` since `texture` was last drawn.
    dirty: Dirty,
    /// Map16 sheet texture for the level's tileset.
    pub(crate) map16_texture: Option<egui::TextureHandle>,
    /// Options the current texture was rendered with.
    pub(crate) rendered_with: Option<RenderOptions>,
    /// Last message for the status bar.
    pub(crate) status: String,
    /// The level running in the emulator, if started.
    pub(crate) play: Option<crate::play::PlaySession>,
}

impl std::fmt::Debug for Document {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Document")
            .field("path", &self.path)
            .field("level", &self.level)
            .finish_non_exhaustive()
    }
}

impl Document {
    /// Opens a ROM and loads `level`.
    ///
    /// # Errors
    ///
    /// ROM load, detection, or layout failures.
    pub(crate) fn open(path: &Path, level: LevelId) -> anyhow::Result<Self> {
        let rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
        let layout = RomLayout::detect(&rom)?;
        let mut doc = Self {
            path: path.to_owned(),
            rom,
            layout,
            level,
            session: None,
            scene: Err(String::new()),
            texture: None,
            raster: None,
            dirty: Dirty::All,
            map16_texture: None,
            rendered_with: None,
            status: String::new(),
            play: None,
        };
        doc.select(level);
        Ok(doc)
    }

    /// Starts (or restarts) the emulator on the level as currently edited,
    /// entering at `entrance` (and, with `pixel`, standing exactly there).
    pub(crate) fn start_play(
        &mut self,
        ctx: &egui::Context,
        entrance: smw_play::Entrance,
        pixel: Option<(u16, u16)>,
    ) {
        let level = match self.session.as_ref() {
            Some(session) => session.level.clone(),
            None => match Level::load(&self.rom, &self.layout, self.level) {
                Ok(level) => level,
                Err(err) => {
                    self.status = format!("cannot play: {err}");
                    return;
                }
            },
        };
        let mario = smw_play::MarioState { entrance, ..smw_play::MarioState::default() };
        let request = smw_play::LaunchRequest { level, mario, pixel };
        if let Some(play) = self.play.as_mut() {
            play.running = true;
            play.send(crate::play::Command::Relaunch(Box::new(request)));
        } else {
            let srm = Some(self.path.with_extension("srm"));
            self.play =
                Some(crate::play::PlaySession::start(ctx, &self.rom, &self.layout, request, srm));
        }
        self.status = format!("playing level {}", self.level);
    }

    /// Loads another level, discarding any edits.
    pub(crate) fn select(&mut self, level: LevelId) {
        self.level = level;
        self.session = match Level::load(&self.rom, &self.layout, level) {
            Ok(parsed) => Some(EditSession::new(parsed)),
            Err(err) => {
                self.scene = Err(err.to_string());
                None
            }
        };
        self.map16_texture = None;
        self.rebuild_scene();
        self.dirty = Dirty::All;
    }

    /// Re-runs the loader on the edited level, sharing the palette, graphics
    /// and Map16 sets with the previous scene when the headers still match,
    /// and notes which tiles the texture must repaint.
    pub(crate) fn rebuild_scene(&mut self) {
        let Some(session) = &self.session else {
            self.texture = None;
            self.raster = None;
            self.dirty = Dirty::All;
            return;
        };
        let old = self.scene.as_ref().ok();
        let built =
            LevelScene::from_level_reusing(&self.rom, &self.layout, session.level.clone(), old);
        match built {
            Ok(new) => {
                let dirty = match (old, self.rendered_with) {
                    (Some(old), Some(options)) => dirty_between(old, &new, options),
                    _ => Dirty::All,
                };
                if !old.is_some_and(|o| Arc::ptr_eq(&o.assets, &new.assets)) {
                    self.map16_texture = None;
                }
                self.dirty = self.dirty.union(dirty);
                self.scene = Ok(new);
            }
            Err(err) => {
                self.scene = Err(err.to_string());
                self.texture = None;
                self.raster = None;
                self.dirty = Dirty::All;
            }
        }
    }

    /// Whether the session has unsaved edits.
    pub(crate) fn is_dirty(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.history.is_dirty())
    }

    /// Applies a command (optionally merged into history `group`), keeping
    /// the selection pointed at the same objects when stream indices shift.
    pub(crate) fn apply(&mut self, command: EditCommand, group: Option<u64>) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let clears_selection = matches!(
            command,
            EditCommand::DeleteObjects { .. } | EditCommand::DeleteSprites { .. }
        );
        let layer = session.selection.layer;
        let ranks: Vec<usize> = session
            .level
            .stream(layer)
            .map(|s| session.selection.objects.iter().filter_map(|&i| s.rank_of(i)).collect())
            .unwrap_or_default();
        let result = match group {
            Some(g) => session.apply_grouped(command, g),
            None => session.apply(command),
        };
        match result {
            Ok(()) => {
                if clears_selection {
                    session.selection.clear();
                } else if let Some(stream) = session.level.stream(layer) {
                    session.selection.objects =
                        ranks.iter().filter_map(|&r| stream.index_of_rank(r)).collect();
                }
                self.rebuild_scene();
            }
            Err(err) => self.status = describe(err),
        }
    }

    /// Undoes the last command.
    pub(crate) fn undo(&mut self) {
        if let Some(session) = self.session.as_mut()
            && let Some(label) = session.undo()
        {
            session.selection.clear();
            self.status = format!("undid {label}");
            self.rebuild_scene();
        }
    }

    /// Redoes the last undone command.
    pub(crate) fn redo(&mut self) {
        if let Some(session) = self.session.as_mut()
            && let Some(label) = session.redo()
        {
            session.selection.clear();
            self.status = format!("redid {label}");
            self.rebuild_scene();
        }
    }

    /// Whether saving would grow the ROM (the first relocation of a level
    /// expands a vanilla image to 1 MiB).
    pub(crate) fn save_would_expand(&self) -> bool {
        self.rom.len() < SaveOptions::default().expand_to
    }

    /// Writes the edited level and its secondary exits into a copy of the
    /// ROM, saves it to `path`, and makes that the document's ROM.
    pub(crate) fn save_to(&mut self, path: &Path) -> anyhow::Result<Option<SaveReport>> {
        let mut rom = self.rom.clone();
        let report = match &self.session {
            Some(session) => Some(
                save_level(&mut rom, &self.layout, &session.level, &SaveOptions::default())
                    .context("saving level")?,
            ),
            None => None,
        };
        if let Some(session) = &self.session {
            for (id, exit) in &session.exits {
                exit.write(&mut rom, &self.layout, *id).context("writing secondary exit")?;
            }
            rom.fix_checksum();
        }
        rom.save(path).with_context(|| format!("writing {}", path.display()))?;
        self.rom = rom;
        path.clone_into(&mut self.path);
        if let Some(session) = self.session.as_mut() {
            session.history.mark_saved();
            // Refresh the addresses the level was loaded from.
            if let Ok(reloaded) = Level::load(&self.rom, &self.layout, self.level) {
                session.level.addresses = reloaded.addresses;
            }
        }
        Ok(report)
    }

    /// Drops all edits and reloads the level from the ROM.
    pub(crate) fn revert(&mut self) {
        self.select(self.level);
    }

    /// Makes sure the level texture shows the scene with `options`: a full
    /// redraw when the options changed or everything is dirty, otherwise
    /// only the dirty tiles are repainted and uploaded.
    pub(crate) fn ensure_texture(&mut self, ctx: &egui::Context, options: RenderOptions) {
        let Ok(scene) = &self.scene else {
            return;
        };
        let partial = self.rendered_with == Some(options)
            && self.dirty != Dirty::All
            && self.texture.is_some()
            && self.raster.is_some();
        if !partial {
            let raster = scene.render(options);
            let image = color_image(&raster);
            match self.texture.as_mut() {
                Some(texture) if texture.size() == image.size => {
                    texture.set(image, egui::TextureOptions::NEAREST);
                }
                _ => {
                    self.texture =
                        Some(ctx.load_texture("level", image, egui::TextureOptions::NEAREST));
                }
            }
            self.raster = Some(raster);
            self.rendered_with = Some(options);
        } else if let Dirty::Rect(rect) = self.dirty
            && let (Some(raster), Some(texture)) = (self.raster.as_mut(), self.texture.as_mut())
        {
            scene.render_region(raster, options, rect);
            let pixels = pixel_rect(rect);
            let patch = raster.crop(pixels);
            let pos = [to_usize(pixels.x), to_usize(pixels.y)];
            texture.set_partial(pos, color_image(&patch), egui::TextureOptions::NEAREST);
        }
        self.dirty = Dirty::Nothing;
    }

    /// The texture's pixels (what the level view shows).
    #[cfg(test)]
    pub(crate) const fn raster(&self) -> Option<&Raster> {
        self.raster.as_ref()
    }

    /// Makes sure the Map16 sheet texture exists.
    pub(crate) fn ensure_map16_texture(&mut self, ctx: &egui::Context) {
        if self.map16_texture.is_some() {
            return;
        }
        let Ok(scene) = &self.scene else {
            return;
        };
        let assets = &*scene.assets;
        let backdrop = assets.palette.get(0, 0).unwrap_or_default().to_rgba8();
        let mut raster = Raster::filled(16 * 16, 32 * 16, backdrop);
        let tile_ctx = smw_render::TileContext {
            atlas: &assets.atlas,
            palette: &assets.palette,
            opaque_zero: false,
        };
        for (n, tile) in assets.fg_set.tiles().iter().enumerate() {
            let n = u32::try_from(n).unwrap_or(0);
            smw_render::draw_map16_tile(
                &mut raster,
                ((n % 16).saturating_mul(16), (n / 16).saturating_mul(16)),
                tile,
                tile_ctx,
            );
        }
        self.map16_texture = Some(upload(ctx, "map16", &raster));
    }
}

fn describe(err: EditError) -> String {
    format!("cannot apply edit: {err}")
}

/// Uploads a raster as a nearest-filtered texture.
fn upload(ctx: &egui::Context, name: &str, raster: &Raster) -> egui::TextureHandle {
    ctx.load_texture(name, color_image(raster), egui::TextureOptions::NEAREST)
}

/// The raster as an egui image. Both passes run in optimized crates (this
/// one is built at opt-level 0 in the dev profile).
fn color_image(raster: &Raster) -> egui::ColorImage {
    let size = [to_usize(raster.width()), to_usize(raster.height())];
    egui::ColorImage::from_rgba_unmultiplied(size, &raster.to_rgba_bytes())
}

fn to_usize(v: u32) -> usize {
    usize::try_from(v).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use smw_data::LevelId;
    use smw_data::level::Layer;

    use super::Document;
    use crate::edit::EditCommand;

    /// Edit, undo, redo, save to a temp file, reopen: the saved ROM holds the
    /// edited level and renders exactly like the in-app scene.
    #[test]
    fn real_rom_edit_save_reopen() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let id = LevelId::new(0x105).unwrap();
        let mut doc = Document::open(std::path::Path::new(&path), id).unwrap();
        let original = doc.session.as_ref().unwrap().level.layer1_bytes();
        assert!(!doc.is_dirty());

        let first = doc
            .session
            .as_ref()
            .unwrap()
            .level
            .layer1
            .objects
            .iter()
            .position(|o| o.position(false).is_some())
            .unwrap();
        doc.apply(
            EditCommand::MoveObjects { layer: Layer::One, indices: vec![first], delta: (1, 0) },
            None,
        );
        assert!(doc.is_dirty(), "{}", doc.status);
        let edited = doc.session.as_ref().unwrap().level.layer1_bytes();
        assert_ne!(edited, original);
        doc.undo();
        assert_eq!(doc.session.as_ref().unwrap().level.layer1_bytes(), original);
        doc.redo();
        assert_eq!(doc.session.as_ref().unwrap().level.layer1_bytes(), edited);
        let in_app =
            doc.scene.as_ref().unwrap().render(smw_render::scene::RenderOptions::default());

        let dir = std::env::temp_dir().join(format!("solar-magic-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("edited.sfc");
        let report = doc.save_to(&out).unwrap().unwrap();
        assert!(report.expanded);
        assert!(!doc.is_dirty());

        let reopened = Document::open(&out, id).unwrap();
        assert_eq!(reopened.session.as_ref().unwrap().level.layer1_bytes(), edited);
        let rendered =
            reopened.scene.as_ref().unwrap().render(smw_render::scene::RenderOptions::default());
        assert_eq!(rendered, in_app);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Dragging objects and sprites keeps the scene assets and the texture,
    /// repaints only the tiles that changed, and the picture stays exact.
    #[test]
    fn real_rom_drag_updates_the_texture_in_place() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let ctx = egui::Context::default();
        // A headless context assumes a small GPU; the level is 5120 px wide.
        let raw = egui::RawInput { max_texture_side: Some(1 << 14), ..egui::RawInput::default() };
        ctx.run_ui(raw, |_| {}).textures_delta.clear();
        let options = crate::app::ViewState::default().options;
        let id = LevelId::new(0x105).unwrap();
        let mut doc = Document::open(std::path::Path::new(&path), id).unwrap();
        doc.ensure_texture(&ctx, options);
        let texture = doc.texture.as_ref().unwrap().id();
        let assets = std::sync::Arc::clone(&doc.scene.as_ref().unwrap().assets);
        let session = doc.session.as_ref().unwrap();
        let object =
            session.level.layer1.objects.iter().position(|o| o.position(false).is_some()).unwrap();
        let steps = 20;
        let started = std::time::Instant::now();
        let mut painting = std::time::Duration::ZERO;
        for step in 0..steps {
            let delta = if step % 2 == 0 { (1, 0) } else { (0, 1) };
            doc.apply(
                EditCommand::MoveObjects { layer: Layer::One, indices: vec![object], delta },
                Some(1),
            );
            doc.apply(EditCommand::MoveSprites { indices: vec![0], delta }, Some(1));
            let paint = std::time::Instant::now();
            doc.ensure_texture(&ctx, options);
            painting += paint.elapsed();
        }
        let per_step = started.elapsed() / steps;
        let bounds = doc.scene.as_ref().unwrap().loaded.ownership.bounds1[object].unwrap();
        eprintln!(
            "drag step (object {bounds:?} + sprite): {per_step:?}, of which repainting {:?}",
            painting / steps
        );
        assert!(doc.is_dirty(), "{}", doc.status);
        let scene = doc.scene.as_ref().unwrap();
        assert!(std::sync::Arc::ptr_eq(&assets, &scene.assets), "assets were rebuilt");
        assert_eq!(doc.texture.as_ref().unwrap().id(), texture, "texture was recreated");
        assert_eq!(doc.raster().unwrap(), &scene.render(options), "patched picture drifted");
        assert!(per_step < std::time::Duration::from_millis(250), "step took {per_step:?}");

        doc.undo();
        doc.ensure_texture(&ctx, options);
        let undone = doc.scene.as_ref().unwrap();
        assert_eq!(doc.raster().unwrap(), &undone.render(options), "undo left a stale picture");
        assert_eq!(doc.texture.as_ref().unwrap().id(), texture);
        // Take the texture uploads (and the frees from dropping the document)
        // off the context; nothing applies them here.
        drop(doc);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
    }
}
