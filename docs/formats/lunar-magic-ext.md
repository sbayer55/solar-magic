# Lunar Magic extensions

What Lunar Magic (LM) adds to a ROM, and where Solar Magic reads it.
Addresses live in `smw_data::layout::LM_25X` (`crates/smw-data/src/layout/lunar_magic.rs`).

**Verified 2026-10-06** by byte inspection and disassembly of two LM-saved
ROMs: Quickie World 1.12 (LM 2.52) and Quickie World 2 (LM 2.53). Every
address below is identical in both. Other LM versions are untested;
`RomLayout::detect` only accepts a ROM whose routines carry the fingerprints
listed at the end, and reports the first missing one otherwise.

## Container

Both ROMs are plain 2 MiB LoROM (map mode `$20`); banks `$10-$3F` map through
`snes_rom::LoRom` unchanged. Every block LM inserts in expanded space sits
right after an 8-byte RATS tag (`docs/formats/rats.md`); the tag's length is
exactly the block length, which the test `lm_rom_rats_oracle` uses to pin
every object-length rule (`LM_ROM=<path> cargo test -p smw-data`).

## Detection

| Address | Content |
|---|---|
| `$0F:F0A0` | ASCII `"Lunar Magic Version 2.53 Public ©2018 FuSoYa, Defender of Relm"` |

Solar Magic searches the image for `Lunar Magic Version` rather than trusting
the offset; the string sits at `$0F:F0A0` in both ROMs.

## Levels

| Table | Address | Format |
|---|---|---|
| Layer 1 / layer 2 / sprite pointer tables | vanilla (`$05:E000`, `$05:E600`, `$05:EC00`) | Edited levels point into banks `$10+`; untouched levels still point into `$06`/`$07`. |
| Sprite data bank bytes | `$0E:F100` | 0x200 bytes, one bank per level (`$07` for unedited levels). Used as the bank of the 2-byte sprite pointer. |
| Layer 2 flags | `$0E:F310` | 0x200 bytes. Bit 1: the layer 2 pointer is a background tilemap at its real bank. Bit 2: that tilemap is 16-bit (see `layer2.md`). Bits 4-7: BG page / page set. Only `00` and `06` occur in the two ROMs. |
| Layer 2 hook | `$0E:F510` (from `$05:8039`) | Replaces vanilla's `LDA $6A / CMP #$FF`. Bank `$FF` → vanilla bank-`$0C` path with flags 0; else `LDX $0E` (level number), copies the flags byte to `$7F:C00B`, and dispatches on bits 1 and 2. |
| Secondary header bytes 4/5/6 | `$05:DE00`, `$06:FC00`, `$06:FE00` | 0x200 bytes each. Byte 4 holds `00/20/28/30/38/78` in the two ROMs; bytes 5-6 are `FF`. Semantics not decoded; kept raw in `LmLevelInfo::secondary_ext`. |
| Custom palette pointers | `$0E:F600` | 0x200 × 24-bit. `000000` = none; all `FFFFFF` = the ROM has no custom palettes. Block format in `palettes.md`. |

Object stream additions (direct Map16 objects, 5-byte exit) are in
`level-objects.md`; the loader precondition (`$0E` = level number) in
`level-load-routine.md`.

## Graphics

| Table | Address | Format |
|---|---|---|
| GFX00-33 pointers | vanilla split tables | Still used, but LM re-stored every file (several in bank `$1E`) as LC_LZ2 **4bpp** (0x1000 bytes) instead of 3bpp. |
| ExGFX 80-FF pointers | `$0F:F600` | 0x80 × 24-bit; `000000` = not inserted. LC_LZ2; ExGFX80 in QW2 decompresses to 0x2000 bytes (256 tiles). |
| ExGFX 100-FFF pointer table | `read3($0F:F937)` (= `$12:8008` here) | 0xF00 × 24-bit. |
| Per-level "Super GFX Bypass" | `read3($0F:F7FF)` (= `$12:AD08` here) | 32 bytes (16 LE words) per level. Words 4-7 = FG3, BG1, FG2, FG1; 8-11 = SP4, SP3, SP2, SP1 (verified against the vanilla tileset lists); 12-15 = GFX 2B, 2A, 29, 28; 0-3 = extra slots (not yet decoded). `0x7F` = no file, `0xFFFF` = never saved, low 12 bits = file number, high bits = flags; word 0 bit 15 = bypass enabled. |
| GFX decompression routine | `$0F:F900` | LM's own; replaces `$00:B8DE`. Not needed by Solar Magic. |

## Map16

