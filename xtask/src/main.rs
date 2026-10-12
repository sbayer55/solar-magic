//! Repository checks that have no clippy equivalent.
//!
//! Run with `cargo xtask check`. CI runs the same command.

#![allow(clippy::print_stdout, clippy::print_stderr)]

mod checks;
mod test_roms;

use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "xtask", about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run every repository check (file size limit, lint hygiene).
    Check,
    /// Fetch the open-source SNES test ROMs into `target/test-roms`.
    FetchTestRoms,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Check => checks::run_all(),
        Command::FetchTestRoms => test_roms::fetch(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
