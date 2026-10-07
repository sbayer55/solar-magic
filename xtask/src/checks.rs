//! The individual checks.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use tokei::{Config, LanguageType, Languages};
use walkdir::WalkDir;

/// Maximum lines of code (blank and comment lines excluded) per Rust file.
const MAX_LOC_PER_FILE: usize = 1000;

/// Directories whose Rust sources are checked.
const SOURCE_ROOTS: &[&str] = &["crates", "xtask"];

/// Crates whose job is printing, so crate-level print allows are fine there.
const PRINTING_CRATES: &[&str] = &["crates/solar-cli", "xtask"];

/// Runs every check, reporting all failures before returning an error.
pub(crate) fn run_all() -> anyhow::Result<()> {
    let root = workspace_root()?;
    let mut failures = Vec::new();
    failures.extend(check_file_sizes(&root));
    failures.extend(check_crate_level_allows(&root)?);
    if failures.is_empty() {
        println!("xtask check: all checks passed");
        return Ok(());
    }
    for failure in &failures {
        eprintln!("{failure}");
    }
    bail!("{} check failure(s)", failures.len());
}

/// The workspace root, taken from Cargo's environment when run via `cargo xtask`.
fn workspace_root() -> anyhow::Result<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().map(Path::to_path_buf).context("xtask manifest directory has no parent")
}

/// Fails for every `.rs` file with more than [`MAX_LOC_PER_FILE`] lines of code.
fn check_file_sizes(root: &Path) -> Vec<String> {
    let paths: Vec<PathBuf> = SOURCE_ROOTS.iter().map(|dir| root.join(dir)).collect();
    let config = Config { types: Some(vec![LanguageType::Rust]), ..Config::default() };
    let mut languages = Languages::new();
    languages.get_statistics(&paths, &[], &config);
    let Some(rust) = languages.get(&LanguageType::Rust) else {
        return Vec::new();
    };
    let mut failures: Vec<String> = rust
        .reports
        .iter()
        .filter(|report| report.stats.code > MAX_LOC_PER_FILE)
        .map(|report| {
            format!(
                "file size: {} has {} lines of code (limit {MAX_LOC_PER_FILE}); split it",
                display_relative(root, &report.name),
                report.stats.code
            )
        })
        .collect();
    failures.sort();
    failures
}

/// Fails for crate-level `#![allow(clippy::...)]` outside the printing crates.
///
/// Item-scoped `#[allow(..., reason = "...")]` is the sanctioned escape hatch.
fn check_crate_level_allows(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut failures = Vec::new();
    for dir in SOURCE_ROOTS {
        for entry in WalkDir::new(root.join(dir)).into_iter().filter_map(Result::ok) {
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "rs") {
                continue;
            }
            let relative = display_relative(root, path);
            if PRINTING_CRATES.iter().any(|crate_dir| relative.starts_with(crate_dir)) {
                continue;
            }
            let source = fs::read_to_string(path).with_context(|| format!("reading {relative}"))?;
            for (index, line) in source.lines().enumerate() {
                if line.trim_start().starts_with("#![allow(clippy::") {
                    failures.push(format!(
                        "lint hygiene: {relative}:{}: crate-level clippy allow; use an \
                         item-scoped #[allow(..., reason = \"...\")] instead",
                        index.saturating_add(1)
                    ));
                }
            }
        }
    }
    Ok(failures)
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).display().to_string().replace('\\', "/")
}