| Table | Address | Format |
|---|---|---|
| FG pages 0-1 | vanilla `Map16Tables` in bank `$0D` | Unchanged mechanism; LM edits in place. |
| FG pages 2-0x7F getter | `$06:F540` | `JSL`, 16-bit A/X, A = tile × 2 → A = offset, bank in `$0C` (`STY $0B` with Y = bank << 8). Caller does `STA $0A; LDA [$0A],y`. A per-ROM template: one `ADC #imm / LDY #imm` pair per 16-page group (QW2: `$17:8000` for tiles 0x200+, QW1: `$12:EE10`); `LDY #$0000` = group not allocated. Blocks are RATS-sized to the last used tile. Tile record = vanilla 8 bytes. |
| BG page set getter | `$0E:FD00` (from `$05:8DA4`) | Reads `$7F:C00B` bit 2; index = flags >> 4 into `$0E:FD50`; returns the page set pointer in `$0A-$0C` and the per-screen stride (0x1B0 or 0x200) in `$05`. |
| BG page set pointers | `$0E:FD50` | 24-bit entries, 16 pages (0x8000 bytes max) each; entry 0 = pages 0x10-0x1F (`$26:8000` / `$13:8000`), whose first 0x1000 bytes equal vanilla `$0D:9100`. |
| Map16 "acts like" table | `$11:8000` (via `$06:F600`) | 2 bytes per tile, identity by default. Not needed for viewing. |

## Markers

Verified 2026-10-08 against Quickie World 2 (2.53, 19 markers) and an LM 3.71
save of the vanilla ROM (26 markers); the vanilla ROM has none. Lunar Magic
stamps a 4-byte marker `"LM" kind revision` (both bytes below `0x20`,
revision non-zero) in two places:

- immediately before each 16-byte-aligned routine it installs in the fixed
  banks (`$06:F5FC` before the acts-like getter, `$0D:E1AC`/`$0D:E1CC`/
  `$0D:E1EC` before extended objects 02/01/03, `$0E:F54C`, `$0E:FD3C`, …);
- as the last four bytes of the RATS blocks holding its own code and tables
  (padded with `0x20`); level-data blocks carry none.

Kinds seen: `00` (code), `01`, `02`, `03`, `10`, `11`; revisions `01` and
`02`. `smw_data::layout::scan_markers` finds them, `solar info` counts them
and `solar diff-rom --markers` lists them. Their exact meaning is not decoded.

## Lunar Magic 3.71 (first observations, 2026-10-08)

From `solar diff-rom vanilla a-unchanged.sfc` (an LM 3.71 save with level
105 untouched) and byte probes; not yet supported by `RomLayout::detect`:

- Five of the six 2.5x fingerprints match. The layer 2 flags hook body at
  `$0E:F510` differs: `38 E9 7F D0 08 8F 0B C0 7F …` (`SEC; SBC #$7F`)
  instead of `C9 FF D0 09 1A …`.
- **Every vanilla `$FF` bank byte in the layer 2 pointer table is rewritten
  to `$0C`** (the real bank), and the flags byte at `$0E:F310` carries `08`
  for a page-0 background, `18` for page 1 (offset ≥ `$E8FE`) and `00` for
  object layer 2. So in 3.x bit 3 means "tilemap in bank `$0C`" and the
  page lives in bits 4–7; bits 1 (relocated) and 2 (16-bit) keep their 2.5x
  meaning.
- `$05:DE00` (2.5x secondary header byte 4) is zeroed; `$06:FA00` holds
  `20` per level, `$06:FC00` `00`, `$06:FE00` `1A`. `$05:FF00–FFFF` (the
  upper half of the fourth vanilla secondary-exit table, unused by vanilla)
  is written, as are unused secondary exits above `0xCE` (zeroed).
- Extended objects 01 and 03 have code at `$0D:E1D0` / `$0D:E1E0`; objects
  0x24–0x26 are null routines in 3.71.
- The unchanged level 105 is re-saved: layer 1 at `$10:8008`, sprites at
  `$10:8640` with `$0E:F100[0x105] = $10`; the background stays at
  `$0C:D900`. Thirteen RATS blocks in total, the last (32 KiB at
  `0x087FF8`) holding LM's own code bank.

## Other known extensions (not handled)

Expanded overworld, layer 3 editing, custom overworld sprites (`$0E:F55D`),
SA-1 and ExLoROM mappings, `.mwl` export, LM 3.x table moves and its
optional LC_LZ3 compression.

## Fingerprints

Bytes `RomLayout::detect` requires (all `FF` in vanilla):

| Address | Bytes | What |
|---|---|---|
| `$06:F540` | `C9 00 04 90 74` | FG Map16 getter prologue |
| `$0E:FD00` | `08 C2 30 A2 00 00 A0 B0 01 AF 0B C0 7F` | BG page set getter prologue |
| `$0E:F510` | `C9 FF D0 09 1A 8F 0B C0 7F` | layer 2 flags hook |
| `$05:8039` | `A5 6A 5C 10 F5 0E` | loader jump to that hook |
| `$05:8DA4` | `22 00 FD 0E EA` | BG page set call |
| `$0D:A4C7` | `50 F1 0D` | dispatch entry of direct Map16 object 0x27 |
