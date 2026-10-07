# Map16

Implemented in `crates/smw-data/src/map16.rs`.

Levels are built from 16×16 "Map16" tiles. Each is four 8×8 SNES tilemap
entries stored as 8 bytes in the order **top-left, bottom-left, top-right,
bottom-right**. Each entry is a little-endian word `YXPCCCTT TTTTTTTT`:

| Bits | Field |
|---|---|
| 0–9 | 8×8 character number (0x000–0x3FF) |
| 10–12 | palette row (0–7) |
| 13 | priority |
| 14 | X flip |
| 15 | Y flip |

Example: common tile 0 is `70 1C 72 1C 71 1C 73 1C`: characters 0x70/0x72/0x71/0x73, palette 7.

## Foreground pages 0–1 (tiles 0x000–0x1FF)

On level load, `CODE_0581FB` ($05:81FB) fills a 512-entry pointer table at
`$7E:0FBE` (one word per tile, into bank `$0D`) by walking a 512-bit mask:

| Table | SNES | Size | Meaning |
|---|---|---|---|
| `DATA_0581BB` bitmask | `$05:81BB` | 64 bytes | MSB first. **1 = next tile from the common table, 0 = next tile from the tileset-specific table.** |
| `Map16Common` | `$0D:8000` | 366 tiles | Shared tiles, consumed sequentially (334 by the mask, 8 diagonal pipes at `$0D:8A70`, rest for animation frames). |
| `TilesetMAP16Loc` | `$05:8000` | 15 words | Bank-`$0D` offset of each tileset's 178 specific tiles: `8B70 BC00 C800 D400 E300 E300 C800 8B70 C800 D400 D400 D400 8B70 E300 D400`. |

Tileset-specific tiles are 0x073–0x0FF, 0x107–0x110 and 0x153–0x16D (the
zero bits of the mask). Tilesets 0 and 7 additionally replace 0x1C4–0x1C7 and
0x1EC–0x1EF with eight consecutive tiles from `$0D:8A70` (diagonal pipes).

Tiles 0x133–0x13A are animated by swapping their pointers between four frames
listed at `MAP16AppTable` (`$05:8776`, four bank-`$0D` offsets `8AB0 84E0 8AF0
8B30`). Solar Magic renders the first frame.

Verified 2026-10-06: tile 0 of every tileset is `70/72/71/73`; tile 0x73 of
tileset 0 is `C5 15 …` (first `Map16Tileset0` entry); tilesets 0 and 1 agree
on 0x100 and differ on 0x73 and 0x1C4.

## Background tiles

When layer 2 is a background tilemap, all 512 pointers are set sequentially
into `Map16BGTiles` (`$0D:9100`, 0x1000 bytes). Lunar Magic shows these as
pages 0x10–0x11. Tile 0 is `F8 00` ×4 (blank).

## Acts-like / behaviour

Vanilla has no per-tile "acts like" table; collision behaviour is decided by
tile-number ranges in the collision code. Lunar Magic adds an explicit table
for its expanded Map16. Deferred to the Lunar Magic compatibility work.
