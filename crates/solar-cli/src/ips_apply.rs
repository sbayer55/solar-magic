//! `solar ips-apply`: apply an IPS patch to a file.

use std::fs;
use std::path::Path;

use anyhow::Context as _;
use snes_rom::copier_header::{self, HeaderMode};

/// Copier-header handling for the target, as a command-line value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Header {
    /// Patch the file bytes as they are.
    Keep,
    /// Prepend a zeroed 512-byte header first (for patches made on headered ROMs).
    Add,
    /// Remove the 512-byte header first (for patches made on headerless ROMs).
    Strip,
}

impl From<Header> for HeaderMode {
    fn from(header: Header) -> Self {
        match header {
            Header::Keep => Self::Keep,
            Header::Add => Self::Add,
            Header::Strip => Self::Strip,
        }
    }
}

/// Flags for [`run`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct Options {
    /// What to do with the target's copier header before patching.
    pub(crate) header: Header,
    /// Print one line per record.
    pub(crate) log: bool,
}

/// Applies `patch` to `target` and writes the result to `out`.
pub(crate) fn run(patch: &Path, target: &Path, out: &Path, options: Options) -> anyhow::Result<()> {
    let patch_bytes = fs::read(patch).with_context(|| format!("reading {}", patch.display()))?;
    let parsed =
        ips::Patch::parse(&patch_bytes).with_context(|| format!("parsing {}", patch.display()))?;
    let raw = fs::read(target).with_context(|| format!("reading {}", target.display()))?;
    let raw_len = raw.len();
    let source = copier_header::apply_mode(raw, options.header.into());
    let applied = ips::apply(&source, &parsed).context("applying patch")?;
    fs::write(out, &applied.data).with_context(|| format!("writing {}", out.display()))?;

    let summary = parsed.summary();
    let report = &applied.report;
    println!(
        "patch:           {} records ({} raw, {} rle)",
        summary.records, summary.raw_records, summary.rle_records
    );
    println!(
        "target:          {} bytes, copier header {}",
        raw_len,
        yes_no(copier_header::detect(raw_len))
    );
    if options.header != Header::Keep {
        println!(
            "header mode:     {} -> {} bytes",
            HeaderMode::from(options.header).label(),
            source.len()
        );
    }
    println!("output:          {report}");
    println!("copier header:   {}", yes_no(copier_header::detect(report.output_len)));
    println!("wrote:           {}", out.display());
    if options.log {
        for (record, entry) in parsed.records.iter().zip(&report.entries) {
            let note = if entry.extended { "  (extends file)" } else { "" };
            println!("  {:>5}  {record}{note}", entry.index);
        }
    }
    Ok(())
}

const fn yes_no(flag: bool) -> &'static str {
    if flag { "yes" } else { "no" }
}
