//! Solar Magic: a Super Mario World level editor built on egui.

mod app;
mod capture;
mod document;
mod edit;
mod ips_tool;
mod panels;
mod play;

use std::path::PathBuf;
use std::process::ExitCode;

/// `solar-magic [ROM] [LEVEL-HEX] [--play]`: optional ROM path and level to
/// open; `--play` starts the level in the emulator right away.
fn parse_args() -> app::Launch {
    let mut args = std::env::args_os().skip(1);
    let rom = args.next().map(PathBuf::from);
    let level = args.next().and_then(|a| u16::from_str_radix(&a.to_string_lossy(), 16).ok());
    let play = args.any(|a| a == "--play");
    app::Launch { rom, level, play }
}

#[allow(clippy::print_stderr, reason = "startup failure goes to stderr")]
fn main() -> ExitCode {
    let launch = parse_args();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_title("Solar Magic"),
        ..Default::default()
    };
    let result = eframe::run_native(
        "Solar Magic",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, launch)))),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
