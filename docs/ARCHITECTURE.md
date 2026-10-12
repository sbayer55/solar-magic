# Architecture

Solar Magic is a Rust reimplementation of Lunar Magic, the Super Mario World
level editor. This document is the map; the format details live in
[`formats/`](formats/) and the emulator's hardware notes in
[`emulation/`](emulation/).

## Crate graph

```
solar-magic (eframe GUI)          solar-cli (`solar`, PNG/dump/patch/emu tools)
        │         │                         │         │        │
        │         └────────────┬────────────│─────────┘        ▼
        │                      ▼            │              smw-play    launch a level, overlay edits
        │                     ips           │                  ▼
        └──────────────┬────────────────────┘              snes-core   the machine: clock, bus,
                                                               │       DMA, cart, Console
                       ▼                              ┌────────┼────────┐
                  smw-render        Level → grid      ▼        ▼        ▼
                       │            → RGBA raster  snes-ppu  snes-apu  (snes-rom, w65c816)
                       ▼
                   smw-data         SMW formats: RomLayout, headers, objects,
                       │            sprites, Map16, GFX, palettes
        ┌──────────────┼──────────────┐
        ▼              ▼              ▼
    snes-rom        lc-lz2         w65c816
  ROM container,  compression   cycle-exact 65c816 interpreter; runs the
  LoROM/ExLoROM/                game's object routines untimed, and the
  SA-1 mapping,                 whole game under snes-core
  header, RATS
```

Dependencies only point downward. Nothing below `solar-magic` knows about egui.
`xtask` is a workspace member that implements repository checks (`cargo xtask check`).

| Crate | Status | Responsibility |
|---|---|---|
| `snes-rom` | done | `Rom` (read, write, expand, checksum, save), `SnesAddr`, `Mapper`/`MapperKind` (LoROM, ExLoROM, SA-1 power-on map), `InternalHeader`, `rats::{scan, alloc}`, `copier_header` add/strip. No game knowledge. |
| `ips` | done | IPS patch parse/serialize/apply/create with RLE and the Lunar IPS truncation extension; 16 MiB limit; avoids the `0x454F46` EOF-offset trap. No game knowledge. See `formats/ips.md`. |
| `smw-data` | viewer, LM 2.5x reading, editing | `RomLayout` (vanilla tables plus Lunar Magic 2.5x's `LunarMagicTables`, known free space), `LevelId`, `gfx`, `palette`, `map16`, `tileset`, `level` (headers with bit-field tables, objects including Lunar Magic's, layer 2 including 16-bit tilemaps with an RLE codec, sprites, secondary exits; byte-exact re-serialization; editing API; vanilla-safe `save_level`). |
| `lc-lz2` | done | LC_LZ2 decompress and compress. |
| `w65c816` | done | CPU interpreter with a `Bus` trait. Cycle-exact through `Bus::idle` (a no-op for untimed buses), hardware `nmi`/`irq`/`reset`, `WAI`/`STP` states. Passes all 512 SingleStepTests 65816 files, native and emulation mode, including the exact bus-cycle sequence (`W65C816_TESTS_DIR`). See `docs/emulation/cpu.md`. |
| `snes-ppu` | renders modes 0-7 | `Ppu`: `$2100-$213F` decoded into `Regs`, `Vram`/`Oam`/`Cgram` with their port quirks, counter latches, open-bus bits; renderer with per-line layer buffers and per-dot compositing (sprites with the 32/34 limits, windows, colour math, direct colour, mosaic, mode 7 + EXTBG), BGR555 `Frame`. Hi-res/interlace/offset-per-tile pending. `docs/emulation/ppu.md`. |
| `spc700` | done | Cycle-exact SPC700 core (`Spc700`, `ApuBus`, 256-entry table); passes all 256 SingleStepTests files with exact cycle sequences (`SPC700_TESTS_DIR`). |
| `snes-apu` | done | `Apu`/`Smp`: 64 KiB RAM, `$F0-$FF` registers, timers, CPU ports, exact master↔SPC clock ratio, boot ROM modelled at port level (`Ipl::Hle`) or supplied (`Ipl::Rom`); `Dsp`: eight BRR voices with Gaussian interpolation, ADSR/GAIN, noise, echo FIR and mixer at 32 kHz (`take_audio`). `docs/emulation/apu.md`. |
| `snes-core` | boots SMW to the title screen | `Clock` (master cycles, lines, dots, events), `SystemBus` (speeds, open bus, routing, lazy catch-up), `CpuRegs` (NMI/IRQ, ALU, joypads), `Dma` (GPDMA/HDMA), `Cartridge` (ROM, SRAM, `Coprocessor` hook), `Console` facade (run frames, breakpoints, peek/poke, overlays). `docs/emulation/{timing,memory-map}.md`. |
| `smw-play` | launches levels | `Launcher`: boots to the title screen, pokes player state and game mode, substitutes the level number at the entry routine's breakpoint, overlays unsaved edits (`docs/formats/smw-launch-protocol.md`). `MarioState`, `Entrance::{Main, Midway, SecondaryExit, Position}`. |
| `smw-render` | viewer, LM 2.5x, ownership | `Raster`, `CharAtlas`, tile drawing; `emu::SnesBus` (with overlays) + `level::load_level_data` run the game's loader on a `Level` value and read back `TileGrid`s; `ownership::AttributedBus` records which object stamped each tile; `map16_ext` resolves Lunar Magic pages 2+; `scene::LevelScene` composes layers, background and sprite markers for both the CLI and the GUI. |
| `solar-cli` | one subcommand per milestone | `info`, `dump-gfx`, `dump-map16`, `dump-level [--json]`, `dump-exits`, `dump-owners`, `render`, `expand`, `free-space`, `rats`, `relocate-level`, `copy-level`, `apply-level`, `bg-rle`, `gfx-check`, `ips-apply`, `ips-create`, `ips-info`, `emu-boot`, `play`. Also the test oracle. |
| `solar-magic` | editor + play | eframe app with egui_dock panels: level list, zoomable level view with selection/drag/placement and overlays, Edit/Header/Exits panels, info, Map16 sheet, palette, and a Play panel (Tools → Play Level, Ctrl+T) that runs the edited level in the in-house emulator on its own thread with keyboard input and sound (cpal, paced on the audio queue). Undo/redo, clipboard, Save / Save As. Tools → IPS Patcher window. Remembers the last ROM and level. |

