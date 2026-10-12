# Launching the game into a level

How `smw-play` sends a booted Super Mario World into a chosen level without
patching the ROM. Addresses live in `smw_data::RomLayout::launch`
(`LaunchHooks`). Every address below was resolved through SMWDisX's
`SMW_U.sym` and the routine bodies were read in `bank_00.asm`/`bank_05.asm`
on 2026-10-11; the fingerprint bytes were checked against the USA ROM.

## The game's level-entry routine

`GM11LoadLevel` (`$00:96D5`, game mode `$11`) calls `CODE_05D796`, which
decides the level number and loads the pointers:

| Step | Code | What it does |
|---|---|---|
| `$05:D796` | `LDA SublevelCount; BNE sublevel` | `$141A` = 0: entering from the overworld |
| `$05:D83E` | `STZ $0F; LDY #0; LDA OverworldOverride; BNE $D8A2` | `$0109` ≠ 0 skips the overworld position lookup (which would write `TranslevelNo`) |
| `$05:D8A2` | `CMP #$25; BCC; SBC #$24; STA LoadingLevelNumber; STA $0E; LDA OWPlayerSubmap,Y; BEQ; LDA #1; STA $0F` | low byte from the override, high byte from the submap flag `$1F11` |
| `$05:D8B7` | `REP #$30; LDA $0E; ASL; CLC; ADC $0E; TAY …` | **breakpoint**: `$0E/$0F` are final; pointers are read from `Layer1Ptrs`, `Layer2Ptrs`, `$05:EC00` (bank `$07` hardcoded) |
| after | `LDA UseSecondaryExit; BNE` else main entrance from the secondary header; `LDA SublevelCount; BNE` else `STZ MidwayFlag`, `LDX TranslevelNo; LDA OWLevelTileSettings,X; AND #$40; BEQ` else `STA SkipMidwayCastleIntro` and entrance screen = midway screen | from-overworld entry clears `$13CE`; the overworld tile's bit 6 selects the midway screen |
| sublevel path | `LDX PlayerXPosNext+1; LDA ExitTableLow,X; … LDA UseSecondaryExit; BEQ` else secondary exit tables `$05:F800/FA00/FC00/FE00` indexed by `$0E/$0F` | screen exits and secondary exits |

Fingerprint at `$05:D8B7`: `C2 30 A5 0E 0A 18 65 0E A8`. Lunar Magic 2.5x
ROMs keep this code (checked on Quickie World 2).

## RAM used (bank `$7E`)

| Name | Address | Launcher use |
|---|---|---|
| `GameMode` | `$0100` | wait for `$07` (title) or `$0E` (overworld); set `$0F` (fade to level); wait for `$14` (playing) |
| `OverworldOverride` | `$0109` | set 1 so the entry routine takes the direct path |
| `$0E/$0F` | | poked with the level number at the breakpoint |
| `LoadingLevelNumber` | `$17BB` | poked with the low byte |
| `$010B` | | Lunar Magic's 16-bit level number copy, poked too |
| `TranslevelNo` | `$13BF` | set 0 |
| `OWPlayerSubmap` | `$1F11` (2) | 1 for levels `$100+` (high byte source) |
| `SublevelCount` | `$141A` | 0, or 1 for a secondary-exit entrance |
| `UseSecondaryExit` | `$1B93` | 1 for a secondary-exit entrance |
| `ExitTableLow/High` | `$19B8`/`$19D8` (32) | entry 0 = exit number for a secondary-exit entrance (`PlayerXPosNext+1` = 0) |
| `PlayerXPosNext/YPosNext` | `$94`/`$96` | read back as the player's position |
| `MidwayFlag` | `$13CE` | cleared |
| `OWLevelTileSettings` | `$1EA2` (96) | bit 6 of entry 0 set for a midway entrance |
| `Powerup` | `$19` | 0-3 |
| `PlayerLives` | `$0DBE` | lives − 1 |
| `PlayerCoins` | `$0DBF` | |
| `PlayerItembox` | `$0DC2` | |
| `CarryYoshiThruLvls` | `$0DC1` | 0 (Yoshi not yet supported) |
| `PlayerRidingYoshi` | `$187A` | 0 |
| `PlayerTurnOW` | `$0DD6` | 0 Mario, 4 Luigi |

## Sequence (`smw_play::Launcher`)

1. Reset the console, install the overlays (below), set the breakpoint at
   `$05:D8B7` when the fingerprint matches.
2. Run frames until the game mode is `$07` or `$0E` (900-frame budget).
   The title demo also passes the breakpoint; those hits are ignored.
3. Poke the player state and the entrance bytes, then `$0100 = $0F`.
4. Run until the breakpoint (240 frames); poke `$0E/$0F`, `$17BB`, `$010B`;
   remove the breakpoint. (Skipped for a secondary-exit entrance: the exit
   decides the target level.)
5. Run until the game mode is `$14` (600 frames).

Unsaved edits: `Level::layer1_bytes` goes to `$60:8000` and the level's
3-byte entry in `Layer1Ptrs` is shadowed to point there; layer 2 objects
likewise at `$61:8000`; a vanilla background tilemap at `$0C:8000` or
`$0C:E8FE` (page) with a `$FF`-bank pointer, as the viewer does; sprite
bytes go to the largest known-free range of bank `$07` (vanilla hardcodes
the bank) or, on Lunar Magic ROMs, to `$62:8000` with the bank table entry
shadowed; the four secondary header bytes are shadowed too (that is how
`Entrance::Position` works).

Verified 2026-10-11 by playing levels `105` (main, midway with cape) and
`001` in the emulator (`solar play`): game mode `$14`, correct entrance
positions, status bar and sprites.

## Open

Yoshi state, Luigi's separate save fields, the "scripted overworld"
fallback for ROMs whose entry routine differs, `Entrance::Position` at
pixel precision.
