# Layer 2 data

Parsing implemented in `crates/smw-data/src/level/layer2.rs`; drawing in
`smw-render` (M5). Decoder in the game: `CODE_05801E` / `CODE_058126`
($05:801E, $05:8126). Verified against the SMWDisX disassembly 2026-10-06.

## Pointer table

`$05:E600`: 512 × 3-byte pointers.

- **Bank byte `$FF`**: the level has a *background tilemap*. The 16-bit offset
  is taken in bank `$0C` (the game overwrites the bank byte with `$0C`).
- **Any other bank**: the pointer addresses a layer 2 *object stream* in the
  same format as layer 1 (see `level-objects.md`), without a 5-byte header.

## Background tilemap stream

A run-length stream decoded into 0x400 bytes at `$7E:B900`
(`Layer2TilemapLow`), pre-filled with `$25`:

```
control byte c:
  c & 0x80 == 0 : literal run,  (c & 0x7F) + 1 bytes follow and are copied
  c & 0x80 != 0 : repeat run,   one byte follows and is written (c & 0x7F) + 1 times
end            : two consecutive 0xFF bytes where a control byte is expected
```

Each output byte is a Map16 tile number in the *background set*
(`Map16BGTiles`, `docs/formats/map16.md`). The high byte for every tile is 0
if the pointer offset is below `$E8FE`, else 1 — i.e. backgrounds stored at
or above `$0C:E8FE` use BG tiles 0x100–0x1FF (Lunar Magic's page 0x11), the
rest use 0x000–0x0FF (page 0x10).

The 0x400 bytes cover two screens of background that repeat horizontally:
screen 0 at offset 0, screen 1 at offset `0x1B0`, each 16 columns × 27 rows
row-major (`CODE_058D7A`). Verified 2026-10-06 by rendering level 105.

Background levels force the FG/BG tileset used for *layer 2 Map16 lookups* to
0 (`ObjectTileset` is zeroed before decoding) but the level's own tileset still
applies to layer 1.

## Lunar Magic backgrounds

Verified 2026-10-06 against both Quickie World ROMs (`lunar-magic-ext.md`).
The loader hook at `$0E:F510` reads the level's flags byte from `$0E:F310`:

| Pointer bank | Flags bit 1 | Flags bit 2 | Layer 2 is |
|---|---|---|---|
| `$FF` | — | — | vanilla background in bank `$0C` (as above); flags treated as 0 |
| other | clear | — | an object stream |
| other | set | clear | an 8-bit tilemap at the pointer's real address; every tile's high byte = flags >> 4 |
| other | set | set | a **16-bit tilemap**: the RLE stream decodes to 0x800 bytes, low bytes first (`$7E:B900-BCFF`) then high bytes (`$7E:BD00-C0FF`); each screen is 16 × **32** rows (stride 0x200); flags >> 4 selects the BG page set (`map16.md`) |

Only flags `00` and `06` occur in the two ROMs (32 and 5 sixteen-bit
backgrounds). `Layer2::Background` records `rows_per_screen` and `page_set`;
the compressed stream re-serializes byte for byte.
