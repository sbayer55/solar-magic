//! `solar expand`: grow a ROM image and fix its checksum.

use std::path::Path;

use anyhow::{Context as _, bail};
use snes_rom::Rom;

/// Parses a size like `1M`, `2M`, `512K` or a plain byte count.
pub(crate) fn parse_size(s: &str) -> Result<usize, String> {
    let s = s.trim();
    let (digits, unit) = s.split_at(s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len()));
    let n: usize = digits.parse().map_err(|e| format!("{s:?}: {e}"))?;
    let multiplier = match unit.trim().to_ascii_uppercase().as_str() {
        "" | "B" => 1,
        "K" | "KB" | "KIB" => 1024,
        "M" | "MB" | "MIB" => 1024 * 1024,
        other => return Err(format!("unknown size unit {other:?}")),
    };
    n.checked_mul(multiplier).ok_or_else(|| format!("{s:?} is too large"))
}

/// Expands the ROM at `path` to `size` bytes and writes it to `out`.
pub(crate) fn run(path: &Path, out: &Path, size: usize) -> anyhow::Result<()> {
    let mut rom = Rom::load(path).with_context(|| format!("loading {}", path.display()))?;
    let before = rom.len();
    if size < before {
        bail!("{} is already {before} bytes; cannot shrink to {size}", path.display());
    }
    rom.expand_to(size).context("expanding")?;
    let checksum = rom.fix_checksum();
    rom.save(out).with_context(|| format!("writing {}", out.display()))?;
    println!(
        "expanded {} -> {} bytes (size code ${:02X}), checksum ${checksum:04X}, wrote {}",
        before,
        rom.len(),
        rom.header().rom_size_code,
        out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::parse_size;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("1M"), Ok(1 << 20));
        assert_eq!(parse_size("2 MiB"), Ok(2 << 20));
        assert_eq!(parse_size("512K"), Ok(512 << 10));
        assert_eq!(parse_size("4096"), Ok(4096));
        assert!(parse_size("1G").is_err());
        assert!(parse_size("x").is_err());
    }
}
