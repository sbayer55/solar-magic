//! `solar ips-create`: build an IPS patch from an original/modified pair.

use std::fs;
use std::path::Path;

use anyhow::Context as _;
use snes_rom::copier_header;

/// Writes to `out` a patch that turns `original` into `modified`.
pub(crate) fn run(original: &Path, modified: &Path, out: &Path, log: bool) -> anyhow::Result<()> {
    let source = fs::read(original).with_context(|| format!("reading {}", original.display()))?;
    let target = fs::read(modified).with_context(|| format!("reading {}", modified.display()))?;
    if copier_header::detect(source.len()) != copier_header::detect(target.len()) {
        eprintln!(
            "warning: original and modified differ in copier-header state; every offset will be \
             shifted by 512 bytes"
        );
    }
    let patch = ips::create(&source, &target).context("creating patch")?;
    let bytes = patch.to_bytes().context("encoding patch")?;
    fs::write(out, &bytes).with_context(|| format!("writing {}", out.display()))?;

    println!("original:        {} bytes", source.len());
    println!("modified:        {} bytes", target.len());
    println!("{}", patch.summary());
    println!("patch size:      {} bytes", bytes.len());
    println!("wrote:           {}", out.display());
    if log {
        for (index, record) in patch.records.iter().enumerate() {
            println!("  {index:>5}  {record}");
        }
    }
    Ok(())
}
