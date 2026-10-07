//! `solar`: command-line tools for inspecting Super Mario World ROMs.
//!
//! Every milestone adds a subcommand that proves its functionality; see
//! `docs/ARCHITECTURE.md`.

#![allow(clippy::print_stdout, clippy::print_stderr)]

mod dump_gfx;
mod dump_level;
mod dump_map16;
mod info;
mod png;
mod render;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// Command-line tools for Super Mario World ROMs.
#[derive(Debug, Parser)]
#[command(name = "solar", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print header, mapping and layout information about a ROM.
    Info {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
    },
    /// Decode GFX files and write each as a greyscale PNG tile sheet.
    DumpGfx {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
        /// Directory to write `GFXxx.png` files into (created if missing).
        out_dir: PathBuf,
        /// Only dump this file number (hex, e.g. `14`); default is every file.
        #[arg(long, value_parser = parse_hex_u16)]
        id: Option<u16>,
        /// Pixel scale factor.
        #[arg(long, default_value_t = 2)]
        scale: u32,
        /// Colour the tiles with this row (0-15) of the default level palette
        /// instead of greyscale.
        #[arg(long)]
        row: Option<usize>,
    },
    /// Print a level's headers, objects and sprites.
    DumpLevel {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
        /// Level number in hex (000-1FF), e.g. `105`.
        #[arg(value_parser = parse_hex_u16)]
        level: u16,
    },
    /// Render a level to a PNG by running the game's own level loader.
    Render {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
        /// Level number in hex (000-1FF), e.g. `105`.
        #[arg(value_parser = parse_hex_u16)]
        level: u16,
        /// Output PNG path.
        out: PathBuf,
        /// Which layers to draw.
        #[arg(long, value_enum, default_value_t = render::Layers::All)]
        layers: render::Layers,
        /// Pixel scale factor.
        #[arg(long, default_value_t = 1)]
        scale: u32,
        /// Draw a marker with the sprite number at each sprite position.
        #[arg(long)]
        sprites: bool,
    },
    /// Render Map16 pages 0-1 for a tileset (or the background set) as a PNG.
    DumpMap16 {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
        /// Output PNG path.
        out: PathBuf,
        /// FG/BG tileset number, 0-14.
        #[arg(long, default_value_t = 0)]
        tileset: u8,
        /// Render the background tile set instead of the foreground pages.
        #[arg(long)]
        bg: bool,
        /// Palette set selections as `fg,bg,sprite,back` (each 0-7).
        #[arg(long, default_value = "0,0,0,0")]
        palettes: String,
        /// Pixel scale factor.
        #[arg(long, default_value_t = 2)]
        scale: u32,
    },
}

/// Parses a hexadecimal number with or without a `0x` prefix.
fn parse_hex_u16(s: &str) -> Result<u16, String> {
    let digits = s.trim_start_matches("0x").trim_start_matches("0X");
    u16::from_str_radix(digits, 16).map_err(|e| format!("{s:?} is not a hex number: {e}"))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Info { rom } => info::run(&rom),
        Command::DumpGfx { rom, out_dir, id, scale, row } => {
            dump_gfx::run(&rom, &out_dir, dump_gfx::Options { id, scale, row })
        }
        Command::DumpLevel { rom, level } => dump_level::run(&rom, level),
        Command::Render { rom, level, out, layers, scale, sprites } => {
            render::run(&rom, &out, render::Options { level, layers, scale, sprites })
        }
        Command::DumpMap16 { rom, out, tileset, bg, palettes, scale } => {
            dump_map16::run(&rom, &out, &dump_map16::Options { tileset, bg, palettes, scale })
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