## The vanilla / Lunar Magic seam

Every table address is read from a `smw_data::RomLayout` resolved once at open
time. `RomLayout::detect` returns the vanilla layout, or, for a ROM carrying a
Lunar Magic signature whose routines match the 2.5x fingerprints, the vanilla
layout plus `lm: Some(LunarMagicTables)` (`docs/formats/lunar-magic-ext.md`).
Any other Lunar Magic version is refused with the first missing fingerprint.
Decoders never hardcode addresses; Lunar Magic-only behaviour is gated on
`layout.lm` / `layout.object_format()`.

Done for Lunar Magic 2.5x: detection, level data (direct Map16 objects, 5-byte
exits), 16-bit backgrounds, custom palettes, ExGFX and the per-level GFX
bypass, Map16 pages 2+ (by running LM's getter in `w65c816`) and BG page
sets. Open: the bypass table's extra slots (words 0–3, probably BG2/BG3/AN2),
layer 3, sprites' graphics, and other LM versions.

## Editing and saving

Edits never touch the `Rom` until Save. The GUI edits a `Level` value through
`smw_data::level`'s editing API (positions are absolute `TilePos`es; screen
flags and jumps are regenerated by `ObjectStream::renormalize`), previews it
by overlaying the serialized bytes on the emulated bus
(`smw_render::level::load_level_data`), and records every command as a
memento in `solar_magic::edit::History`. `save_level` writes into a clone of
the ROM: unchanged parts are left alone, layer data is relocated into
RATS-tagged expanded space (expanding to 1 MiB first, exactly where Lunar
Magic puts it), sprites and background tilemaps stay in their banks (in place
when they fit, else in the known free ranges, else sharing an identical
block another level already uses), pointer tables and secondary header bytes
are updated and the checksum fixed. See `docs/formats/rats.md`.

