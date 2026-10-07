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
