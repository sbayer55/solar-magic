# `.mwl` level files

Implemented in `crates/smw-data/src/formats/{mwl,mwl_level}.rs`. The
container was decoded from Lunar Magic 3.71's export of the unchanged
vanilla level 105 (`roms/lm371/a-unchanged-105.mwl`) and verified against
the 3.71 ROM on 2026-10-08; the field meanings follow the community
description of the format (SMW Central's "MWL File Format", v3.63 revision)
where the sample could confirm them. Solar Magic's export of level 105 equals
Lunar Magic's byte for byte (`lm3_export_matches_lm_export`), importing it
reproduces the level (`lm3_import_reproduces_level`), and every vanilla
level survives export then import (`vanilla_levels_round_trip_through_mwl`).

## Container

```
0x00  "LM"
0x02  u16 LE  Lunar Magic version (0x0371 = 3.71)
0x04  u32 LE  offset of the pointer table (0x40)
0x08  u32 LE  length of the pointer table (0x40)
0x0C  u32 LE  flags (bit 0: exported from Super Mario Advance 2)
0x10  48      comment (no effect)
0x40  8 × { u32 offset, u32 length }   one per section
0x80  sections, back to back
```

Sections 2–7 start with an 8-byte prefix `{ flags, 0, 0, 0, addr24, 0 }`
where `addr24` is the source ROM address (informational only).

| # | Section | Contents |
|---|---|---|
| 1 | level info (0x40) | `u16` level; secondary header bytes (`$05:F000/F200/F400/F600`) then `$05:DE00`; 2 unused; 4 midway bytes (location unknown, written as 0) + 1 unused; `$06:FC00`; `$06:FE00`; the 3.x dimension byte (`read3(read3($05:D9A2)+0x70)[level]`); `$06:FA00`; padding |
| 2 | layer 1 | prefix flags bit 0 = custom palette in use; primary header + object stream as in ROM |
| 3 | layer 2 | objects: header copy + stream as in ROM (flags 0). Backgrounds: flags = the `$0E:F310` byte **with bit 2 set** (the file always holds a 16-bit tilemap), body = 2 screens × 32 rows × 16 columns of `u16 LE` tiles (page in the high byte), rows beyond a 27-row tilemap zero |
| 4 | sprites | sprite header + list as in ROM (3 bytes per sprite unless the ROM has 3.x extension sizes, not handled yet) |
| 5 | palette | prefix address = custom palette block or 0; 256 colours as `u16 LE` BGR555 (colour 0 written as 0) then the back area colour |
| 6 | secondary exits | 8 zero bytes, then 8 per exit **whose destination is this level**: `u16` exit, the `$05:FA00/FC00/FE00` bytes, the two 3.x extra bytes (`read3($05:DC86)[exit]`, `read3($05:DC8B)[exit]`), 0 |
| 7 | ExAnimation | prefix flags = `$03:FE00[level]`; body = per-level ExAnimation data (not decoded; written empty) |
| 8 | GFX bypass (0x20) | the 16 bypass words in ROM order: AN2, LT3, BG3, BG2, FG3, BG1, FG2, FG1, SP4, SP3, SP2, SP1, LG4, LG3, LG2, LG1; a level without a row gets `7F ×8, FFFF, 7F ×3, 2B 2A 29 28` |

The secondary exit destination's bit 8 is bit 3 of the `$05:FE00` byte in
Lunar Magic ROMs (vanilla takes it from the submap at run time): in 3.71 exit
`0x1CB` holds `0E` there and is exported with level 105, while exit `0xF4`
(`06`) is not.

## Import

`import_level` builds a `Level` for a slot: the object streams are parsed in
the target ROM's object format, an 8-bit background is re-encoded with
`encode_bg_rle` (so its stored bytes differ from the source ROM's stream
while the tiles are identical), a tilemap whose tiles span several pages is
imported as a 16-bit background and refused for vanilla layouts. The
palette, secondary exits, bypass row and ExAnimation flag come back
separately (`MwlImport`); `solar mwl-import` writes the level and the exits,
and reports a custom palette it cannot write yet (LM10).

## Commands

```
solar mwl-info <file.mwl>
solar mwl-export <rom> <level> <out.mwl>
solar mwl-import <rom> <out.sfc> <file.mwl> [--level N]
solar export-all <rom> <dir> [--prefix "level "]
solar import-all <rom> <out.sfc> <dir>
```
