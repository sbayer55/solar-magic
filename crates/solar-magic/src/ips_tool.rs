//! Tools → IPS Patcher: apply and create IPS patches without leaving the app.
//!
//! The window works on raw file bytes (never on the open [`crate::document::Document`]),
//! so copier headers are handled explicitly: the target's header state is
//! shown, and the user can add or strip the header before applying.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context as _;
use snes_rom::copier_header::{self, HeaderMode};

/// Which half of the window is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Apply,
    Create,
}

/// Result of a successful apply or create, for the status area.
struct Outcome {
    summary: String,
    warnings: Vec<String>,
    log: Vec<String>,
}

/// State of the IPS Patcher window.
#[derive(Debug, Default)]
pub(crate) struct IpsTool {
    /// Whether the window is showing.
    pub(crate) open: bool,
    mode: Mode,
    // Apply mode.
    patch: Option<PathBuf>,
    target: Option<PathBuf>,
    target_has_header: Option<bool>,
    output: Option<PathBuf>,
    header_mode: HeaderMode,
    // Create mode.
    original: Option<PathBuf>,
    modified: Option<PathBuf>,
    patch_out: Option<PathBuf>,
    // Results of the last run.
    status: Option<Result<String, String>>,
    warnings: Vec<String>,
    log: Vec<String>,
    /// Patched ROM that is not the open one, offered via a button.
    produced: Option<PathBuf>,
    /// ROM the app should (re)open.
    reload: Option<PathBuf>,
}

impl IpsTool {
    /// Uses the open ROM as the default target and original, if unset.
    pub(crate) fn prefill(&mut self, rom: &Path) {
        if self.target.is_none() {
            self.set_target(rom.to_owned());
        }
        if self.original.is_none() {
            self.original = Some(rom.to_owned());
        }
    }

    /// A ROM the app should open now, if the last run produced one.
    pub(crate) const fn take_reload(&mut self) -> Option<PathBuf> {
        self.reload.take()
    }

