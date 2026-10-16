# Solar Magic

A Rust reimplementation of [Lunar Magic](https://fusoya.eludevisibility.org/lm/),
the Super Mario World level editor. It opens vanilla and Lunar Magic 2.5x ROMs, renders every level
by running the game's own loader, and edits and saves levels in a
"vanilla-safe" way that Lunar Magic can open. Full Lunar Magic compatibility
(opening LM-saved ROMs, ExGFX, custom palettes, 4× Map16, …) is the long-term
goal; see the roadmap in `docs/ARCHITECTURE.md`.

## Status

| Milestone | State |
|---|---|
| M0 ROM container, address mapping, header, RATS, layout seam, CI | done |
| M8 Lunar Magic 2.5x (and, since LM1, 3.71) ROMs: levels, backgrounds, palettes, ExGFX, Map16 pages | done |
| M1 LC_LZ2, GFX and palette decoding (`solar dump-gfx`) | done |
| M2 Map16 (`solar dump-map16`) | done |
| M3 level headers, objects, sprites, layer 2 parse + 512-level round trip (`solar dump-level`) | done |
| M4 65c816 interpreter (passes 1.57M single-step tests) + layer 1 rendering via the game's own loader (`solar render`) | done |
| M5 layer 2 objects and backgrounds, compositing | done (tile priority bits pending) |
| M6 sprites | position markers with sprite numbers (`--sprites`); real sprite graphics pending |
| M7 egui viewer (`cargo run -p solar-magic -- <rom> [level]`) | done: level list, zoom/pan level view, info, Map16 and palette panels |
| E1–E2 ROM writing, expansion, checksum, RATS allocator (`solar expand`, `free-space`, `rats`) | done |
| E3 header bit-field tables, secondary exits, editing API (`solar dump-exits`) | done |
| E4 vanilla-safe level saving: relocation into RATS-tagged expanded space, pointer tables, block sharing (`solar relocate-level`, `copy-level`, `apply-level`, `dump-level --json`) | done |
| E5 background RLE encoder (`solar bg-rle`) | done |
| E6 per-object tile ownership from the emulated loader (`solar dump-owners`, `render --owners`) | done |
| E7 LC_LZ2 compressor (`solar gfx-check`) | done |
| G1–G4 editor: select, rubber band, drag, insert, delete, cut/copy/paste, undo/redo, header and exit editing, Save / Save As with dirty tracking | done |
| G5 direct Map16 paint tool | pending (needs Lunar Magic's object format) |
| Prep for Lunar Magic compatibility: `solar trace` (ROM/WRAM access tracing of any routine or level load), secondary exits in the undo stack, paste at the cursor, saved-block leak fixes | done |
| LM0 reverse-engineering tooling: `solar diff-rom` (changed ranges labelled by table, RATS tag changes, Lunar Magic markers), labelled `trace` output, `RomLayout::known_ranges`, marker scanning in `info` | done |
| LM1 version-agnostic Lunar Magic detection: per-feature hijack scan (`LmFeatures`), 2.5x and 3.x layer 2 flag formats; LM 3.71 ROMs open and render identically to vanilla | done |
| LM9 `.mwl` level files: `solar mwl-export` / `mwl-import` / `mwl-info` / `export-all` / `import-all`; exports equal Lunar Magic 3.71's byte for byte | done |
| Lunar Magic ROM compatibility, rendering fidelity (sprites, priority, layer 3), overworld | planned, see `docs/ARCHITECTURE.md` |
| Tools: IPS patcher (`solar ips-apply` / `ips-create` / `ips-info`, GUI Tools → IPS Patcher) | done: Lunar IPS parity (RLE, expand/truncate, apply log, copier-header add/strip) |
| In-house SNES emulator (`snes-core`, `snes-ppu`, `spc700`, `snes-apu`, `snes-sa1`, `smw-play`; `solar emu-boot`, `solar play 105 --out frame.png`) | E0–E9 done: cycle-exact 65c816 and SPC700, machine core, S-SMP with boot-ROM handshake and S-DSP sound, PPU renderer (modes 0-7, hi-res, interlace, offset-per-tile, sprites, windows, colour math; 56 of krom's 73 test ROMs match their screenshots), SA-1 coprocessor (unit-tested; no SA-1 ROM locally), launch straight into any level with unsaved edits (main/midway/secondary-exit entrances, play from any tile). Tools → Play Level (Ctrl+T) runs the edited level in the editor with keyboard or gamepad (`--features gamepad`) input and sound, save states, rewind and `.srm` saves; the Debug panel has registers, disassembly, breakpoints, watches, memory and a trace. Next: verify SA-1/ExLoROM against a real Lunar Magic ROM (`docs/emulation/`) |

## Try it

You need your own `Super Mario World (USA).sfc`; ROMs are never distributed
with this project.

```bash
cargo solar info "roms/Super Mario World (USA).sfc"
cargo solar dump-gfx "roms/Super Mario World (USA).sfc" out/gfx --row 2
cargo solar dump-map16 "roms/Super Mario World (USA).sfc" out/map16.png --tileset 0
cargo solar dump-level "roms/Super Mario World (USA).sfc" 105
cargo solar render "roms/Super Mario World (USA).sfc" 105 out/level105.png --sprites
cargo solar trace "roms/Super Mario World (USA).sfc" level:105      # what the loader reads and writes
cargo solar diff-rom before.sfc after.sfc --markers                 # what Lunar Magic changed, by table
cargo solar mwl-export "roms/Super Mario World (USA).sfc" 105 out/105.mwl
cargo solar mwl-import "roms/Super Mario World (USA).sfc" out/hack.sfc out/105.mwl --level 106
cargo solar relocate-level "roms/Super Mario World (USA).sfc" out/edited.sfc 105
cargo run -p solar-magic -- "roms/Super Mario World (USA).sfc" 105   # the editor
cargo solar ips-info hack.ips
cargo solar ips-apply hack.ips "roms/Super Mario World (USA).sfc" out/hack.smc --header add --log
cargo solar ips-create original.sfc modified.sfc out/changes.ips
cargo run -p solar-magic -- "roms/Super Mario World (USA).sfc" 105 --play   # the editor, playing the level
```

With [`just`](https://github.com/casey/just) installed, `just` lists the common
tasks (`just check` runs what CI runs, `just run 105` opens the editor), and on
macOS `just dmg` (or `just dmg universal`) builds a drag-to-Applications installer
in `target/dist/`.

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the design and
[`docs/formats/`](docs/formats/) for the file formats.

## License

MIT.
