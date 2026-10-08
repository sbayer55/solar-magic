//! `solar`: command-line tools for inspecting Super Mario World ROMs.
//!
//! Every milestone adds a subcommand that proves its functionality; see
//! `docs/ARCHITECTURE.md`.

#![allow(clippy::print_stdout, clippy::print_stderr)]

mod bg_rle;
mod dump_exits;
mod dump_gfx;
mod dump_level;
mod dump_map16;
mod dump_owners;
mod expand;
mod free_space;
mod gfx_check;
mod info;
mod level_io;
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
        /// Print the parsed level as JSON (the input format of `apply-level`).
        #[arg(long)]
        json: bool,
    },
    /// Replace a level with one described by `dump-level --json` output.
    ApplyLevel {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
        /// Output ROM path.
        out: PathBuf,
        /// JSON file holding the level.
        json: PathBuf,
    },
    /// Move a level's layer data into expanded space, leaving it unchanged.
    RelocateLevel {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
        /// Output ROM path.
        out: PathBuf,
        /// Level number in hex (000-1FF).
        #[arg(value_parser = parse_hex_u16)]
        level: u16,
    },
    /// Copy one level over another.
    CopyLevel {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
        /// Output ROM path.
        out: PathBuf,
        /// Source level number in hex.
        #[arg(value_parser = parse_hex_u16)]
        src: u16,
        /// Destination level number in hex.
        #[arg(value_parser = parse_hex_u16)]
        dst: u16,
    },
    /// Print which layer 1 object owns each tile of a level.
    DumpOwners {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
        /// Level number in hex (000-1FF).
        #[arg(value_parser = parse_hex_u16)]
        level: u16,
    },
    /// Print secondary exits (all non-blank ones, or a single one).
    DumpExits {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
        /// Exit number in hex (000-1FF); default is every used exit.
        #[arg(value_parser = parse_hex_u16)]
        exit: Option<u16>,
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
        /// Tint every layer 1 tile by the object that owns it.
        #[arg(long)]
        owners: bool,
    },
    /// Expand a ROM to a larger power-of-two size, fix its checksum and write
    /// the result to a new file.
    Expand {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
        /// Output path for the expanded image.
        out: PathBuf,
        /// Target size, e.g. `1M`, `2M`, `4M` or a byte count.
        #[arg(long, default_value = "1M", value_parser = expand::parse_size)]
        size: usize,
    },
    /// List runs of unused bytes (0x00/0xFF outside RATS tags).
    FreeSpace {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
        /// Only search this LoROM bank (hex), e.g. `07`.
        #[arg(long, value_parser = parse_hex_u8)]
        bank: Option<u8>,
        /// Hide runs shorter than this many bytes.
        #[arg(long, default_value_t = 16)]
        min_len: usize,
    },
    /// Re-encode every background tilemap and verify the encoder.
    BgRle {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
    },
    /// Re-compress every GFX file and verify the LC_LZ2 encoder.
    GfxCheck {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
    },
    /// List every RATS tag in the ROM.
    Rats {
        /// Path to the `.sfc` / `.smc` file.
        rom: PathBuf,
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

/// Parses a hexadecimal byte with or without a `0x` prefix.
fn parse_hex_u8(s: &str) -> Result<u8, String> {
    let value = parse_hex_u16(s)?;
    u8::try_from(value).map_err(|_| format!("{s:?} does not fit in a byte"))
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
        Command::DumpLevel { rom, level, json } => dump_level::run(&rom, level, json),
        Command::ApplyLevel { rom, out, json } => level_io::apply(&rom, &out, &json),
        Command::RelocateLevel { rom, out, level } => level_io::relocate(&rom, &out, level),
        Command::CopyLevel { rom, out, src, dst } => level_io::copy(&rom, &out, src, dst),
        Command::DumpExits { rom, exit } => dump_exits::run(&rom, exit),
        Command::Render { rom, level, out, layers, scale, sprites, owners } => {
            render::run(&rom, &out, render::Options { level, layers, scale, sprites, owners })
        }
        Command::DumpOwners { rom, level } => dump_owners::run(&rom, level),
        Command::Expand { rom, out, size } => expand::run(&rom, &out, size),
        Command::FreeSpace { rom, bank, min_len } => {
            free_space::free_space(&rom, free_space::Options { bank, min_len })
        }
        Command::Rats { rom } => free_space::rats_list(&rom),
        Command::BgRle { rom } => bg_rle::check(&rom),
        Command::GfxCheck { rom } => gfx_check::check(&rom),
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