    /// Draws the window when open. `current_rom` is the open document's path,
    /// used to reload it when it is overwritten.
    pub(crate) fn show(&mut self, ctx: &egui::Context, current_rom: Option<&Path>) {
        let mut open = self.open;
        egui::Window::new("IPS Patcher").open(&mut open).default_width(560.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.mode, Mode::Apply, "Apply patch");
                ui.selectable_value(&mut self.mode, Mode::Create, "Create patch");
            });
            ui.separator();
            match self.mode {
                Mode::Apply => self.ui_apply(ui, current_rom),
                Mode::Create => self.ui_create(ui),
            }
            self.ui_results(ui);
        });
        self.open = open;
    }

    fn set_target(&mut self, path: PathBuf) {
        self.target_has_header = fs::metadata(&path)
            .ok()
            .and_then(|m| usize::try_from(m.len()).ok())
            .map(copier_header::detect);
        self.target = Some(path);
    }

    fn ui_apply(&mut self, ui: &mut egui::Ui, current_rom: Option<&Path>) {
        egui::Grid::new("ips_apply_paths").num_columns(3).show(ui, |ui| {
            path_row(ui, "Patch", &mut self.patch, || pick_file("IPS patch", &["ips"]));
            let mut target = self.target.clone();
            if path_row(ui, "Target", &mut target, || pick_file("SNES ROM", &["sfc", "smc"]))
                && let Some(path) = target
            {
                self.set_target(path);
            }
            let suggested = self.target.as_deref().map(default_output);
            path_row(ui, "Output", &mut self.output, || {
                save_file(suggested.as_deref(), "SNES ROM", &["sfc", "smc"])
            });
        });
        match self.target_has_header {
            Some(true) => ui.label("Target has a 512-byte copier header; IPS offsets include it."),
            Some(false) => ui.label("Target has no copier header."),
            None => ui.weak("Pick a target to patch."),
        };
        egui::ComboBox::from_label("Target header")
            .selected_text(self.header_mode.label())
            .show_ui(ui, |ui| {
                for mode in HeaderMode::ALL {
                    ui.selectable_value(&mut self.header_mode, mode, mode.label());
                }
            });
        let ready = self.patch.is_some() && self.target.is_some() && self.output.is_some();
        if ui.add_enabled(ready, egui::Button::new("Apply patch")).clicked() {
            self.run_apply(current_rom);
        }
    }

    fn ui_create(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("ips_create_paths").num_columns(3).show(ui, |ui| {
            path_row(ui, "Original", &mut self.original, || pick_file("SNES ROM", &["sfc", "smc"]));
            path_row(ui, "Modified", &mut self.modified, || pick_file("SNES ROM", &["sfc", "smc"]));
            let suggested = self.modified.as_deref().map(|p| p.with_extension("ips"));
            path_row(ui, "Patch", &mut self.patch_out, || {
                save_file(suggested.as_deref(), "IPS patch", &["ips"])
            });
        });
        let ready = self.original.is_some() && self.modified.is_some() && self.patch_out.is_some();
        if ui.add_enabled(ready, egui::Button::new("Create patch")).clicked() {
            self.run_create();
        }
    }

    fn ui_results(&mut self, ui: &mut egui::Ui) {
        if self.status.is_none() {
            return;
        }
        ui.separator();
        for warning in &self.warnings {
            ui.colored_label(egui::Color32::YELLOW, warning);
        }
        match &self.status {
            Some(Ok(summary)) => ui.colored_label(egui::Color32::LIGHT_GREEN, summary),
            Some(Err(error)) => ui.colored_label(egui::Color32::LIGHT_RED, error),
            None => ui.label(""),
        };
        if let Some(path) = self.produced.clone()
            && ui.button("Open patched ROM").clicked()
        {
            self.reload = Some(path);
        }
        if self.log.is_empty() {
            return;
        }
        egui::CollapsingHeader::new(format!("Log ({} records)", self.log.len())).show(ui, |ui| {
            let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
            egui::ScrollArea::vertical().max_height(220.0).show_rows(
                ui,
                row_height,
                self.log.len(),
                |ui, rows| {
                    for line in self.log.get(rows).unwrap_or(&[]) {
                        ui.monospace(line);
                    }
                },
            );
        });
    }

    fn clear_results(&mut self) {
        self.status = None;
        self.warnings.clear();
        self.log.clear();
        self.produced = None;
    }

    fn run_apply(&mut self, current_rom: Option<&Path>) {
        self.clear_results();
        let (Some(patch), Some(target), Some(output)) =
            (self.patch.clone(), self.target.clone(), self.output.clone())
        else {
            return;
        };
        match apply_files(&patch, &target, &output, self.header_mode) {
            Ok(outcome) => {
                self.status = Some(Ok(outcome.summary));
                self.warnings = outcome.warnings;
                self.log = outcome.log;
                if current_rom.is_some_and(|rom| same_file(rom, &output)) {
                    self.reload = Some(output);
                } else {
                    self.produced = Some(output);
                }
            }
            Err(err) => self.status = Some(Err(format!("{err:#}"))),
        }
    }

    fn run_create(&mut self) {
        self.clear_results();
        let (Some(original), Some(modified), Some(out)) =
            (self.original.clone(), self.modified.clone(), self.patch_out.clone())
        else {
            return;
        };
        match create_files(&original, &modified, &out) {
            Ok(outcome) => {
                self.status = Some(Ok(outcome.summary));
                self.warnings = outcome.warnings;
                self.log = outcome.log;
            }
            Err(err) => self.status = Some(Err(format!("{err:#}"))),
        }
    }
}

