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
                       ▼                         ┌────────┬───┴────┬────────┐
                  smw-render        Level → grid ▼        ▼        ▼        ▼
                       │            → RGBA raster snes-ppu snes-apu snes-sa1 (snes-rom, w65c816)
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
| `smw-data` | viewer, LM reading, editing, `.mwl` | `RomLayout` (vanilla tables plus Lunar Magic's `LunarMagicTables` with per-feature detection, known free space, `known_ranges`, markers), `LevelId`, `gfx`, `palette`, `map16`, `tileset`, `level` (headers with bit-field tables, objects including Lunar Magic's, layer 2 including 16-bit tilemaps with an RLE codec, sprites, secondary exits; byte-exact re-serialization; editing API; vanilla-safe `save_level`), `formats::mwl` (Lunar Magic level files, export and import). |
| `lc-lz2` | done | LC_LZ2 decompress and compress. |
| `w65c816` | done | CPU interpreter with a `Bus` trait. Cycle-exact through `Bus::idle` (a no-op for untimed buses), hardware `nmi`/`irq`/`reset`, `WAI`/`STP` states. Passes all 512 SingleStepTests 65816 files, native and emulation mode, including the exact bus-cycle sequence (`W65C816_TESTS_DIR`). See `docs/emulation/cpu.md`. |
| `snes-ppu` | renders modes 0-7 | `Ppu`: `$2100-$213F` decoded into `Regs`, `Vram`/`Oam`/`Cgram` with their port quirks, counter latches, open-bus bits; renderer with per-line layer buffers and per-dot compositing (sprites with the 32/34 limits, windows, colour math, direct colour, mosaic, mode 7 + EXTBG), BGR555 `Frame`. Hi-res/interlace/offset-per-tile pending. `docs/emulation/ppu.md`. |
| `spc700` | done | Cycle-exact SPC700 core (`Spc700`, `ApuBus`, 256-entry table); passes all 256 SingleStepTests files with exact cycle sequences (`SPC700_TESTS_DIR`). |
| `snes-apu` | done | `Apu`/`Smp`: 64 KiB RAM, `$F0-$FF` registers, timers, CPU ports, exact master↔SPC clock ratio, boot ROM modelled at port level (`Ipl::Hle`) or supplied (`Ipl::Rom`); `Dsp`: eight BRR voices with Gaussian interpolation, ADSR/GAIN, noise, echo FIR and mixer at 32 kHz (`take_audio`). `docs/emulation/apu.md`. |
| `snes-sa1` | unit-tested only | `Sa1`: second `w65c816::Cpu` over its own bus with I-RAM, BW-RAM (linear, windowed, bitmap), Super MMC, programmable vectors, interrupts both ways, H/V timer, DMA and character conversion, arithmetic unit, bit reader; behind `snes-core`'s `Coprocessor` trait. No SA-1 ROM available locally. `docs/emulation/sa1.md`. |
| `snes-core` | boots SMW to the title screen | `Clock` (master cycles, lines, dots, events), `SystemBus` (speeds, open bus, routing, lazy catch-up), `CpuRegs` (NMI/IRQ, ALU, joypads), `Dma` (GPDMA/HDMA), `Cartridge` (ROM, SRAM, `Coprocessor` hook), `Console` facade (run frames, breakpoints, peek/poke, overlays). `docs/emulation/{timing,memory-map}.md`. |
| `smw-play` | launches levels | `Launcher`: boots to the title screen, pokes player state and game mode, substitutes the level number at the entry routine's breakpoint, overlays unsaved edits (`docs/formats/smw-launch-protocol.md`). `MarioState`, `Entrance::{Main, Midway, SecondaryExit, Position}`. |
| `smw-render` | viewer, LM 2.5x, ownership, tracing, machine picture | `Raster`, `CharAtlas`, tile drawing; `emu::SnesBus` (with overlays and an optional access `Trace`) + `level::load_level_data` run the game's loader on a `Level` value and read back `TileGrid`s; `ownership::AttributedBus` records which object stamped each tile; `map16_ext` resolves Lunar Magic pages 2+; `assets::LevelAssets` (plain data captured from the emulator), `plane`/`compose` (index planes and the PPU's mode 1 compositing); `scene::LevelScene` composes layers, background, layer 3 and sprite markers for both the CLI and the GUI. |
| `solar-cli` | one subcommand per milestone | `info`, `dump-gfx`, `dump-map16`, `dump-level [--json]`, `dump-exits`, `dump-owners`, `render`, `trace`, `diff-rom`, `expand`, `free-space`, `rats`, `relocate-level`, `copy-level`, `apply-level`, `mwl-export`, `mwl-import`, `mwl-info`, `export-all`, `import-all`, `bg-rle`, `gfx-check`, `ips-apply`, `ips-create`, `ips-info`, `emu-boot`, `emu-bench`, `play`. Also the test oracle. |
| `solar-magic` | editor + play | eframe app with egui_dock panels: level list, zoomable level view (machine-exact picture captured on a worker thread, `capture.rs`; plain picture meanwhile) with selection/drag/placement and overlays, Edit/Header/Exits panels, info, Map16 sheet, palette, a Play panel (Tools → Play Level, Ctrl+T; right-click a tile → Play from here) that runs the edited level in the in-house emulator on its own thread with keyboard or gamepad input and sound (cpal, paced on the audio queue), save/load state, a rewind ring (10 s) and `<rom>.srm` battery saves, and a Debug panel (registers, disassembly with click-to-break, breakpoints, watches, memory page, instruction trace, single-step). Undo/redo, clipboard, Save / Save As. Tools → IPS Patcher window. Remembers the last ROM and level. |

