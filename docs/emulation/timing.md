# Master clock and timing events

Implemented in `crates/snes-core/src/clock.rs` (positions) and
`crates/snes-core/src/bus/mod.rs` (what happens at each one). Sources:
fullsnes "SNES Timing H/V Events" and "CPU Clock Cycles", bsnes
`ppu/counter`. Read 2026-10-11.

## Model

The CPU is the master. Every bus access charges its speed in master cycles
and advances the clock; internal cycles (`Bus::idle`) cost 6. Components
that run on their own (PPU, APU, SA-1) catch up lazily when the CPU touches
them and at every scanline event, so writes land on the exact dot.

| Region | Cycles |
|---|---|
| `$00-$3F/$80-$BF:$0000-$1FFF` WRAM mirror, `$6000-$7FFF` | 8 |
| `$2000-$3FFF`, `$4200-$5FFF` registers | 6 |
| `$4000-$41FF` joypad | 12 |
| `$8000-$FFFF` and banks `$40-$7F` | 8 |
| banks `$80-$FF` `$8000+` (`$C0+` whole) with `MEMSEL` bit 0 | 6 |
| DMA/HDMA | 8 per byte, 8 per channel, 8 to start (bsnes) |
| DRAM refresh | 40 cycles once per line at H ≈ 538 |

## Lines and frames (NTSC)

1364 master cycles per line, 340 dots (`$213C` counts 0-339); dots 323 and
327 take 6 cycles (bsnes model; fullsnes describes the same 1364 total as
four 5-cycle dots, which only differs in the latch value by ±1 around those
dots). 262 lines, 263 on the odd field when interlaced. Line 240 of odd
non-interlaced fields is 1360 cycles. PAL: 312/313 lines, line 311 of odd
interlaced fields is 1368.

## Events (master cycles into the line)

| H | Event | Where handled |
|---|---|---|
| 0 | new line; at V=0 new frame: field toggles, NMI flag auto-clears, `SETINI` interlace sampled | `SystemBus::service` |
| 0 on V=vdisp | vblank begins (`HVBJOY` bit 7 is derived from V), OAM address reload | `Ppu::vblank_start` |
| 2 (dot 0.5) on V=vdisp | `RDNMI` bit 7 sets; NMI pending if enabled | `Interrupts::on_events` |
| 4 (dot 1) | hblank flag clears | derived |
| 10 (dot 2.5) on V=VTIME | V-only IRQ (and H+V with `HTIME = 0`) | `Interrupts` |
| 24 (dot 6) on V=0 | HDMA tables reload | `hdma_init` |
| 130 (dot 32.5) on V=vdisp | auto-joypad read starts, busy for 4224 cycles | `Joypads::start_auto_read` |
| 538 (dot 133.5) | DRAM refresh: CPU stalls 40 cycles | `service` |
| 1096 (dot 274) | hblank flag sets | derived |
| 1112 (dot 278) on visible lines | HDMA transfers | `hdma_run` |
| `HTIME × 4 + 14` (dot HTIME + 3.5) | H timer match | `Clock::set_htime` |

vdisp is 225, or 240 with overscan (`SETINI` bit 2, sampled per line).

## Interrupt delivery

NMI and IRQ are polled by `Console::step_instruction` before each
instruction: a pending NMI edge is taken first, otherwise the IRQ line
(`TIMEUP` flag or the cartridge) is offered; the CPU takes it only with I
clear, but either wakes `WAI`. `RDNMI` reads clear the flag but not a
pending edge; `TIMEUP` reads and disabling the timer clear the IRQ.

## Known approximations (to revisit with timing test ROMs)

- DMA start alignment to the 8-cycle clock and the per-channel/HDMA
  overheads follow bsnes' simple model, not anomie's 18-cycle figures.
- The IRQ compare fires exactly at the documented dot; bsnes' lookahead
  compensation for recognition latency is not modelled.
- Auto-joypad results are available immediately (the busy flag is exact).
