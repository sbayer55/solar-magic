# IPS patches

Implemented in `crates/ips` (format, apply, create) and
`crates/snes-rom/src/copier_header.rs` (header add/strip). Exposed as
`solar ips-apply`, `solar ips-create`, `solar ips-info` and the GUI's
Tools → IPS Patcher window. Feature parity target: Lunar IPS 1.03 (create,
apply, RLE, expand/truncate, apply log, 16 MiB files).

## File layout

```
offset  size  meaning
0       5     ASCII "PATCH"
5       …     records, back to back
…       3     ASCII "EOF"
…       3     optional: truncation length, big-endian (Lunar IPS extension)
```

A record:

```
offset  size  meaning
0       3     target file offset, big-endian (0 .. 0xFFFFFF)
3       2     size, big-endian
5       size  literal bytes                       if size != 0
5       2     run length, big-endian              if size == 0 (RLE)
7       1     byte to repeat                      if size == 0 (RLE)
```

## Apply semantics

- Records are applied in order to a copy of the input.
- A record that reaches past the end of the output grows it; any gap is
  zero-filled.
- After the last record, a truncation length cuts the output to that size if
  it is longer. A length equal to or larger than the output is a no-op (it
  never extends).
- Offsets are 24-bit, so files are limited to 16 MiB. A record that would
  write past `0x1000000` is an error.
- An RLE run length of 0 writes nothing but is kept so parse → serialize is
  byte-identical.
- Anything after `EOF` that is not exactly 3 bytes is rejected as trailing
  data (`Error::TrailingData`).

## The two format traps

**The EOF offset.** A record starting at `0x454F46` (`"EOF"`) is
indistinguishable from the terminator, and every applier reads it as `EOF`.
SNESTool's encoder emitted such records; Lunar IPS avoids them. Our encoder
funnels every record through one `push` that, when the offset is `0x454F46`,
starts the record one byte earlier and rewrites the (unchanged) byte before it:
a raw record gets that byte prepended (and its last byte split off if that makes
it 65536 long); an RLE record extends backwards by one if the previous byte has
the run value, otherwise a 2-byte raw record covers the boundary and the rest of
the run follows at `0x454F47`. `Patch::to_bytes` independently refuses to write
a record at that offset.

**Growth.** When the target is longer than the source, an applier only grows
the file as far as the last record reaches. If the new tail is all zeros there
would be no "changed" bytes there, so the final byte of the target is always
treated as changed and covered by a record.

## Encoder

Cost model: a raw record costs 5 + n bytes, an RLE record 8 bytes, and both
are limited to 65535 bytes of output per record.

1. Scan the target for changed bytes: inside the source's length a byte is
   changed when it differs; past it, when it is non-zero (the applier
   zero-fills) or it is the last byte.
2. Merge neighbouring changed ranges separated by 5 or fewer unchanged bytes:
   rewriting them costs no more than a new header.
3. Walk each range in runs of one byte value. A run becomes an RLE record when
   it is at least 4 bytes and is the whole range, at least 9 bytes when it
   touches one end of the range or follows another RLE record, or at least 14
   bytes when it would split a raw block (which costs a second header).
   Everything else is literal.
4. Chunk raw and RLE records at 65535 bytes.
5. If the target is shorter than the source, append its length after `EOF`.

Identical inputs give `PATCH EOF` (8 bytes). The output is not byte-identical
to Lunar IPS's (closed source), but it is correct under any applier and in
practice smaller: recreating two community patches from their patched ROMs gave
313 319 vs 318 728 bytes (Demo World 1.00) and 1 550 249 vs 1 551 328 bytes
(dwtlc 1.10), both reapplying byte for byte.

## Copier headers

IPS offsets are plain file offsets, so a patch made from a ROM with a 512-byte
copier header only lines up with another headered file. Most old SMW hacks were
made that way. `snes_rom::copier_header` detects the header by
`len % 1024 == 512` and can add a zeroed one or strip it; `ips-apply --header
add|strip` and the GUI's "Target header" choice apply that to the target before
patching. `ips-create` and the GUI warn when original and modified differ in
header state.

Verified 2026-10-07 against: the ZeroSoft IPS specification, Flips's
`libips.cpp` (truncation extension, EOF-offset handling), and by applying Demo
World 1.00 and dwtlc 1.10 (both headered-ROM patches) to the USA ROM with
`--header add`, which yields valid 2 MiB / 6 MiB LoROM images.
