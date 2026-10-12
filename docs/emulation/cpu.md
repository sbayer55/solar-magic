# 65c816 core: cycles and interrupts

Implemented in `crates/w65c816`. This note covers what the SNES emulator
needs beyond "functionally correct": exact bus cycles and hardware
interrupts. The untimed callers (`smw-render`) are unaffected.

## Cycle model

The core does not count cycles. Every byte it reads or writes is one bus
cycle, and every internal cycle is a `Bus::idle()` call (a default no-op, so
`FlatBus`, `smw_render::emu::SnesBus` and the ownership wrapper compile and
behave as before). A timed bus charges the access speed for the address
(6/8/12 master cycles) in `read`/`write` and 6 in `idle`, and lets the rest of
the machine catch up before returning the byte.

Internal cycles are placed where the hardware spends them:

| Rule | Where |
|---|---|
| direct page with `DL != 0`: +1 after the operand byte | `dp_loc` |
| `dp,X`/`dp,Y`/`(dp,X)`: +1 for indexing (`(dp,X)` before the pointer read) | `resolve_dp` |
| `abs,X`/`abs,Y`/`(dp),Y`: +1 when writing or read-modify-writing, when the index is 16-bit, or when the index carries into the high address byte | `idle_index` |
| `sr,S`: +1; `(sr,S),Y`: +1 before and +1 after the pointer | `resolve` |
| read-modify-write: +1 between read and write; 16-bit results are written high byte first | `alu::rmw`, `write_loc_rmw` |
| implied/transfer/flag ops, `INX`…: +1; `XBA`: +2; `SEP`/`REP`/`XCE`: +1; `WDM`: skips its operand during +1 | `misc.rs` |
| taken branch: +1, and +1 more in emulation mode across a page | `branch_if` |
| `BRL`/`PER`: +1 after the operand; `PEA`/`PEI`: none; `PEI`'s pointer read never page-wraps | `branch.rs`, `stack.rs` |
| `PHx`: +1 before the push; `PLx`: +2 before the pull | `stack.rs` |
| `JSR abs`: +1 before the push; `JSR (abs,X)`: push between the operand bytes (with the page-1 wrap), +1 after; `JSL`: push PB, +1, fetch bank, push PC; `JMP (abs,X)`: +1 | `branch.rs` |
| `RTS`: +2, pull, +1; `RTL`: +2, three raw pulls, then SH forced to 1; `RTI`: +2 | `branch.rs` |
| `BRK`/`COP`: signature fetch, pushes, vector (7/8 cycles, no idle) | `software_interrupt` |
| `MVN`/`MVP`: 7 per byte (read, write, +2), PC rewound until A wraps | `block_move` |
| `WAI`/`STP`: +2 | `misc.rs` |
| hardware interrupt entry: dummy opcode fetch, +1, pushes, vector (7/8) | `interrupt.rs` |

In emulation mode the stack pointer's high byte is forced to `$01` before
every instruction (`Cpu::step`), matching the wired SH register.

## Interrupts

`Cpu::nmi(bus)` always enters (`$FFEA`/`$FFFA`); `Cpu::irq(bus)` enters only
with I clear (`$FFEE`/`$FFFE`) and returns whether it did. Both clear
`waiting`, so an IRQ with I set still resumes after `WAI`. `WAI` sets
`waiting`; `step` then returns `Exec::Waiting` without touching the bus until
an interrupt arrives. `STP` sets `stopped`; only `Cpu::reset(bus)` clears it
(emulation mode, I set, D clear, `PC` from `$FFFC`). NMI edge detection,
`RDNMI`/`TIMEUP` latches and timer compares live in the SNES bus, not here.

## Verification

`W65C816_TESTS_DIR=<dir> cargo test -p w65c816 --release --test single_step`
runs all 512 SingleStepTests files (10,000 cases each, native and emulation
mode) and requires the architectural state **and** the exact cycle sequence
to match: read/write with address and value, internal cycles by position.
Status 2026-10-11: 0 failures in both modes. Skipped: `44`/`54` (the suite
models a whole block move per step) and one case listed in
`KNOWN_DISAGREEMENTS` (`(dp,X)` with `DL = 0` and `dp + X = $FF` in emulation
mode: the core keeps the 6502 page wrap for the pointer's high byte, as bsnes
does; the suite reads it from the next page and has no other case covering
it).
