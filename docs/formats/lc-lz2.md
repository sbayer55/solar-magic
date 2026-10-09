# LC_LZ2

Implemented in `crates/lc-lz2` (`decompress.rs`, `compress.rs`). The format
Lunar Compress calls LC_LZ2 is what SMW uses for its GFX files. Decompression
routine in vanilla: `$00:B8DE`, output buffer `$7E:AD00`.

Each chunk starts with a header byte `CCCLLLLL`:

| `CCC` | Command | Payload | Output |
|---|---|---|---|
| `000` | Direct copy | `L+1` bytes | the bytes as-is |
| `001` | Byte fill | 1 byte | that byte `L+1` times |
| `010` | Word fill | 2 bytes | alternating the two bytes for `L+1` bytes |
| `011` | Increasing fill | 1 byte | byte, byte+1, byte+2, … for `L+1` bytes |
| `100` | Repeat | 2 bytes, **big-endian** offset into the output so far | copy `L+1` bytes from that offset (may overlap the write position) |
| `101`, `110` | unused | | treat as error |
| `111` | Long length | second header byte: `111CCCLL LLLLLLLL` → 10-bit length, then the payload of command `CCC` | |

A header byte of `$FF` terminates the stream.

Verified 2026-10-06: all 50 vanilla GFX files (GFX00–GFX31) decode; 3bpp
files decompress to 0xC00 bytes (128 tiles × 24 bytes), 4bpp to 0x1000, 2bpp
to 0x400/0x800. The two unassigned commands are rejected.

The encoder (`compress`) is greedy: at each position it takes the longest of
byte fill, word fill, increasing fill or repeat when that is cheaper than
copying the bytes directly, and otherwise extends a direct-copy run (split at
1024 bytes). Repeats search the whole 64 KiB window and may overlap the
write position. `solar gfx-check` confirms every vanilla file re-encodes to
no more than its stored size and decodes identically.
