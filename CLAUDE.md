# Solar Magic

Rust reimplementation of Lunar Magic (Super Mario World level editor).
Read `docs/ARCHITECTURE.md` first; format details are in `docs/formats/`,
emulator hardware notes in `docs/emulation/`.

## Commands

```bash
cargo build --workspace
cargo test --workspace                       # unit tests; real-ROM tests skip without SMW_ROM / LM_ROM
# absolute paths: cargo runs each crate's tests from the crate directory
SMW_ROM="$PWD/roms/Super Mario World (USA).sfc" LM_ROM="$PWD/roms/Patched_Quickie World 2/QW2.sfc" cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all --check
cargo xtask check                            # 1000-LOC file limit, lint hygiene
cargo solar info "roms/Super Mario World (USA).sfc"
cargo solar dump-gfx  "roms/Super Mario World (USA).sfc" <out_dir> [--id 14] [--row 2]
cargo solar dump-map16 "roms/Super Mario World (USA).sfc" <out.png> [--tileset 0] [--bg]
cargo solar dump-level "roms/Super Mario World (USA).sfc" 105
cargo solar render "roms/Super Mario World (USA).sfc" 105 <out.png> [--layers one|two|all] [--sprites] [--owners]
cargo solar dump-exits "roms/Super Mario World (USA).sfc" [id]
cargo solar dump-owners "roms/Super Mario World (USA).sfc" 105
cargo solar expand <rom> <out> --size 1M        # also: free-space, rats, bg-rle, gfx-check
cargo solar relocate-level <rom> <out> 105      # also: copy-level, apply-level (from dump-level --json)
cargo solar render "roms/Super Mario World (USA).sfc" 105 <out.png> [--layers one|two|all]
cargo solar ips-apply <patch.ips> <target> <out> [--header keep|add|strip] [--log]
cargo solar ips-create <original> <modified> <out.ips> [--log]
cargo solar ips-info <patch.ips> [--limit N]
W65C816_TESTS_DIR=~/code/SingleStepTests/65816/v1 cargo test -p w65c816 --release --test single_step   # cycle-exact, both modes
SPC700_TESTS_DIR=~/code/SingleStepTests/spc700/v1 cargo test -p spc700 --release --test single_step     # cycle-exact
cargo xtask fetch-test-roms && SNES_TEST_ROMS=target/test-roms/SNES cargo test -p snes-core --release --test test_roms   # open-source test ROMs vs their screenshots (SNES_TEST_ROMS_REPORT=1 lists all)
cargo solar emu-boot "roms/Super Mario World (USA).sfc" --frames 600 --out title.png [--trace 20000]  # in-house emulator: boot and render
cargo solar emu-bench "roms/Super Mario World (USA).sfc" [--frames 600] [--warmup 120]   # emulator speed vs real time
cargo solar play "roms/Super Mario World (USA).sfc" 105 --frames 120 --out play.png [--audio play.wav] [--midway] [--exit ID] [--powerup 0-3]
cargo run -p solar-magic -- "roms/Super Mario World (USA).sfc" 105 [--play]   # GUI editor; --play starts the level in the emulator
cargo run -p solar-magic --features gamepad -- <rom> 105 --play   # gamepad input in the Play panel (gilrs; libudev-dev on Linux)
```

To check the GUI visually: launch it in the background, then `screencapture -x shot.png`
and view the PNG (the bare cargo binary has no bundle id for app-level tools).

All of the above must pass before a change is done. CI runs them on macOS,
Linux and Windows.

## Rules

- **Verify every ROM address against the ROM before trusting a wiki.** Record
  the verification (what, date) in the relevant `docs/formats/*.md`. The
  SMWDisX disassembly (IsoFrieze, GitHub) plus its `SMW_U.sym` symbol file is
  the best reference; read it, never commit it.
- **Never commit ROMs, disassembly dumps, or Lunar Magic binaries.** `roms/`
  is gitignored. Goldens derived from the ROM are local-only until decided
  otherwise.
- **All table addresses go through `smw_data::RomLayout`**, never hardcoded in
  decoders. This is the seam for Lunar Magic-modified ROMs.
- **Lint policy is strict** (see `Cargo.toml` `[workspace.lints]`): no
  `unwrap`/`expect`/`panic`/indexing/`as`/bare arithmetic in library code.
  Use `.get()`, `checked_*`/`wrapping_*`/`saturating_*`, `u32::from`,
  `u8::try_from`. Small `Copy` types take `self` by value. Functions: ≤ 80
  lines, cognitive complexity ≤ 15, ≤ 5 args (bundle into a struct). Files:
  ≤ 1000 lines of code. Exceptions are item-scoped
  `#[allow(lint, reason = "...")]`; `as` in `const fn` is the usual case.
  Crate-level allows are only permitted in `solar-cli` and `xtask`.
- Prefer dispatch tables and data (`const` arrays, `.toml`) over large `match`
  statements.
- Tests that need the ROM read `SMW_ROM` and return early with an `eprintln!`
  when it is unset.
- Parsed level data must re-serialize byte for byte (`Level::layer1_bytes`
  etc.); the 512-level round-trip test enforces this.
- Level tiles come from running the game's loader in `w65c816`
  (`docs/formats/level-load-routine.md`), never from hand-ported object code.
- Edits live in a `Level` value and are previewed through bus overlays; the
  ROM is only written by `save_level` (vanilla-safe: RATS-tagged relocation,
  never overwrite untagged vanilla data). See `docs/formats/rats.md`.
- `roms/` lives in the main checkout; worktrees must use an absolute path or
  a symlink. Lunar Magic test material is under `roms/lm371/`.
- `as` casts live only in `crates/w65c816/src/bits.rs` and `const fn`
  accessors, each with an item-level allow and reason.
