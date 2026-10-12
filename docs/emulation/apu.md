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

## S-DSP

Register storage only (`DspRegs`, `FLG` powers on as `$E0`). Voices, BRR,
envelopes, echo and output are milestone E6.
