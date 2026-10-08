//! The open ROM, the level being edited, and the rendered scene.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use smw_data::level::{
    EditError, ExitId, Level, SaveOptions, SaveReport, SecondaryExit, save_level,
};
use smw_data::{LevelId, RomLayout};
use smw_render::scene::{LevelScene, RenderOptions};
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
    /// Level texture (recreated when the scene or options change).
    pub(crate) texture: Option<egui::TextureHandle>,
    /// Map16 sheet texture for the level's tileset.
    pub(crate) map16_texture: Option<egui::TextureHandle>,
    /// Options the current texture was rendered with.
    pub(crate) rendered_with: Option<RenderOptions>,
    /// Secondary exits edited but not yet written to the ROM.
    pub(crate) pending_exits: BTreeMap<ExitId, SecondaryExit>,
    /// Last message for the status bar.
    pub(crate) status: String,
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
            map16_texture: None,
            rendered_with: None,
            pending_exits: BTreeMap::new(),
            status: String::new(),
        };
        doc.select(level);
        Ok(doc)
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
    }

    /// Re-runs the loader on the edited level and drops stale textures.
    pub(crate) fn rebuild_scene(&mut self) {
        if let Some(session) = &self.session {
            self.scene = LevelScene::from_level(&self.rom, &self.layout, session.level.clone())
                .map_err(|e| e.to_string());
        }
        self.texture = None;
        self.rendered_with = None;
    }

    /// Whether the session has unsaved edits.
    pub(crate) fn is_dirty(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.history.is_dirty())
            || !self.pending_exits.is_empty()
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
                self.map16_texture = None;
                self.rebuild_scene();
            }
            Err(err) => self.status = describe(err),
        }
    }

    /// Undoes the last command.
    pub(crate) fn undo(&mut self) {
        if let Some(session) = self.session.as_mut()
            && let Some(label) = session.history.undo(&mut session.level)
        {
            session.selection.clear();
            self.status = format!("undid {label}");
            self.rebuild_scene();
        }
    }

    /// Redoes the last undone command.
    pub(crate) fn redo(&mut self) {
        if let Some(session) = self.session.as_mut()
            && let Some(label) = session.history.redo(&mut session.level)
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

    /// Writes the edited level (and pending secondary exits) into a copy of
    /// the ROM, saves it to `path`, and makes that the document's ROM.
    pub(crate) fn save_to(&mut self, path: &Path) -> anyhow::Result<Option<SaveReport>> {
        let mut rom = self.rom.clone();
        let report = match &self.session {
            Some(session) => Some(
                save_level(&mut rom, &self.layout, &session.level, &SaveOptions::default())
                    .context("saving level")?,
            ),
            None => None,
        };
        for (id, exit) in &self.pending_exits {
            exit.write(&mut rom, &self.layout, *id).context("writing secondary exit")?;
        }
        if report.is_none() && !self.pending_exits.is_empty() {
            rom.fix_checksum();
        }
        rom.save(path).with_context(|| format!("writing {}", path.display()))?;
        self.rom = rom;
        path.clone_into(&mut self.path);
        self.pending_exits.clear();
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
        self.pending_exits.clear();
        self.select(self.level);
    }

    /// Makes sure the level texture matches `options`.
    pub(crate) fn ensure_texture(&mut self, ctx: &egui::Context, options: RenderOptions) {
        if self.texture.is_some() && self.rendered_with == Some(options) {
            return;
        }
        let Ok(scene) = &self.scene else {
            return;
        };
        let raster = scene.render(options);
        self.texture = Some(upload(ctx, "level", &raster));
        self.rendered_with = Some(options);
    }

    /// Makes sure the Map16 sheet texture exists.
    pub(crate) fn ensure_map16_texture(&mut self, ctx: &egui::Context) {
        if self.map16_texture.is_some() {
            return;
        }
        let Ok(scene) = &self.scene else {
            return;
        };
        let backdrop = scene.palette.get(0, 0).unwrap_or_default().to_rgba8();
        let mut raster = smw_render::Raster::filled(16 * 16, 32 * 16, backdrop);
        let tile_ctx = smw_render::TileContext {
            atlas: &scene.atlas,
            palette: &scene.palette,
            opaque_zero: false,
        };
        for (n, tile) in scene.fg_set.tiles().iter().enumerate() {
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
fn upload(ctx: &egui::Context, name: &str, raster: &smw_render::Raster) -> egui::TextureHandle {
    let size = [
        usize::try_from(raster.width()).unwrap_or(0),
        usize::try_from(raster.height()).unwrap_or(0),
    ];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, &raster.to_rgba_bytes());
    ctx.load_texture(name, image, egui::TextureOptions::NEAREST)
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
}
