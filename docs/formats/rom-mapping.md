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

## ExLoROM (map mode $32) — `snes_rom::ExLoRom`

Lunar Magic's layout for ROMs above 4 MiB. The bank's top bit is inverted
relative to LoROM:

| SNES | PC |
|---|---|
| `$80-$BF:$8000-$FFFF`, `$C0-$FF:$0000-$FFFF` | `((bank & 0x7F) << 15) \| (offset & 0x7FFF)` — the first 4 MiB, exactly as LoROM |
| `$00-$3F:$8000-$FFFF`, `$40-$7D:$0000-$FFFF` | `0x400000 + (((bank & 0x7F) << 15) \| (offset & 0x7FFF))` — the second 4 MiB |

Banks `$7E`/`$7F` are WRAM, so file offsets `0x7F0000-0x7FFFFF` are
unreachable. The lower halves of banks `$40-$7D`/`$C0-$FF` mirror their upper
halves (LoROM style). Bank `$00`'s internal header therefore sits at file
offset `0x407FC0`; `Rom::from_bytes` reads it there when the image is larger
than 4 MiB and it declares `$32`.

**Verification status:** matches the extended-LoROM map of snes9x
(`Map_ExtendedLoROMMap`) and bsnes; fullsnes does not describe it. Not yet
checked against a Lunar Magic–expanded ROM (none in `roms/`); do that before
`smw_data::RomLayout` relies on it.

## SA-1 (map mode $23) — `snes_rom::Sa1Rom`

The S-CPU's view at power-on, before the game writes the Super MMC registers
`$2220-$2223` (CXB/DXB/EXB/FXB, reset to blocks 0–3 with the LoROM bit
clear):

| SNES | PC |
|---|---|
| `$00-$3F:$8000-$FFFF` | `(bank << 15) \| (offset & 0x7FFF)` — first 2 MiB |
| `$80-$BF:$8000-$FFFF` | `0x200000 + (((bank & 0x3F) << 15) \| (offset & 0x7FFF))` — next 2 MiB |
| `$C0-$FF:$0000-$FFFF` | `((bank & 0x3F) << 16) \| offset` — the same 4 MiB as HiROM-style 1 MiB blocks |
| `$40-$4F`, `$00-$3F/$80-$BF:$6000-$7FFF` | BW-RAM (not ROM) |
| `$00-$3F/$80-$BF:$3000-$37FF` | I-RAM; `$2200-$23FF` SA-1 registers |

Verified 2026-10-11 against fullsnes, "SNES Cart SA-1 Memory Control": with
bit 7 of the MMC registers clear, "first 2 MByte of ROM are mapped to
00h-3Fh, and next 2 MByte to 80h-BFh". Game code reprograms the blocks at
runtime; that live mapping is the emulator's job (`snes-sa1`), and file
offsets at or above 4 MiB have no static address (`to_snes` returns `None`).

## Copier header

Some dumps carry a 512-byte copier header. It is detected by
`size % 1024 == 512` (`snes_rom::copier_header::detect`) and stripped before
anything else looks at the image. The same module can add or strip the header
on raw file bytes for IPS patching; see `ips.md`.

## Internal header ($00:FFC0, 32 bytes parsed)

| Offset | Size | Field | Vanilla SMW (USA) |
|---|---|---|---|
| `$FFC0` | 21 | Title, space padded | `SUPER MARIOWORLD     ` |
| `$FFD5` | 1 | Map mode (`$20` LoROM, `$21` HiROM, `$23` SA-1, `$25` ExHiROM, `$32` ExLoROM; `+$10` FastROM). HiROM/ExHiROM are still rejected. | `$20` |
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
