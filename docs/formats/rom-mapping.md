# ROM mapping and internal header

Implemented in `crates/snes-rom/src/mapper.rs` and `header.rs`.

## LoROM (map mode $20 / $30)

Each 32 KiB half of the ROM appears at `$8000-$FFFF` of consecutive banks.

```
PC   = ((bank & 0x7F) << 15) | (offset & 0x7FFF)      for offset >= $8000
bank = PC >> 15      (use $FE/$FF instead of $7E/$7F, which are WRAM)
```

| SNES | PC | Notes |
|---|---|---|
| `$00:8000` | `0x000000` | first ROM byte |
| `$00:FFC0` | `0x007FC0` | internal header |
| `$05:E000` | `0x02E000` | layer 1 pointer table (SMW) |
| `$0F:FFFF` | `0x07FFFF` | last byte of a 512 KiB ROM |

`$0000-$7FFF` of any bank is treated as non-ROM. `$7E`/`$7F` are WRAM.
Lunar Magic can expand ROMs beyond 4 MiB (ExLoROM, map mode `$32`) or convert
them to SA-1 (`$23`); both are detected and rejected for now.

## Copier header

Some dumps carry a 512-byte copier header. It is detected by
`size % 1024 == 512` (`snes_rom::copier_header::detect`) and stripped before
anything else looks at the image. The same module can add or strip the header
on raw file bytes for IPS patching; see `ips.md`.

## Internal header ($00:FFC0, 32 bytes parsed)

| Offset | Size | Field | Vanilla SMW (USA) |
|---|---|---|---|
| `$FFC0` | 21 | Title, space padded | `SUPER MARIOWORLD     ` |
| `$FFD5` | 1 | Map mode (`$20` LoROM, `$21` HiROM, `$23` SA-1, `$25` ExHiROM, `$32` ExLoROM; `+$10` FastROM) | `$20` |
| `$FFD6` | 1 | Cartridge type | `$02` (ROM + RAM + battery) |
| `$FFD7` | 1 | ROM size: `1 << (n + 10)` bytes | `$09` (512 KiB) |
| `$FFD8` | 1 | SRAM size: `1 << (n + 10)` bytes, 0 = none | `$01` (2 KiB) |
| `$FFD9` | 1 | Region | `$01` (USA) |
| `$FFDA` | 1 | Developer ID | `$01` (Nintendo) |
| `$FFDB` | 1 | Version | `$00` (1.0) |
| `$FFDC` | 2 | Checksum complement (LE) | `$5F25` |
| `$FFDE` | 2 | Checksum (LE) | `$A0DA` |

The checksum is the 16-bit sum of every byte; images whose size is not a power
of two are mirrored up to one before summing. Header detection scores both
candidate offsets (`0x7FC0`, `0xFFC0`) and requires a minimum plausibility
(consistent checksum pair, or known map mode + sane size + printable title).

Verified against `Super Mario World (USA).sfc` on 2026-10-06: title, map mode,
sizes, checksum pair, and that the computed checksum equals `$A0DA`.

## Writing (`Rom::write`, `expand_to`, `fix_checksum`, `save`)

- Writes are file-contiguous like reads; a write that overlaps the header
  re-parses it, so `Rom::header()` never goes stale.
- `expand_to(len)` requires a power of two no smaller than the image and no
  larger than the mapping allows (4 MiB for LoROM), pads with `0x00` and sets
  the size code to `log2(len) - 10`. Lunar Magic expands a vanilla ROM to
  1 MiB (`$0A`) on its first save and also pads with `0x00` (verified
  2026-10-07 against an LM 3.71 save: bytes `0x80000..` are zero apart from
  RATS-tagged blocks).
- `fix_checksum` writes `FF FF 00 00` over complement + checksum, sums the
  image, then stores complement and checksum little-endian. Those four bytes
  always contribute `0x1FE`, so the result is stable under repeated fixes.
- `save` re-prepends the copier header that was stripped on load and writes
  through a sibling `.tmp` file followed by a rename.
