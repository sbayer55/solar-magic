# RATS tags

Implemented in `crates/snes-rom/src/rats/` (scanning in `mod.rs`, allocation
in `alloc.rs`).

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

## Allocation (`rats::alloc`)

Free space is any run of `0x00` / `0xFF` bytes not covered by a tag. An
allocation is first-fit inside a caller-supplied range and never straddles a
32 KiB LoROM bank, because the game's level loader advances its data pointer
with a 16-bit add (`docs/formats/level-objects.md`). Freeing zero-fills the
tag and its data.

Where Solar Magic allocates (`smw_data::level::save_level`, vanilla-safe mode):

- **Layer 1 and layer 2 object data** go to expanded space, i.e. file offsets
  at or above `RomLayout::expanded_start_pc` (`0x80000` for vanilla); the
  image is grown to 1 MiB first. This mirrors Lunar Magic, which expands a
  vanilla ROM to 1 MiB on its first save and places its first blocks at
  `0x80000` (verified 2026-10-07 against an LM 3.71 save: tags at `0x080000`,
  `0x080120`, …). Untagged vanilla data is never overwritten, so the original
  level stays recoverable and pointer aliasing between levels is harmless.
- **Sprite data** must stay in bank `$07` (the game hardcodes the bank) and
  **background tilemaps** in bank `$0C` (on the half matching their page). They
  are rewritten in place when they fit and nobody else points at them;
  otherwise they move into `RomLayout::known_free`, the ranges from SMWDisX
  `freespace.txt` (SNES addresses) that were checked to be all `0xFF` in the
  USA ROM on 2026-10-07. Bank `$07` offers about 4.4 KiB in five ranges and
  bank `$0C` about 580 bytes in three; growing beyond that needs Lunar
  Magic's sprite-bank hijack (`docs/formats/lunar-magic-ext.md`).

Zero runs inside the original image are **not** treated as free below
`expanded_start_pc`: the sprite property tables in bank `$07`, for example,
contain long runs of `0x00` that are real data.
