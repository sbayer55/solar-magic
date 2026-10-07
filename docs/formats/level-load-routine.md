# Running the game's level loader

Implemented in `crates/smw-render/src/level.rs` on top of the `w65c816`
interpreter and `crates/smw-render/src/emu.rs` (bus). Verified 2026-10-06 by
rendering Yoshi's Island 2 (level 105) and comparing against the game.

## Why

Levels are object lists; the ~0x40 standard, ~0x100 extended and
tileset-specific object routines in bank `$0D` stamp Map16 tiles into RAM.
Instead of porting them, Solar Magic runs them. This also makes Lunar Magic's
patched object code work for free later.

## Entry point and preconditions

`JSL $05:801E` (`CODE_05801E`) does everything needed for a static picture:

1. Fills the background buffer with tile `$25` and, for background levels,
   decodes the RLE stream into it and sets its page high bytes.
2. Clears the level tile RAM: low bytes to `$25`, high bytes to `$00`.
3. `LoadLevel` (`$05:83AC`): decodes the 5-byte header (`CODE_0584E3`), builds
   the Map16 pointer table (`CODE_0581FB`), then runs `LoadLevelData` for
   layer 1 and, for layer-2 modes, layer 2.

RAM the routine expects before the call:

| Address | Meaning | Value Solar Magic writes |
|---|---|---|
| `$65-$67` | layer 1 data pointer | the level's layer 1 pointer |
| `$68-$6A` | layer 2 data pointer | the raw layer 2 pointer (bank `$FF` for backgrounds) |
| `$CE-$D0` | sprite data pointer | the level's sprite pointer (unused by this routine) |
| `$0100` | game mode | `$22` so the trailing `JSL CODE_02A751` (sprite system init) is skipped |
| `$0E-$0F` | level number | the level number; Lunar Magic's layer 2 hook (`$0E:F510`) indexes its flags table with it (verified 2026-10-06) |
| `$010B-$010C` | level number (Lunar Magic's copy) | the level number |

Everything else starts zeroed. No hardware registers are written (the bus
counts and ignores any). Yoshi's Island 2 takes ~115k instructions.

## Reading the result

Tile RAM: low bytes at `$7E:C800`, high bytes at `$7F:C800`. The screen
layout depends on the level mode and comes from the game's own tables:

- `LoadBlkPtrs` (`$00:BEA8`): 2 words (layer 1, layer 2) → per-mode word table
  (`Ptrs00BDA8` / `Ptrs00BDE8`) → per-screen 24-bit RAM pointers (low bytes).
- `LoadBlkTable2` (`$00:BEAC`): same for high bytes.

| Level kind | Per-screen block | Layout |
|---|---|---|
| horizontal | `0x1B0` bytes | 16 columns × 27 rows, row-major; screens left to right (`C800 + s*1B0`) |
| vertical | `0x200` bytes | two 16×16 row-major halves, left at `+0`, right at `+0x100`; screens top to bottom (`C800 + s*200`) |
| layer 2 objects (horizontal) | `0x1B0` | after layer 1: `C800 + 1B00 + s*1B0`, 16 screens |

Background levels: decoded tilemap at `$7E:B900` (low) / `$7E:BD00` (high),
two 16×27 screens at stride `0x1B0`, repeating horizontally
(`CODE_058D7A`). The high byte is 0 or 1 (BG Map16 page).

## Not yet handled

- Layer 2 tile priority bits (layer 2 tiles with priority draw above layer 1).
- Layer 3, HDMA gradients, animated tiles, sprites (M6).
- Lunar Magic: the patched loader runs as is (verified on Quickie World 1/2,
  2026-10-06); Map16 pages 2+ and ExGFX are resolved outside the loader
  (`lunar-magic-ext.md`). SA-1 / ExLoROM ROMs are still rejected.
