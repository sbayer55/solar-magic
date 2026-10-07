# Solar Magic

A Rust reimplementation of [Lunar Magic](https://fusoya.eludevisibility.org/lm/),
the Super Mario World level editor. Early days: the current milestone is a
read-only level viewer for vanilla and Lunar Magic 2.5x ROMs. Full Lunar Magic compatibility is the
long-term goal.

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
cargo solar ips-info hack.ips
cargo solar ips-apply hack.ips "roms/Super Mario World (USA).sfc" out/hack.smc --header add --log
cargo solar ips-create original.sfc modified.sfc out/changes.ips
cargo run -p solar-magic -- "roms/Super Mario World (USA).sfc" 105   # the viewer
```

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for the design and
[`docs/formats/`](docs/formats/) for the file formats.

## License

MIT.
