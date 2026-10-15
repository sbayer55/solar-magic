# The level picture

What the editor (and `solar render`) shows for a level and where each
input comes from. Implemented in `crates/smw-render/src/{scene,plane,
compose,assets}.rs` and `crates/smw-play/src/capture.rs`.

## Two pictures

**Plain** (`RenderOptions::machine == false`, or no capture yet): the tile
grids from the untimed loader, drawn layer 2 under layer 1 from the level's
GFX files and the ROM's palette tables. Tile priority is ignored, chars
0x200+ and the animated tiles are blank, there is no layer 3. This is the
instant preview.

**Machine** (the default once `LevelAssets` exist): the same tile grids,
but every character, colour and compositing rule comes from the running
game. `smw_play::capture_level` launches the level in the in-house
emulator, waits for the fade-in and snapshots (`smw-launch-protocol.md`,
"Boot cache and capture"):

| Input | Source | Used for |
|---|---|---|
| characters 0x000–0x3FF (4bpp) | VRAM from BG1's character base | layers 1 and 2, including the animated tiles' current frame and the GFX32/33 pieces the game uploads |
| the animated Map16 tiles 0x133–0x13A | the live pointer table at `$7E:0FBE` (`map16.md`) | their current frame |
| palette | CGRAM | every colour, including what the game changed after `LoadPalette` |
| layer 3 characters (2bpp) and tilemap | VRAM from BG3's bases | the layer 3 plane |
| `BGMODE`, `TM`, `TS`, `CGADSUB`, `CGWSEL`, `COLDATA`, `INIDISP`, scroll | PPU registers sampled below the status bar IRQ line (`VTIME`) | layer order and colour math |
| tile RAM | `$7E:C800` / `$7F:C800` | the oracle for the untimed loader (`tile_ram_matches_the_viewer`) |

## Composition

Each background becomes an index plane (`plane.rs`): CGRAM index per pixel
plus the tile's priority bit, transparent where the character pixel is 0.
`compose.rs` then resolves every pixel the way the PPU does in mode 1
(`crates/snes-ppu/src/render/composite.rs`), without sprites or windows:

1. BG3 priority tiles first when `BGMODE` bit 3 is set, then BG1 high,
   BG2 high, BG1 low, BG2 low, BG3 high, BG3 low, each only when its `TM`
   (main) or `TS` (sub) bit is set.
2. Colour math when `CGADSUB` has the main layer's bit (bit 5 for the
   backdrop): add or subtract (bit 7) the sub-screen pixel (`CGWSEL` bit 1)
   or `COLDATA`, halved (bit 6) unless the sub screen was empty and fell
   back to the fixed colour. `CGWSEL`'s window regions are evaluated with no
   window active. SMW keeps layer 2 on the sub screen and adds it to the
   backdrop, which is why vanilla levels need this.
3. Master brightness.

Layer 2 is placed where the game shows it relative to layer 1 at the
entrance: the BG2 scroll minus the BG1 scroll (parallax and the initial
layer 2 position both end up there). A background tilemap wraps with its
period (two screens wide, 27 or 32 rows tall); layer 2 objects shift
without wrapping. Verified 2026-10-14: levels 0C5 (BG2 48 pixels above BG1)
and 0E8 (BG2 at half the horizontal scroll) match the emulator exactly.

Layer 3 is laid over the level in level coordinates: BG3's tilemap is
positioned relative to BG1's camera (`level pixel − BG1 scroll + BG3
scroll`), repeats with its size, and the rows the status bar occupies (the
top-of-frame BG3 scroll up to the IRQ line) stay transparent.

## The pixel oracle

`solar diff-view <rom> <level>` and `smw-play/tests/view_oracle.rs`
compare the machine picture against the emulator's own frame with sprites
hidden, over frame rows `VTIME + 1 .. 224`. The frame's top-left corner is
level pixel (`BG1 hofs`, `BG1 vofs + 1`): the PPU shows tilemap row
`vofs + line + 1` on line `line` (`docs/emulation/ppu.md`). Verified
2026-10-14: levels 105, 001, 0C5, 0E8 and 101 match with **0 differing
pixels**; the oracle test covers them and skips levels with HDMA active.

## In the editor

The GUI shows the machine picture by default (View → "Machine-exact
view"; "Layer 3" beside it). A capture runs on its own thread
(`solar-magic/src/capture.rs`, `CaptureSession`), cloning a console the
document booted once (`BootCache` behind a mutex, forgotten when the ROM is
replaced by Save). The level view's status bar says "capturing machine
view… Ns" meanwhile and "machine view" once the assets are in; the plain
picture is shown until then, so editing never waits for the emulator.

What triggers a capture is the level's `SceneKey`
(`smw-render/src/scene/cache.rs`): the two headers, the layer 2 page set
and Lunar Magic's per-level records. Moving, adding or deleting objects,
sprites and exits never recaptures; the scene is rebuilt from the cached
palette, GFX, Map16 sets and assets (`LevelScene::from_level_cached`), and
only the loader runs again. Changing a header field (tileset, palettes,
mode, scroll, entrance, screens) starts one new capture; one runs at a
time, and a result for a key the level has since moved away from is
dropped and the capture repeated. A capture that fails (a level the
emulator cannot fade in, a mode other than 1) is remembered for its key
and the status bar shows why the plain picture is used.

## Still approximate

Sprites are drawn as markers (their characters are captured, `sprite_chars`,
for later). HDMA gradients and window effects are not composed. The
animation frame is the one at capture time (the picture does not animate).
Layer 3 is static at its initial scroll. Levels in other background modes
(mode 7 boss rooms) fall back to the plain picture.
