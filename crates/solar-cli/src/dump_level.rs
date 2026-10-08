//! `solar dump-level`: print a level's decoded headers, objects and sprites.

use std::path::Path;

use anyhow::Context as _;
use smw_data::gfx_bypass::GfxBypass;
use smw_data::level::{Layer2, Level, ObjectKind};
use smw_data::tileset::level_gfx_slots;
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
        Layer2::Background { page_set, rows_per_screen, tiles, compressed } => {
            println!(
                "\nLayer 2: background tilemap at {} (page set {page_set}, {rows_per_screen} rows/screen, first tile {:#05X}, {} compressed bytes)",
                level.addresses.layer2,
                tiles.first().copied().unwrap_or(0),
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
    let slots = level_gfx_slots(rom, layout, level.id, *h)?;
    let name =
        |id: Option<smw_data::gfx::GfxId>| id.map_or_else(|| "-".to_owned(), |i| i.to_string());
    println!(
        "  gfx: FG1 {} FG2 {} BG1 {} FG3 {}  SP1-4 {} {} {} {}{}",
        name(slots.layer[0]),
        name(slots.layer[1]),
        name(slots.layer[2]),
        name(slots.layer[3]),
        name(slots.sprites[0]),
        name(slots.sprites[1]),
        name(slots.sprites[2]),
        name(slots.sprites[3]),
        if slots.bypassed { "  (Lunar Magic bypass)" } else { "" }
    );
    if let Some(bypass) = GfxBypass::load(rom, layout, level.id)? {
        println!("  bypass row: {:04X?}", bypass.words);
    }
    if let Some(lm) = &level.lm {
        println!(
            "  lunar magic: flags {:#04X} (relocated tilemap: {}, 16-bit: {}, page {})  custom palette: {}  secondary 4-6: {:02X?}",
            lm.flags.0,
            lm.flags.relocated_tilemap(),
            lm.flags.sixteen_bit(),
            lm.flags.page(),
            lm.custom_palette.map_or_else(|| "none".to_owned(), |a| a.to_string()),
            lm.secondary_ext
        );
    }
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
            ObjectKind::ScreenExitExt { screen, destination, secondary, flags } => format!(
                "LM screen exit scr {screen:02X} -> {destination:03X}{} flags {flags:02X}",
                if secondary { " (secondary)" } else { "" }
            ),
            ObjectKind::DirectMap16 { tile, size } => {
                format!("direct Map16 {tile:03X} size {size:02X}")
            }
            ObjectKind::DirectMap16Ext { tile, size, ext, ext_len } => format!(
                "direct Map16 {tile:04X} size {size:02X} ext {:02X?}",
                ext.get(..usize::from(ext_len)).unwrap_or(&[])
            ),
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
