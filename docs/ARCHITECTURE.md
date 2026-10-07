# Architecture

Solar Magic is a Rust reimplementation of Lunar Magic, the Super Mario World
level editor. This document is the map; the format details live in
[`formats/`](formats/).

## Crate graph

```
solar-magic (eframe GUI)          solar-cli (`solar`, PNG/dump tools)
        │                                   │
        └──────────────┬────────────────────┘
                       ▼
                  smw-render        Level → Map16 grid → RGBA raster
                       │
                       ▼
                   smw-data         SMW formats: RomLayout, headers, objects,
                       │            sprites, Map16, GFX, palettes
        ┌──────────────┼──────────────┐
        ▼              ▼              ▼
    snes-rom        lc-lz2         w65c816
  ROM container,  compression   65c816 interpreter that runs the
  LoROM mapping,                game's own object routines
  header, RATS
```

Dependencies only point downward. Nothing below `solar-magic` knows about egui.
`xtask` is a workspace member that implements repository checks (`cargo xtask check`).

| Crate | Status | Responsibility |
|---|---|---|
| `snes-rom` | done | `Rom`, `SnesAddr`, `Mapper`/`LoRom`, `InternalHeader`, `rats::scan`. No game knowledge. |
| `smw-data` | M0–M3, M8 done | `RomLayout` (vanilla tables plus Lunar Magic 2.5x's `LunarMagicTables`), `LevelId`, `gfx`, `palette`, `map16`, `tileset`, `level` (headers, objects including Lunar Magic's, layer 2 including 16-bit tilemaps, sprites; byte-exact re-serialization). |
| `lc-lz2` | decompress done | LC_LZ2 decompress; compress later. |
| `w65c816` | done | Minimal CPU interpreter with a `Bus` trait. No timing, no PPU. Passes the SingleStepTests 65816 native-mode suite (run with `W65C816_TESTS_DIR`). |
| `smw-render` | M2–M6, M8 done | `Raster`, `CharAtlas`, tile drawing; `emu::SnesBus` + `level::load_level` run the game's loader and read back `TileGrid`s; `scene::LevelScene` composes layers, background and sprite markers for both the CLI and the GUI. |
| `solar-cli` | `info`, `dump-gfx`, `dump-map16`, `dump-level`, `render` | One subcommand per milestone; also the test oracle. |
| `solar-magic` | M7 done | eframe app with egui_dock panels: level list, zoomable level view (`egui::Scene`), info, Map16 sheet, palette. Remembers the last ROM and level. |

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

## Rendering pipeline (planned)

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
- Round-trip: every parsed level re-serializes to the original bytes (M3).
- Golden grids: `tests/golden/<level>.grid` dumped from an emulator's
  `$7EC800`/`$7FC800` after loading the level; compared cell-for-cell (M4).
- ROMs, disassemblies and Lunar Magic binaries are never committed.
