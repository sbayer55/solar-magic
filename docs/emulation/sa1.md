# SA-1

Implemented in `crates/snes-sa1`, plugged into `snes-core` through the
`Coprocessor` trait (`cart/sa1_glue.rs`). fullsnes "SNES Cart SA-1"
(read 2026-10-12) is the register reference; ares `sfc/coprocessor/sa1`
settles what it leaves open (interrupt latches, write-protection rules,
the character-conversion buffer layouts, division remainder sign).

## What the chip is

A second 65c816 at 10.74 MHz (one SA-1 cycle = 2 master cycles) with:

- 2 KiB I-RAM at `$3000-$37FF` (also `$0000-$07FF` on the SA-1 side);
- BW-RAM (the board's save RAM, up to 256 KiB) at `$40-$4F`, an 8 KiB
  block of it at `$6000-$7FFF` chosen per side (`BMAPS`/`BMAP`), and a
  bitmap projection at `$60-$6F` for the SA-1 (`BBF`: 2 or 4 bits per
  byte-sized pixel);
- the Super MMC: four 1 MiB ROM blocks (`CXB`-`FXB`) mapped HiROM-style
  at `$C0-$FF` and, when a block's bit 7 is set, LoROM-style at
  `$00-$3F`/`$80-$BF:$8000-$FFFF`; blocks without bit 7 see the first
  4 MiB linearly there;
- programmable vectors: the SA-1 always takes reset/NMI/IRQ from
  `CRV`/`CNV`/`CIV`; the S-CPU takes NMI/IRQ from `SNV`/`SIV` when `SCNT`
  bits 4/6 say so;
- interrupts both ways (`CCNT`/`SCNT` with the `SIE`/`SIC`/`CIE`/`CIC`
  enable and clear ports), an H/V or linear timer, memory-to-memory DMA
  (ROM/BW-RAM/I-RAM), two character conversions into an I-RAM tile
  buffer, a signed 16×16 multiplier / 16÷16 divider / 40-bit multiply-
  accumulate, and a variable-length bit reader over ROM.

## Model

`Sa1 { cpu: w65c816::Cpu, chip: Chip }`; `Chip` implements `w65c816::Bus`
for the SA-1 core and holds the memories, registers and the SA-1's own
clock in master cycles. Each SA-1 bus access costs one cycle (two for
BW-RAM), which is what ares charges without contention.

Scheduling: the S-CPU is the master. `Sa1::run_to(now)` runs SA-1
instructions until its clock passes `now`; it is called before every
S-CPU access to a shared region (registers, I-RAM, BW-RAM) and at every
line start, so the SA-1 may lead the S-CPU by at most one instruction and
an SA-1→S-CPU interrupt is seen within a line. Bus-arbitration stalls
(the two CPUs contending for ROM/BW-RAM/I-RAM) are not modelled; neither
are the 5-6 cycles the arithmetic unit takes (results are immediate).

Interrupt entry: before each instruction the pending NMI (`CCNT` bit 4
latched by `CIC`/`CIE`) or, with I clear, the timer/DMA/S-CPU IRQ is
taken; the vector is supplied to the core's normal `$FFEA`/`$FFEE` reads
(either mode) by the bus. Releasing `CCNT` bit 5 resets the SA-1 core
(emulation mode, `PC` from `CRV`) and clears `CIWP`.

Write protection: I-RAM writes need the side's `SIWP`/`CIWP` bit for the
256-byte chunk (both zero at power-on, so a program must open them first);
BW-RAM's `BWPA` area is protected only while both `SBWE` and `CBWE` are
clear (ares, for Kirby's Dream Land 3).

Character conversion 1 turns the S-CPU's BW-RAM reads into converted
tiles (one character converted into the `DDA` buffer at each character
boundary); type 2 converts one eight-pixel row from `BRF` per completed
half of the register file. DMA runs to completion at the triggering write
and sets the DMA IRQ flag.

## Not modelled / unverified

No real SA-1 cartridge is available locally, so the chip is verified only
by unit tests (`crates/snes-sa1/src/tests.rs`: MMC mapping, vectors,
BW-RAM windows and bitmap views, write protection, arithmetic, bit
reader, DMA, conversion type 2, a small SA-1 program with a timer) and a
console-level test (`crates/snes-core/tests/sa1.rs`: the S-CPU programs
the chip, the SA-1 computes and interrupts back). Lunar Magic's SA-1 Pack
ROMs are the real target; `RomLayout` does not yet know their layout.
PAL boards would need the timer's 312-line count (the cartridge always
builds the chip for NTSC).