Per-object selection comes from `smw_render::ownership`: the loader fetches
each object's bytes through the bus, so a wrapper tags every subsequent
tile-RAM write with that object; the last writer wins.

## Rendering pipeline

```
Rom ─RomLayout─▶ Level{headers, objects, layer2, sprites}
                 ─w65c816 runs object routines─▶ TileGrid (L1), TileGrid (L2)
Map16 + GfxFiles + Palette ─▶ TileAtlas (8x8 → RGBA per palette row, cached)
TileGrid × TileAtlas ─▶ Raster per layer ─▶ composite ─▶ PNG / egui texture
Sprites ─▶ overlay
```

Why run the game's code instead of hand-porting object routines: there are
roughly 0x40 standard, 0x100 extended, and many tileset-specific objects, and
Lunar Magic patches some of them. Executing the ROM's own routines in a tiny
65c816 interpreter against fake WRAM gives bit-exact results for every level
and every patched ROM. Running one object at a time and diffing the grid also
records which object owns each tile, which the editor needs for selection.

## Code quality gates

All of these run in CI (`.github/workflows/ci.yml`) and locally:

| Gate | Command | Where configured |
|---|---|---|
| Formatting | `cargo fmt --all --check` | `rustfmt.toml` |
| Lints | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | `[workspace.lints]` in `Cargo.toml`, thresholds in `clippy.toml` |
| File size: 1000 lines of code per `.rs` file | `cargo xtask check` | `xtask/src/checks.rs` |
| No crate-level `#![allow(clippy::…)]` outside printing crates | `cargo xtask check` | `xtask/src/checks.rs` |
| Tests | `cargo test --workspace` (set `SMW_ROM=/path/to/rom.sfc` to include real-ROM tests) | |
| Docs | `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS=-D warnings` | |
| Licenses / advisories | `cargo deny check` | `deny.toml` |

Lint policy in one line: `clippy::all`, `pedantic`, `nursery` and `cargo` are
on, plus a set of restriction lints (`unwrap_used`, `expect_used`, `panic`,
`indexing_slicing`, `arithmetic_side_effects`, `as_conversions`, the `cast_*`
family, …). Thresholds: cognitive complexity 15, function length 80 lines,
5 arguments. Exceptions are item-scoped `#[allow(lint, reason = "…")]`.
Consequence: use `.get()`, `checked_`/`wrapping_` arithmetic, and
`u32::from`/`u8::try_from` instead of indexing, bare operators and `as`.
Dispatch tables beat giant `match` statements.

## Testing strategy

- Bit decoders: table-driven unit tests with known bytes.
- Real ROM: tests that need it read `SMW_ROM` from the environment and skip
  (with a note) when it is unset, so CI passes without a ROM.
- Round-trip: every parsed level re-serializes to the original bytes (M3);
  every secondary exit and every background tilemap round-trips too.
- Saving: relocating a level renders pixel-identically; copying a level and
  editing the copy never touches the shared source block; the GUI document
  test edits, undoes, redoes, saves to a temp file and reopens.
- Ownership: attribution equals the "load one more object and diff" oracle.
- Golden grids: `tests/golden/<level>.grid` dumped from an emulator's
  `$7EC800`/`$7FC800` after loading the level; compared cell-for-cell
  (planned, see the roadmap).
- Emulator: the two SingleStepTests suites (cycle-exact 65816 and SPC700,
  env-gated), krom's open-source SNES test ROMs run against their hardware
  screenshots (`cargo xtask fetch-test-roms`, pinned commit, never vendored;
  strict list in `crates/snes-core/tests/test_roms.rs`, CI job `emu-roms`),
  and `SMW_ROM`/`LM_ROM`-gated boot, launch and audio tests with local-only
  goldens under `SMW_GOLDENS_DIR`.
- ROMs, disassemblies and Lunar Magic binaries are never committed.
