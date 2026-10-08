# Palettes

Implemented in `crates/smw-data/src/palette.rs`.

## Colour format

SNES colours are 15-bit little-endian words `0bbbbbgggggrrrrr` (BGR555).
Solar Magic expands each 5-bit channel to 8 bits with `(v << 3) | (v >> 2)`.

## CGRAM layout

The game mirrors CGRAM at `$7E:0703` ("MainPalette"): 16 rows × 16 colours,
colour `(row, col)` at byte offset `row*0x20 + col*2`. Rows 0–7 are used by
layers (background rows 0–1, foreground rows 2–3, shared 4–7), rows 8–15 by
sprites (row 8 is the player). Colour 0 of each row is transparent; CGRAM
colour 0 (row 0, col 0) is the back area colour.

## How a level palette is assembled

`LoadPalette` at `$00:ABED` fills the image from these tables (all in bank
`$00`; addresses from the SMWDisX symbol file, verified 2026-10-06 by reading
them and checking that different set selections change only their rows):

| Step | Source | SNES | Bytes | Destination |
|---|---|---|---|---|
| 1 | constant `$7FDD` | | | colour 1 of rows 0–7 |
| 2 | constant `$7FFF` | | | colour 1 of rows 8–15 |
| 3 | `StatusBarColors` | `$00:B170` | 32 | rows 0–1, colours 8–15 (layer 3 / status bar) |
| 4 | `StandardColors` | `$00:B250` | 120 | rows 4–13, colours 2–7 |
| 5 | `BackAreaColors[back_area]` | `$00:B0A0` | 2 of 16 | row 0, colour 0 |
| 6 | `ForegroundPalettes[fg]` | `$00:B190` | 24 of 192 | rows 2–3, colours 2–7 |
| 7 | `SpriteColors[sp]` | `$00:B318` | 24 of 192 | rows 14–15, colours 2–7 |
| 8 | `BackgroundPalettes[bg]` | `$00:B0B0` | 24 of 192 | rows 0–1, colours 2–7 |
| 9 | `BerryColors` | `$00:B674` | 42 | rows 2–4, colours 9–15 |
| 10 | `BerryColors` again | `$00:B674` | 42 | rows 9–11, colours 9–15 |
| 11 | `PlayerColors[player]` | `$00:B2C8` | 20 of 80 | row 8, colours 6–15 |

Set indices `fg`, `bg`, `sp` (0–7) come from the level header; each set is 12
colours (24 bytes), selected via the offset table `DATA_00ABD3`
(`00 18 30 48 60 78 90 A8`). Player palettes are 10 colours (20 bytes) each:
Mario, Luigi, fire Mario, fire Luigi. Row 8 is loaded by the player-palette
code rather than `LoadPalette` itself; Solar Magic loads Mario by default.

Cells no step touches (e.g. rows 5–7 colours 8–15) stay black; the game
leaves them at whatever was in RAM too.

Lunar Magic adds per-level custom palettes (full 16×16 images) at `$0E:F600`;
see `lunar-magic-ext.md`.

## Lunar Magic custom palettes

Verified 2026-10-06 against both Quickie World ROMs (`lunar-magic-ext.md`).
`$0E:F600` holds one 24-bit pointer per level (`000000` = none; all `FFFFFF`
= the ROM has no custom palettes). Each pointer names a RATS-tagged 0x202-byte
block:

| Offset | Size | Content |
|---|---|---|
| 0 | 2 | back area colour (upload routine `$0E:F570` stores it at `$7E:0701`) |
| 2 | 512 | colours 0–255, row-major, copied to `$7E:0703` |

Colour 1 is not forced to `$7FDD` (levels override it). Implemented by
`palette::load_custom_palette`; `palette::level_palette` picks it over the
vanilla assembly when `LmLevelInfo::custom_palette` is set.
