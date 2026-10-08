# Level header

Implemented in `crates/smw-data/src/level/header.rs`. Decoder in the game:
`CODE_0584E3` ($05:84E3), verified against the SMWDisX disassembly 2026-10-06.

## Pointer tables

| Table | SNES | Entry | Notes |
|---|---|---|---|
| Layer 1 | `$05:E000` | 3-byte pointer × 512 | points at the 5-byte header followed by the layer 1 object stream |
| Layer 2 | `$05:E600` | 3-byte pointer × 512 | bank byte `$FF` ⇒ background tilemap in bank `$0C` (see `layer2.md`), else object stream |
| Sprites | `$05:EC00` | 2-byte pointer × 512 | bank `$07` implied |

Verified: level 0's layer 1 pointer is `$06:8654`.

Writing (`crates/smw-data/src/level/save.rs`): the three entries are rewritten
when a part moves; a background keeps bank `$FF` in its layer 2 entry; the
sprite entry stores only the offset unless the ROM has Lunar Magic's bank
table. Several vanilla levels share one data block through identical
pointers, so a save checks `pointer_refs` before writing in place or freeing.

## Primary header (5 bytes at the start of layer 1 data)

| Byte | Bits | Field | Notes |
|---|---|---|---|
| 0 | 0–4 | screens − 1 | number of screens = value + 1 |
| 0 | 5–7 | background palette set | selects `BackgroundPalettes[n]` |
| 1 | 0–4 | level mode | indexes `VerticalTable`, `LevMainScrnTbl`, `LevSubScrnTbl`, `LevCGADSUBtable`, `SpecialLevTable`, `LevXYPPCCCTtbl` |
| 1 | 5–7 | back area colour | selects `BackAreaColors[n]` |
| 2 | 0–3 | sprite set | selects 4 sprite GFX files via `SPRITEGFXLIST` |
| 2 | 4–6 | music | indexes `LevelMusicTable` |
| 2 | 7 | layer 3 priority | |
| 3 | 0–2 | foreground palette set | selects `ForegroundPalettes[n]` |
| 3 | 3–5 | sprite palette set | selects `SpriteColors[n]` |
| 3 | 6–7 | time limit | `TimerTable`: 0, 200, 300, 400 |
| 4 | 0–3 | FG/BG tileset | selects Map16 specific tiles and 4 GFX files via `OBJECTGFXLIST` |
| 4 | 4–5 | horizontal/vertical scroll setting | |
| 4 | 6–7 | item memory setting | |

Level mode (`VerticalTable` at `$05:D...`): bit 0 of the entry marks a
vertical level; even/odd mode also decides whether the screen count applies
horizontally or vertically.

## Secondary header

Four byte tables indexed by level number (decoded around `$05:D8B7`):

| Table | Bits | Field | Notes |
|---|---|---|---|
| `$05:F000` | 0–3 | entrance Y position index | pixel Y from `DATA_05D730` (low) / `DATA_05D740` (high) |
| `$05:F000` | 4–7 | layer 2 scroll setting | indexes `DATA_05D720` (horizontal) / `DATA_05D710` (vertical) |
| `$05:F200` | 0–2 | entrance X position index | pixel X from `DATA_05D750` / `DATA_05D758` |
| `$05:F200` | 3–5 | entrance type | 0 none, others door/pipe animations |
| `$05:F200` | 6–7 | layer 3 setting | |
| `$05:F400` | 0–1 | initial layer 2 Y | `DATA_05D70C`: `60 90 C0 00` |
| `$05:F400` | 2–3 | initial layer 1 Y | `DATA_05D708`: `00 60 C0 00` |
| `$05:F400` | 4–7 | midway entrance screen | |
| `$05:F600` | 0–4 | main entrance screen | |
| `$05:F600` | 5–6 | screen mode bits | vertical level / layer 2 flags |
| `$05:F600` | 7 | disable "No Yoshi" intro | |

Implemented in `crates/smw-data/src/level/header.rs` (`SecondaryHeader`).

## Secondary exits

Four 512-entry byte tables indexed by secondary exit number (0x000–0x1FF).
Decoded by the game at `$05:D7D5`–`$05:D83B` (the `UseSecondaryExit` branch
of the level-entry routine); verified against the SMWDisX disassembly
2026-10-07. Implemented in `crates/smw-data/src/level/exits.rs`.

| Table | Bits | Field | Notes |
|---|---|---|---|
| `$05:F800` | 0–7 | destination level, low byte | the high bit comes from the current submap in vanilla (`$0F` = 1 when not on the main map); Lunar Magic stores it elsewhere |
| `$05:FA00` | 0–3 | entrance Y position index | `DATA_05D730` / `DATA_05D740`, same as the main entrance |
| `$05:FA00` | 4–5 | initial layer 1 Y | `DATA_05D708` |
| `$05:FA00` | 6–7 | initial layer 2 Y | `DATA_05D70C` |
| `$05:FC00` | 0–4 | entrance screen | |
| `$05:FC00` | 5–7 | entrance X position index | `DATA_05D750` / `DATA_05D758` |
| `$05:FE00` | 0–2 | entrance type | as the main entrance |
| `$05:FE00` | 3–7 | unused in vanilla | Lunar Magic uses them (slippery/water flags, `$05:DD00`) |

Lunar Magic adds further header bytes at `$05:DE00`, `$06:FC00`, `$06:FE00`.
