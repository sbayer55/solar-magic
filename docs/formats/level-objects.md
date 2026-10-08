# Level object streams

Implemented in `crates/smw-data/src/level/objects.rs`. Parser in the game:
`LoadLevelData` ($05:86xx) with extended objects dispatched by `CODE_0DA106`.
Verified against the SMWDisX disassembly 2026-10-06 and by re-serializing all
512 vanilla levels byte for byte.

Layer 1 data is the 5-byte header followed by the stream. Layer 2 object
data is an unused 5-byte header copy followed by a stream in the same format.

## Encoding

```
byte 0   nBBhyyyy   n = new screen flag (advance the current screen by 1 before placing)
                    BB = object number bits 5-4
                    h = "high coordinate" (lower half of the screen); with yyyy forms Y 0-31
byte 1   bbbbxxxx   bbbb = object number bits 3-0, xxxx = X within screen 0-15
byte 2   ssssssss   standard objects: size byte (usually hhhhwwww, object-specific)
                    extended objects (number 0): the extended object number
terminator: a byte 0xFF where byte 0 would be
```

Object number = `((byte0 & 0x60) >> 1) | (byte1 >> 4)`, 0x00–0x3F.

### Extended objects (object number 0)

| byte 2 | Meaning | Length |
|---|---|---|
| `0x00` | **Screen exit.** `byte0 & 0x1F` = screen; `byte1 & 1` = destination bit 8; `byte1 & 2` = use secondary exit; **byte 3** = destination bits 0–7. | 4 |
| `0x01` | **Screen jump.** Current screen becomes `byte0 & 0x1F`. | 3 |
| `0x02`–`0x0F` | unused (null routine) | 3 |
| `0x10`+ | single-tile and other extended objects (`ExtOBJSingleTile`, `ExtOBJDoor`, …) | 3 |

Screen jumps and the new-screen flag both affect the "current screen" the
following objects land on; Solar Magic resolves this into `RawObject::screen`.

### Vertical levels

In vertical levels the game swaps the X and Y nibbles of bytes 0 and 1 for
every object except screen exits and jumps (`CODE_0585D8`). The parser keeps
the raw fields; renderers apply the swap.

### Rendering

The 0x3F standard object routines (`CODE_0DA40F` dispatch) and the extended
object routines stamp Map16 tiles into `$7E:C800`/`$7F:C800`. Solar Magic
runs them in a 65c816 interpreter (M4) rather than reimplementing them.

## Loader facts that constrain writing (verified in SMWDisX 2026-10-07)

- `LoadLevelData` (`$05:85FF`) reads each object with `LDA [$65],Y` and then
  advances `$65` with a 16-bit add (`ADC $65` / `ADC #$00` into `$66`); the
  bank byte `$67` is never incremented. A level's layer 1 (or layer 2 object)
  data must therefore stay inside one bank, i.e. one 32 KiB LoROM chunk.
- Vertical levels (`CODE_0585D8`): when the level is vertical the loader swaps
  the low nibbles of bytes 0 and 1 for every object except extended objects
  0 (screen exit) and 1 (screen jump). So in a vertical level byte 0 bits 0–3
  hold X (low nibble), bit 4 the X high bit (right half of the 32-wide
  screen), and byte 1 bits 0–3 hold Y within the 16-row screen.
## Lunar Magic objects

Lunar Magic 2.5x fills standard object slots that vanilla never uses (vanilla
levels contain no objects 0x03, 0x07, 0x16, 0x22-0x2D or 0x2F) and adds one
extended object. Verified 2026-10-06 by disassembling the dispatch table at
`$0D:A4B2` (3-byte entries, indexed by number − 0x20) and the routines it
points at in Quickie World 2, then re-serializing every RATS-tagged object
stream of both Quickie World ROMs to its exact tag length. The rules only
apply when the layout is Lunar Magic (`ObjectFormat::LunarMagic`); vanilla
streams parse identically under both formats (`real_rom_streams_agree_under_lm_format`).

| Object | Routine | Length | Meaning |
|---|---|---|---|
| standard `0x22` / `0x23` | `$0D:F08A` / `$0D:F08E` | 4 | Direct Map16 tile from page 0 / page 1: `tile = (number & 1) << 8 \| byte3`; byte 2 = size `hhhhwwww` (counts − 1). |
| standard `0x24`, `0x25`, `0x26` | `$0D:F0E0`, `$0D:F0F0`, `$0D:F130` | 3 | LM settings objects (store into `$FA-$FD`, `$0DDA`). |
| standard `0x27` | `$0D:F150` | 5-8 | Direct Map16 tile, `tile = (byte3 & 0x3F) << 8 \| byte4`. Byte 3 bit 7: byte 5 (pattern nibbles) follows; bits 7+6: byte 6 (height) follows too; then byte 2 bit 7: byte 7 (conditional flag, `$0D:F290` tests `$7F:C060`) follows. |
| standard `0x29` | `$0D:FF50` | 5-8 | As 0x27 with `tile \| 0x4000`. |
| extended `0x02` | `$0D:E1B0` (pointer at `$0D:A115`) | 5 | Screen exit: screen = `byte0 & 0x1F`; byte 3 = destination bits 0-7; byte 4 bit 0 = destination bit 8, bit 1 = secondary exit, upper bits = LM flags. |

Lengths: `len(0x27/0x29) = 5 + [b3 & 0x80] + [(b3 & 0xC0) == 0xC0] + [(b3 & 0xC0) == 0xC0 && b2 & 0x80]`
(`crates/smw-data/src/level/lm_objects.rs`). The two ROMs contain 7039 5-byte,
115 6-byte and 266 7-byte 0x27 objects; the 8-byte form and object 0x29 are
verified by disassembly only.

LM also changes the vanilla 4-byte exit's flag handling (`$0D:A531`:
`AND #$0F` instead of `AND #$01`), so bits 2-3 of byte 1 are LM flags.
