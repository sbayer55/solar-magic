# Overworld level names

Implemented in `crates/smw-data/src/level/names.rs` (`OverworldNames`),
addresses in `RomLayout::level_names` (`LevelNameTables`). The drawing
routine is `CODE_049D07` ($04:9D07), called from the overworld with the
translevel under the player; verified against the SMWDisX disassembly
(`bank_04.asm`, `SMW_U.sym`) and the USA ROM on 2026-10-15.

The name shown in the overworld border is not stored as one string. Each
translevel has a two-byte entry that picks a word from each of three word
tables; the words are runs of layer 3 font tiles.

## Tables

| Table | SNES | Size | Notes |
|---|---|---|---|
| `LevelNames` | `$04:A0FC` | 2 bytes × `0x5D` | one entry per translevel `$00`-`$5C`; `$04:A1B6` onward is free space (`0xFF`) |
| `LevelNameStrings` | `$04:9AC5` | `0x1CC` bytes | tile strings; a string ends at the first byte with bit 7 set, which is still drawn |
| first words | `$04:9C91` | 31 × u16 | offsets into `LevelNameStrings` |
| second words | `$04:9CCF` | 15 × u16 | offsets |
| third words | `$04:9CED` | 13 × u16 | offsets; the routine follows at `$04:9D07` |

Entry `(byte0, byte1)` at `LevelNames + 2 × translevel`:

| Field | Bits | Table |
|---|---|---|
| first word | `byte1 & 0x7F` | `$04:9C91` |
| second word | `byte0 >> 4` | `$04:9CCF` |
| third word | `byte0 & 0x0F` | `$04:9CED` |

Entry 0 of every table is offset `0x1CB`, a lone terminated space: the blank
word. The routine skips a blank first or second word (`BMI` on the first
byte, `CMP #$9F` on the second) and always draws the third. After the three
words it pads with blank words until `0x26` tilemap bytes (19 tiles) are
written, and `CODE_049D7F` drops every tile past that limit. Each tile is
written with property byte `$39` (palette 6, priority, tile `0x100+`).

Translevel: the slot number for `0x000-0x024`; for `0x101-0x13B` it is the
slot minus `0xDC`. The overworld builds the level number the other way round
at `CODE_05D8A2` ($05:D8A2): a translevel `>= 0x25` has `0x24` subtracted and
the high byte set when the player is on a submap (`LevelId::translevel`).

## Font

The tiles are the overworld layer 3 font (GFX2A, 2bpp), read off the dumped
sheet (`solar dump-gfx`) on 2026-10-15:

| Tiles | Text |
|---|---|
| `0x00-0x19` | `A`-`Z` |
| `0x1A-0x1F` | `!` `.` `-` `,` `?` space |
| `0x32-0x37` | ` ILLUSI` squeezed into 6 tiles, the gap after OF drawn inside the first (FOREST OF ILLUSION) |
| `0x38-0x3C` | `YELLOW` squeezed into 5 tiles (YELLOW SWITCH PALACE) |
| `0x40-0x59` | `a`-`z` (unused by vanilla names) |
| `0x5A`, `0x5B`, `0x5C`, `0x5D` | `#` `(` `)` `'` |
| `0x64-0x6B` | `1`-`7`, `0` |

The decoder prints `_` for any other tile.

## Verified examples (USA ROM)

| Slot | Translevel | Entry | Name |
|---|---|---|---|
| `105` | `0x29` | `21 01` | YOSHI'S ISLAND 1 (`YOSHI'S ` + `ISLAND ` + `1`) |
| `104` | `0x28` | `10 01` | YOSHI'S HOUSE |
| `101` | `0x25` | `40 03` | #1 IGGY'S CASTLE (`#` is tile `0x5A`, `1` is `0x64`) |
| `003` | `0x03` | `00 0C` | TOP SECRET AREA (one first-table word) |
| `014` | `0x14` | `33 0E` | YELLOW SWITCH PALACE: the entry also selects third word `3`, which falls past the 19-tile limit and is never drawn |
| `11E` | `0x42` | `A1 15` | FOREST OF ILLUSION 1, exactly 19 tiles: `FOREST ` + `OF`+squeezed+`ON ` + `1` |
| `125` | `0x49` | `0B 00` | FUNKY: special-world names are third-table words alone |

Translevels `0x5D-0x5F` (slots `139`-`13B`) have no entry: the bytes there
are free space, and the decoder reports no name rather than following an
index past the tables.

## Lunar Magic

The layout keeps the vanilla addresses for Lunar Magic ROMs. Before reading,
`OverworldNames::read` checks the routine's fingerprint at `$04:9D28`
(`LDA $01 / AND #$007F / ASL / TAY / LDA $9C91,Y / TAY / SEP #$20 /
LDA $9AC5,Y`); a ROM whose level-name system was replaced fails the check and
yields no names, so the editor falls back to its sidecar names.
