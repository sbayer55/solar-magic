# LC_LZ2

Planned for `crates/lc-lz2` (M1). The format Lunar Compress calls LC_LZ2 is
what SMW uses for its GFX files. Decompression routine in vanilla: `$00:B8DE`,
output buffer `$7E:AD00`.

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

Status: spec recorded from sneslab.net; to be verified against GFX00 on
implementation (expected decompressed size 0x1800 bytes for a 3bpp 128-tile file).
