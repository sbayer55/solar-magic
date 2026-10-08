//! `solar dump-level`: print a level's decoded headers, objects and sprites.

use std::path::Path;

use anyhow::Context as _;
use smw_data::level::{Layer2, Level, ObjectKind};
use smw_data::{LevelId, RomLayout};
use snes_rom::Rom;

/// Prints everything known about level `number`, or its JSON form.
pub(crate) fn run(rom_path: &Path, number: u16, json: bool) -> anyhow::Result<()> {
    let rom = Rom::load(rom_path).with_context(|| format!("loading {}", rom_path.display()))?;
    let layout = RomLayout::detect(&rom)?;
    let id = LevelId::new(number)
        .with_context(|| format!("level {number:#X} is out of range (max 1FF)"))?;
    let level = Level::load(&rom, &layout, id)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&level).context("serializing level")?);
        return Ok(());
    }
    print_headers(&rom, &layout, &level)?;
    print_objects("Layer 1", &level.layer1);
    match &level.layer2 {
        Layer2::Objects { stream, .. } => print_objects("Layer 2", stream),
        Layer2::Background { page, compressed, .. } => {
            println!(
                "\nLayer 2: background tilemap at {} (BG page {page}, {} compressed bytes)",
                level.addresses.layer2,
                compressed.len()
            );
        }
    }
    print_sprites(&level);
    Ok(())
}

fn print_headers(rom: &Rom, layout: &RomLayout, level: &Level) -> anyhow::Result<()> {
    let h = &level.header;
    let s = &level.secondary;
    let mode = h.level_mode();
    println!("Level {}", level.id);
    println!("  layer 1 data:   {}", level.addresses.layer1);
    println!("  layer 2 ptr:    {} -> {}", level.addresses.layer2_raw, level.addresses.layer2);
    println!("  sprite data:    {}", level.addresses.sprites);
    println!("  header bytes:   {:02X?}  secondary: {:02X?}", h.raw, s.raw);
    println!(
        "  screens: {}  mode: {:#04X} ({})  tileset: {}  sprite set: {}",
        h.screens(),
        mode.get(),
        if mode.is_vertical(rom, layout)? { "vertical" } else { "horizontal" },
        h.tileset(),
        h.sprite_set()
    );
    println!(
        "  palettes: fg {} bg {} sprite {} back area {}   music: {}  time: {}s  layer 3 priority: {}",
        h.foreground_palette(),
        h.background_palette(),
        h.sprite_palette(),
        h.back_area_color(),
        h.music(),
        h.time_limit_seconds(),
        h.layer3_priority()
    );
    println!("  scroll: {}  item memory: {}", h.scroll_setting(), h.item_memory());
    println!(
        "  entrance: screen {} x-index {} y-index {} type {}  midway screen {}  layer 2 scroll {}  layer 3 {}",
        s.entrance_screen(),
        s.entrance_x_index(),
        s.entrance_y_index(),
        s.entrance_type(),
        s.midway_screen(),
        s.layer2_scroll(),
        s.layer3_setting()
    );
    println!(
        "  initial y: layer 1 code {} layer 2 code {}  screen mode bits {}  no-yoshi intro disabled: {}",
        s.layer1_initial_y(),
        s.layer2_initial_y(),
        s.screen_mode(),
        s.disable_no_yoshi_intro()
    );
    println!(
        "  sprite memory: {}  buoyancy: {}",
        level.sprites.sprite_memory(),
        level.sprites.buoyancy()
    );
    Ok(())
}

fn print_objects(name: &str, stream: &smw_data::level::ObjectStream) {
    println!("\n{name}: {} objects", stream.objects.len());
    println!("  idx  off  scr  x   y   kind                               bytes");
    for (i, o) in stream.objects.iter().enumerate() {
        let kind = match o.kind() {
            ObjectKind::Standard { number, size } => {
                format!("standard {number:02X} size {size:02X}")
            }
            ObjectKind::Extended { number } => format!("extended {number:02X}"),
            ObjectKind::ScreenExit { screen, destination, secondary } => {
                format!(
                    "screen exit scr {screen:02X} -> {destination:03X}{}",
                    if secondary { " (secondary)" } else { "" }
                )
            }
            ObjectKind::ScreenJump { screen } => format!("screen jump -> {screen:02X}"),
        };
        let bytes: Vec<String> = o.as_bytes().iter().map(|b| format!("{b:02X}")).collect();
        println!(
            "  {i:3}  {:3}  {:02X}  {:2X}  {:2X}  {kind:<34} {}{}",
            o.offset,
            o.screen,
            o.x(),
            o.y(),
            bytes.join(" "),
            if o.new_screen() { "  [new screen]" } else { "" }
        );
    }
}

fn print_sprites(level: &Level) {
    println!("\nSprites: {}", level.sprites.sprites.len());
    println!("  idx  scr  x   y   extra  number");
    for (i, s) in level.sprites.sprites.iter().enumerate() {
        println!(
            "  {i:3}  {:02X}  {:2X}  {:2X}  {}      {:02X}{}",
            s.screen(),
            s.x(),
            s.y(),
            s.extra_bits(),
            s.number(),
            if s.is_scroll_command() { "  (scroll command)" } else { "" }
        );
    }
}
