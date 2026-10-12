# Audio unit: SPC700, S-SMP and (later) S-DSP

Implemented in `crates/spc700` (the core) and `crates/snes-apu` (everything
around it). fullsnes "SNES APU" chapters, read 2026-10-11.

## SPC700 core (`spc700`)

Same shape as `w65c816`: a 256-entry table of `(handler, parameter)`
entries, one small handler per operand shape (`table.rs`), and a bus trait
where every `read`/`write`/`idle` is exactly one SPC cycle, so instruction
timing is emergent. Implied-operand instructions perform the hardware's
dummy read of the byte after the opcode; stores perform a dummy read of
their destination; `MUL` spends 7 and `DIV` 10 internal cycles; `DIV`
follows the hardware's behaviour for quotients that do not fit (ares
`algorithmDIV`); `AND1` is one cycle shorter than `OR1`/`EOR1`.
`SLEEP`/`STOP` halt the core (the S-SMP has no interrupt sources).

Verification: `SPC700_TESTS_DIR=~/code/SingleStepTests/spc700/v1 cargo
test -p spc700 --release --test single_step` compares registers, RAM and
the exact cycle sequence for all 256 opcodes (1,000 cases each). Status
2026-10-11: 0 failures.

## S-SMP (`snes-apu::Smp`)

| Address | Register | Model |
|---|---|---|
| `$0000-$FFFF` | 64 KiB RAM | zeroed at power-on; every write reaches RAM, even under the ports and the boot ROM |
| `$F0 TEST` | stored only | |
| `$F1 CONTROL` | timer enables (0→1 restarts a timer), bits 4/5 clear the two inbox pairs, bit 7 maps the boot ROM | power-on `$80` |
| `$F2/$F3` | DSP address/data | `$80-$FF` read the mirror, writes there are ignored |
| `$F4-$F7` | CPU ports | reads return what the S-CPU wrote to `$2140-$2143`; writes appear on the S-CPU's reads |
| `$F8/$F9` | aux ports | RAM-like |
| `$FA-$FC` | `TnDIV` (0 = 256) | |
| `$FD-$FF` | `TnOUT` | 4 bits, cleared on read |

Timers: an 8-bit counter advances at 8 kHz (T0/T1, every 128 cycles) or
64 kHz (T2, every 16 cycles); reaching the target resets it and increments
the 4-bit output.

Clock: `SPC cycles = master × 1 024 000 / 21 477 272`, computed exactly in
integers (`Apu::target_cycles`), so the two crystals never drift. The bus
catches the APU up before every `$2140-$217F` access and at every
scanline.

## Boot ROM

Nintendo's 64-byte IPL is not shipped. `Ipl::Hle` (the default) reproduces
its behaviour from the fullsnes disassembly as a state machine with the
instructions' cycle costs:

1. `Entry`: zero `$0001-$00EF`, `SP = $EF`, announce `$AA`/`$BB` on ports 0/1.
2. `WaitKick`: wait for `$CC` on port 0.
3. `Main`: copy ports 2/3 to `$0000/$0001`, echo port 0, read the command
   from port 1: nonzero → `WaitZero`, zero → jump to `[$0000]` with
   `A = X = Y = 0`, `SP = $EF`, `PSW = $02`.
4. `WaitZero`: wait for index 0 on port 0.
5. `Transfer`: while port 0 equals the expected index, store port 1 at
   `[$0000] + index` and echo the index; when port 0 has moved *past* the
   index (`Y - port0` negative), return to `Main`.

Reads of `$FFC0-$FFFF` return `$FF` under the model. `Ipl::Rom(bytes)`
executes a user-supplied dump normally. Super Mario World's driver upload
completes and the game reaches its title screen (`SMW_ROM`-gated test).

## S-DSP (`snes-apu::Dsp`)

Runs once every 32 SPC cycles (32 kHz), after the fullsnes DSP chapters and
the `snes_spc`/ares model (`dsp/{mod,voice,echo,tables}.rs`):

- **Voices:** `DIR`/`SRCN` → BRR start/loop; two BRR bytes decode per
  sample into a 12-sample ring (shift with the `≥13` rule, filters 0-3 with
  the exact fixed-point formulas, 16-bit clamp, stored ×2); the pitch
  counter (bits 12-13 sample, 4-11 Gaussian index, `+pitch` per sample
  clamped to `$7FFF`, pitch modulation from the previous voice's output);
  four-point Gaussian interpolation with the 512-entry table transcribed
  from fullsnes and the hardware's partial overflow handling; noise LFSR
  shared by all voices at the `FLG` rate.
- **Envelope:** ADSR (attack `N×2+1`, decay `N×2+16`, sustain `N`) or GAIN
  (direct, linear/exponential decrease, linear/bent increase), stepped when
  the shared 30720-sample counter fires for the rate (ares' rate/offset
  tables), decay→sustain at the sustain level, release −8 per sample.
- **Keys:** `KON`/`KOFF` polled every other sample; key-on starts a
  five-sample delay (envelope 0, no pitch) and clears the voice's `ENDX`;
  a block with end-without-loop or `FLG` bit 7 forces release at level 0;
  `ENDX` sets when a looping block ends; `ENVX`/`OUTX` are written every
  sample; writing `ENDX` clears it.
- **Echo:** ring buffer at `ESA` of `EDL×2 KiB` (length sampled on wrap),
  8-tap FIR over the last eight reads (first seven summed wrapping, the
  eighth clamped), feedback through `EFB`, writes gated by `FLG` bit 5.
- **Mixer:** per-voice `VOL×out >> 7` with 16-bit clamps, master and echo
  volumes, mute; output queued as interleaved `i16` for the host
  (`Console::take_audio`).

Verified 2026-10-12: `solar play 105 --audio out.wav` produces the level's
music (continuous, bounded, with harmonic peaks); `SMW_ROM`-gated test
`launched_level_plays_music`. The editor's Play panel streams it through
`cpal` (resampled to the device rate) and paces emulation on the queue
depth, so picture and sound stay in step.
