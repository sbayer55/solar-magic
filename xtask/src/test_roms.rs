//! `cargo xtask fetch-test-roms`: a sparse, pinned checkout of krom's
//! open-source SNES test ROMs (prebuilt `.sfc` files with hardware
//! screenshots) into `target/test-roms/SNES`. The checkout is never
//! committed; `SNES_TEST_ROMS=target/test-roms/SNES cargo test -p snes-core
//! --release --test test_roms` uses it.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, bail};

/// Upstream repository.
const REPO: &str = "https://github.com/PeterLemon/SNES.git";
/// Pinned commit (2022-09-20).
const COMMIT: &str = "350b394e86ec5d62f600b5cbf64cdce3721bb6ef";
/// Directories the tests use.
const PATHS: [&str; 2] = ["PPU", "CPUTest"];

fn git(dir: &Path, args: &[&str]) -> anyhow::Result<()> {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !status.success() {
        bail!("git {} failed with {status}", args.join(" "));
    }
    Ok(())
}

/// Fetches (or updates) the checkout and prints where it is.
pub(crate) fn fetch() -> anyhow::Result<()> {
    let root = PathBuf::from("target").join("test-roms");
    std::fs::create_dir_all(&root).context("creating target/test-roms")?;
    let checkout = root.join("SNES");
    if !checkout.join(".git").exists() {
        git(&root, &["clone", "--filter=blob:none", "--sparse", "--no-checkout", REPO, "SNES"])?;
    }
    git(&checkout, &["sparse-checkout", "set", PATHS[0], PATHS[1]])?;
    git(&checkout, &["fetch", "--depth", "1", "origin", COMMIT])?;
    git(&checkout, &["checkout", "--quiet", COMMIT])?;
    println!("test ROMs ready in {} (commit {COMMIT})", checkout.display());
    println!(
        "run: SNES_TEST_ROMS={} cargo test -p snes-core --release --test test_roms",
        checkout.display()
    );
    Ok(())
}