## The vanilla / Lunar Magic seam

Every table address is read from a `smw_data::RomLayout` resolved once at open
time. `RomLayout::detect` returns the vanilla layout, or, for a ROM carrying a
Lunar Magic signature, the vanilla layout plus `lm: Some(LunarMagicTables)`
(`docs/formats/lunar-magic-ext.md`). Detection is per feature: every known
hijack site is checked independently (`scan_hijacks`, the `HIJACKS` table)
and the result lands in `LunarMagicTables::features`; only the layer 2 hook
and its call site are required, and the hook's variant fixes
`Layer2FlagsFormat` (2.5x or 3.x). Decoders never hardcode addresses;
Lunar Magic-only behaviour is gated on `layout.lm`, the feature flags and
`layout.object_format()`.

Done for Lunar Magic 2.5x and 3.71: detection, level data (direct Map16
objects, 5-byte exits), 16-bit backgrounds, 3.x's rewritten background bank
bytes and flag bits, custom palettes, ExGFX and the per-level GFX bypass,
Map16 pages 2+ (by running LM's getter in `w65c816`) and BG page sets.
Open: 3.x level-data additions (extended objects 01/03, sprite extension
bytes, the new per-level tables), the bypass table's extra slots, layer 3,
sprites' graphics, and LM-compatible saving.

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

Secondary exits are ROM-global, so the session keeps the edited ones beside
the level (`EditSession::exits`); every command snapshots both, so they share
the level's undo stack and are written on Save.

After every command the `Document` rebuilds its `LevelScene` through a
`SceneCache`: the palette, GFX, Map16 sets and the machine assets are reused
while the level's `SceneKey` (headers, layer 2 page set, Lunar Magic records)
is unchanged, so only the loader runs again for an object or sprite edit. A
key change drops the assets and a `CaptureSession` thread captures them anew
from a `BootCache` the document shares between captures.

## Reverse-engineering tooling

`SnesBus::enable_trace` records every ROM byte read, WRAM byte written, WRAM
byte read before it was written, and hardware register touched. `solar trace
<rom> level:<n>` runs the level loader that way and `solar trace <rom>
<bank:offset> [--ram addr=val]` `JSL`s into any routine, printing the touched
ranges coalesced and labelled by the table they start in (`--unknown-only`
hides the labelled ones). This is how table locations are found before they
are verified and recorded in `docs/formats/`: for example `solar trace rom
04:DC57` shows the overworld layer 1 tilemap being copied from
`$0C:F7DF..$0C:FFDE` to `$7E:C800`.

`RomLayout::known_ranges` is the catalogue behind those labels: every table
the layout names, plus the ones reached through a pointer (tileset Map16
blocks, Lunar Magic's relocated tables). `solar diff-rom a b` compares two
images with it: changed byte ranges labelled `layer1_ptrs[0x105]`-style,
RATS tags added, removed or resized, and (`--markers`) Lunar Magic's `"LM"`
markers (`smw_data::layout::scan_markers`). The Lunar Magic
reverse-engineering loop is: save a before/after ROM pair in Lunar Magic,
`diff-rom` them, `trace` the affected level, record the table in
`docs/formats/lunar-magic-ext.md` with the date, add a fingerprint and a test.

## Rendering pipeline

```
Rom ─RomLayout─▶ Level{headers, objects, layer2, sprites}
                 ─w65c816 runs object routines─▶ TileGrid (L1), TileGrid (L2)
plain:   Map16 + GfxFiles + Palette ─▶ CharAtlas ─▶ Raster (L2 under L1) ─▶ PNG / egui texture
machine: smw-play launches the level ─▶ LevelAssets {VRAM chars, CGRAM, BG3 map, PPU regs}
         TileGrid × captured chars ─▶ IndexPlane per layer ─▶ compose (mode 1 order,
         TM/TS, colour math) ─▶ Raster
Sprites ─▶ markers
```

`docs/formats/level-view.md` describes both pictures and how the GUI
captures on a worker thread. The machine picture is checked against the
emulator's own frame pixel for pixel (`solar diff-view`,
`smw-play/tests/view_oracle.rs`).

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
