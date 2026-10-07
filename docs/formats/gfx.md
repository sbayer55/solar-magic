# GFX files

Implemented in `crates/smw-data/src/gfx.rs`; compression in `crates/lc-lz2`.

SMW's graphics are 8×8 tiles in the SNES planar formats, stored LC_LZ2
compressed in banks `$08-$0B`. Lunar Magic numbers them GFX00–GFX33.

## Pointer tables (GFX00–GFX31)

Three parallel 50-entry byte tables in bank `$00`, indexed by file number:

| Table | SNES | PC |
|---|---|---|
| low bytes | `$00:B992` | `0x3992` |
| high bytes | `$00:B9C4` | `0x39C4` |
| bank bytes | `$00:B9F6` | `0x39F6` |

Verified 2026-10-06 by decompressing every entry: GFX00 → `$08:D9F9`, GFX01 →
`$08:E231`, GFX02 → `$08:ECBB`, …, GFX31 → `$0B:E936`. (Community notes giving
`$00:B9F4`/`$00:BA56` for the high/bank tables are wrong for the USA ROM.)

GFX32 and GFX33 are not in this table. Candidate blocks found by scanning:
`$08:8000` (decompresses to 0x5D00 bytes) and `$08:BFC0` (0x2400 bytes). To be
identified when Mario and animated tile rendering is needed.

## Decompressed sizes and formats

| Decompressed size | Format | Tiles | Vanilla files |
|---|---|---|---|
| `0x400` | 2bpp (16 bytes/tile) | 64 | GFX2F (credits font) |
| `0x800` | 2bpp (16 bytes/tile) | 128 | GFX28–GFX2B (layer 3) |
| `0xC00` | 3bpp (24 bytes/tile) | 128 | everything else in GFX00–GFX31 |
| `0x1000` | 4bpp (32 bytes/tile) | 128 | GFX32/GFX33 class files |

Format is inferred from size, as Lunar Magic does. Verified 2026-10-06: 45
files decode to `0xC00`, four (GFX28–2B) to `0x800`, GFX2F to `0x400`.

## Which files a level loads

| Table | SNES | Layout |
|---|---|---|
| `OBJECTGFXLIST` | `$00:A92B` | 4 bytes per FG/BG tileset (15 level tilesets + overworld/title rows): `FG1 FG2 BG1 FG3` |
| `SPRITEGFXLIST` | `$00:A8C3` | 4 bytes per sprite set (16): `SP1 SP2 SP3 SP4` |

Tileset 0 ("Normal 1") is `14 17 19 15`; sprite set 0 ("Forest") is `00 01 13 02`.
Implemented in `crates/smw-data/src/tileset.rs`, verified 2026-10-06.

## VRAM character layout (what an 8×8 tile number means)

`UploadGFXFile` (`$00:AA5F`) converts each 3bpp file to 4bpp and uploads it to
VRAM word address `0x0000`, `0x0800`, `0x1000`, `0x1800` for `FG1`, `FG2`,
`BG1`, `FG3` respectively (table `DATA_00A9D6`). So layer tilemap character
`N` comes from file `N >> 7`, tile `N & 0x7F`, in that order; characters
0x200–0x3FF are not covered by level files (animated tiles are DMA'd there
each frame; see `level-load-routine.md` later). Sprite files go to
`0x6000`, `0x6800`, `0x7000`, `0x7800` for `SP1`–`SP4`.

Upload quirk: for GFX01 and GFX17 only, tiles 0x6E, 0x6F, 0x7E and 0x7F are
written with bitplane 3 forced on wherever any lower plane is set, so their
non-zero pixels use palette colours 8–15 (the berry/Yoshi-coin colours).
`smw_render::CharAtlas::load_slot` reproduces this.

## Planar tile layouts

Pixel `x` of row `y` takes bit `7-x` from each plane byte.

- **2bpp**: bytes `[2y]` = plane 0, `[2y+1]` = plane 1 for `y` in 0..8.
- **3bpp**: as 2bpp for the first 16 bytes, then byte `[16+y]` = plane 2.
- **4bpp**: as 2bpp for the first 16 bytes, then `[16+2y]` = plane 2,
  `[16+2y+1]` = plane 3.

Decoded tiles are stored as 64 palette indices (0–3, 0–7 or 0–15). Index 0 is
transparent when drawn over another layer.