/// Reads, patches and writes. Returns the status text and the log.
fn apply_files(
    patch: &Path,
    target: &Path,
    output: &Path,
    mode: HeaderMode,
) -> anyhow::Result<Outcome> {
    let patch_bytes = fs::read(patch).with_context(|| format!("reading {}", patch.display()))?;
    let parsed =
        ips::Patch::parse(&patch_bytes).with_context(|| format!("parsing {}", patch.display()))?;
    let raw = fs::read(target).with_context(|| format!("reading {}", target.display()))?;
    let raw_len = raw.len();
    let source = copier_header::apply_mode(raw, mode);
    let applied = ips::apply(&source, &parsed).context("applying patch")?;
    fs::write(output, &applied.data).with_context(|| format!("writing {}", output.display()))?;

    let counts = parsed.summary();
    let report = &applied.report;
    let summary = format!(
        "Applied {} records ({} raw, {} RLE): {}. Wrote {}.",
        counts.records,
        counts.raw_records,
        counts.rle_records,
        report,
        output.display()
    );
    let mut warnings = Vec::new();
    if mode != HeaderMode::Keep && source.len() == raw_len {
        warnings.push(format!(
            "\"{}\" changed nothing: the target was already in that state.",
            mode.label()
        ));
    }
    if copier_header::detect(report.output_len) {
        warnings.push("The patched file has a 512-byte copier header.".to_owned());
    }
    let log = parsed
        .records
        .iter()
        .zip(&report.entries)
        .map(|(record, entry)| {
            let note = if entry.extended { "  (extends file)" } else { "" };
            format!("{:>5}  {record}{note}", entry.index)
        })
        .collect();
    Ok(Outcome { summary, warnings, log })
}

/// Reads both files, builds the patch and writes it.
fn create_files(original: &Path, modified: &Path, out: &Path) -> anyhow::Result<Outcome> {
    let source = fs::read(original).with_context(|| format!("reading {}", original.display()))?;
    let target = fs::read(modified).with_context(|| format!("reading {}", modified.display()))?;
    let mut warnings = Vec::new();
    if copier_header::detect(source.len()) != copier_header::detect(target.len()) {
        warnings.push(
            "Original and modified differ in copier-header state; every offset is shifted by 512 \
             bytes."
                .to_owned(),
        );
    }
    let patch = ips::create(&source, &target).context("creating patch")?;
    let bytes = patch.to_bytes().context("encoding patch")?;
    fs::write(out, &bytes).with_context(|| format!("writing {}", out.display()))?;

    let counts = patch.summary();
    let truncation =
        counts.truncate.map_or_else(String::new, |len| format!(", truncates to {len:#X}"));
    let summary = format!(
        "Created {} records ({} raw, {} RLE){}, {} bytes. Wrote {}.",
        counts.records,
        counts.raw_records,
        counts.rle_records,
        truncation,
        bytes.len(),
        out.display()
    );
    let log = patch
        .records
        .iter()
        .enumerate()
        .map(|(index, record)| format!("{index:>5}  {record}"))
        .collect();
    Ok(Outcome { summary, warnings, log })
}

/// One grid row: label, current path (or a hint) and a Browse button.
/// Returns `true` when a new path was picked.
fn path_row(
    ui: &mut egui::Ui,
    label: &str,
    path: &mut Option<PathBuf>,
    pick: impl FnOnce() -> Option<PathBuf>,
) -> bool {
    ui.label(label);
    match path.as_deref() {
        Some(p) => ui.monospace(p.display().to_string()),
        None => ui.weak("not set"),
    };
    let picked = if ui.button("Browse…").clicked() { pick() } else { None };
    ui.end_row();
    picked.is_some_and(|p| {
        *path = Some(p);
        true
    })
}

fn pick_file(filter: &str, extensions: &[&str]) -> Option<PathBuf> {
    rfd::FileDialog::new().add_filter(filter, extensions).pick_file()
}

fn save_file(suggested: Option<&Path>, filter: &str, extensions: &[&str]) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().add_filter(filter, extensions);
    if let Some(path) = suggested {
        if let Some(dir) = path.parent() {
            dialog = dialog.set_directory(dir);
        }
        if let Some(name) = path.file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy());
        }
    }
    dialog.save_file()
}

/// `<stem>-patched.<ext>` next to `input`.
fn default_output(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .map_or_else(|| "patched".to_owned(), |s| s.to_string_lossy().into_owned());
    let name = input.extension().map_or_else(
        || format!("{stem}-patched"),
        |ext| format!("{stem}-patched.{}", ext.to_string_lossy()),
    );
    input.with_file_name(name)
}

/// Whether two paths name the same file (canonicalised when possible).
fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::default_output;

    #[test]
    fn default_output_name() {
        assert_eq!(default_output(Path::new("/x/smw.sfc")), Path::new("/x/smw-patched.sfc"));
        assert_eq!(default_output(Path::new("rom")), Path::new("rom-patched"));
    }
}
