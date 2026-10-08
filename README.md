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
| M8 Lunar Magic 2.5x ROMs: levels, backgrounds, palettes, ExGFX, Map16 pages | done |
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
| Lunar Magic ROM compatibility, rendering fidelity (sprites, priority, layer 3), overworld | planned, see `docs/ARCHITECTURE.md` |
| Tools: IPS patcher (`solar ips-apply` / `ips-create` / `ips-info`, GUI Tools → IPS Patcher) | done: Lunar IPS parity (RLE, expand/truncate, apply log, copier-header add/strip) |

## Try it

You need your own `Super Mario World (USA).sfc`; ROMs are never distributed
with this project.

```bash
cargo solar info "roms/Super Mario World (USA).sfc"
cargo solar dump-gfx "roms/Super Mario World (USA).sfc" out/gfx --row 2
cargo solar dump-map16 "roms/Super Mario World (USA).sfc" out/map16.png --tileset 0
cargo solar dump-level "roms/Super Mario World (USA).sfc" 105
cargo solar render "roms/Super Mario World (USA).sfc" 105 out/level105.png --sprites
cargo solar relocate-level "roms/Super Mario World (USA).sfc" out/edited.sfc 105
cargo run -p solar-magic -- "roms/Super Mario World (USA).sfc" 105   # the editor
cargo solar ips-info hack.ips
cargo solar ips-apply hack.ips "roms/Super Mario World (USA).sfc" out/hack.smc --header add --log
cargo solar ips-create original.sfc modified.sfc out/changes.ips
cargo run -p solar-magic -- "roms/Super Mario World (USA).sfc" 105   # the viewer
```

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the design and
[`docs/formats/`](docs/formats/) for the file formats.

## License

MIT.
