# RATS tags

Implemented in `crates/snes-rom/src/rats.rs`.

"Recognized Allocated Tag System" tags mark blocks in expanded ROM space that a
tool has claimed. Lunar Magic wraps everything it inserts in one, and other
tools (Asar, GPS, PIXI, AddmusicK) honour them when searching for free space.

```
offset  size  meaning
0       4     ASCII "STAR"
4       2     data length - 1, little-endian   (0x0000 .. 0xFFFF)
6       2     bitwise NOT of the previous word, little-endian
8       n     protected data (1 .. 65536 bytes)
```

A tag is valid only if `size ^ inverse == 0xFFFF`. Scanning walks the image
byte by byte and skips over each tag's data, so a `STAR` inside protected data
is not reported as a tag. The vanilla ROM contains no tags.

Future (write support): free space is any run of `0x00`/`0xFF` bytes above
`0x80000` not covered by a tag; every allocation writes a tag first.
