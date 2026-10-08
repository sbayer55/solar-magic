//! `solar ips-info`: describe an IPS patch record by record.

use std::fs;
use std::path::Path;

use anyhow::Context as _;

/// Prints the summary and (up to `limit`) records of the patch at `path`.
pub(crate) fn run(path: &Path, limit: Option<usize>) -> anyhow::Result<()> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let parsed =
        ips::Patch::parse(&bytes).with_context(|| format!("parsing {}", path.display()))?;
    println!("file:            {}", path.display());
    println!("size:            {} bytes", bytes.len());
    println!("{}", parsed.summary());
    let shown = limit.unwrap_or(usize::MAX);
    if !parsed.records.is_empty() {
        println!("    #  offset    len     kind");
    }
    for (index, record) in parsed.records.iter().take(shown).enumerate() {
        println!("{index:>5}  {record}");
    }
    if parsed.records.len() > shown {
        println!("  ... {} more", parsed.records.len().saturating_sub(shown));
    }
    Ok(())
}
