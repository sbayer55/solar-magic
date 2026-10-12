# Picture processor

Implemented in `crates/snes-ppu`. fullsnes "SNES PPU" chapters (priority
chart, sprites, VRAM, windows, colour math, rotation/scaling) and ares'
mode 7 arithmetic, read 2026-10-11.

## Registers and memories

`$2100-$2133` are decoded into `Regs` by a table of one function per
register; `$2134-$213F` are read through `Ppu::read` with the PPU1/PPU2
open-bus bytes. VRAM (`$2115-$2119`, `$2139-$213A`: increment modes, the
address remap modes, the read prefetch latch and the "first read repeats"
behaviour), OAM (`$2102-$2104`, `$2138`: pair latch for the low table,
immediate high-table writes, address reload at vblank, priority rotation)
and CGRAM (`$2121-$2122`, `$213B`: byte pairs, bit 7 dropped) keep the port
quirks the CPU can observe. VRAM is write-protected and reads zero outside
forced blank and vblank; OAM/CGRAM access during active display is not
yet restricted.

## Rendering model

The PPU does not own the raster counters: the bus passes a `Position`
(line, dot, first vblank line) with every register access and at every
line start, and `Ppu::run_to` advances a renderer to it.

- At the start of each visible line (1 to 224, or 239 with overscan) the
  layer line buffers are built from the registers as they are at that
  moment: window masks, the sprite line (evaluation of up to 32 sprites
  from `first_sprite`, then up to 34 8×8 tiles, drawn from the lowest
  priority sprite upwards so OAM order wins; the tile budget drops the
  highest-priority sprites first, as bsnes does), and each background
  (tilemap lookup with the four screen layouts, 2/4/8 bpp planar decode,
  16×16 tiles, flips, mosaic; mode 7 through the affine transform with
  the 13-bit clamp, the four repeat modes and EXTBG).
- Hi-res backgrounds (modes 5/6) are rendered as 512 half-pixels per line
  with 16-half-pixel-wide tiles (`BGMODE` 8×8 acts as 16×8) and the
  horizontal scroll doubled; even half-pixels belong to the sub screen,
  odd to the main screen (ares `Background::run`). Every output dot stores
  two half-pixels: in a hi-res mode (5/6 or `SETINI` bit 3 pseudo-hires)
  the sub-screen pixel on the left and the main-screen pixel on the right,
  otherwise the main pixel twice. A lo-res frame reads back as 256 wide,
  a frame with any hi-res dot as 512.
- Interlace (`SETINI` bit 0, latched per frame): the stored picture has two
  rows per line and each field writes its own; hi-res backgrounds sample
  row `2·line + field` (the field bit is dropped under mosaic), other
  modes repeat the line in both fields. `SETINI` bit 1 halves sprites and
  shows alternate rows per field (ares `Object::fetch`, including its
  per-half flip of the 16×32/32×64 sizes).
- Offset-per-tile (modes 2/4/6): for every 8-pixel output column after the
  first, BG1 and BG2 take their horizontal (BG3 tilemap row `BG3VOFS`)
  and vertical (`BG3VOFS + 8`) offsets from BG3's tilemap at
  `(column − 1)·8 + (BG3HOFS & ~7)` when the entry has the layer's valid
  bit (13 for BG1, 14 for BG2); the horizontal offset replaces the coarse
  scroll and keeps the layer's fine scroll, mode 4 uses one entry whose
  bit 15 picks the direction (bsnes/anomie; ares' current code adds a
  never-fetched value there). Mode 6 is implemented but unverified.
- Output dots 22-277 are composited one at a time as the raster passes
  them, so writes to the compositor's inputs (brightness, `TM`/`TS`,
  windows, colour math, fixed colour) take effect on the exact dot, while
  scroll, tilemap and character changes apply from the next line (the
  hardware fetches tiles in 8-dot slots; mid-line scroll changes are the
  gap to close for full dot accuracy).
- Compositing (`render/composite.rs`, after ares `DAC::above`/`below`):
  for the main and sub screens, walk the mode's priority chart
  (`render/color.rs`, from fullsnes) with mode 1's BG3 promotion; colour
  math per `CGWSEL`/`CGADSUB` (region select for clip-to-black and
  prevent, subscreen or fixed colour, half unless the subscreen fell back
  to the fixed colour, sprite palettes 4-7 only), direct colour for 8 bpp
  BG1, then master brightness. In hi-res the sub half-pixel is blended
  with the *previous* dot's main pixel using that dot's math decisions,
  both screens use colour 0 as backdrop, and the first half-pixel of a
  line shows the sub colour unblended (krom's screenshots; ares blanks it
  and notes the state is unconfirmed). Output is BGR555 in `Frame`
  (512×478 storage, read back at the frame's natural size);
  `Frame::to_rgba` converts.

Vertical scroll shows row `VOFS + line`, i.e. the picture is one line
"late" relative to the register, as on hardware. Sprites on line V use row
`V − 1 − Y`.

## Not yet modelled

OAM/CGRAM display-period access corruption; the exact dot of each tile
fetch (mid-line scroll writes); mode 6 offset-per-tile and hi-res mosaic
are unverified (krom's `MosaicMode5` screenshot shows 16-half-pixel blocks
with interlace stripes where ares' model, which this follows, gives
16-pixel blocks without them; no third source to settle it).

## Verification

`cargo solar emu-boot <rom> --frames 600 --out title.png` renders Super
Mario World's title screen; frames 1500/2400 show the attract demo with
sprites, scrolling and priorities (viewed 2026-10-11). `SMW_ROM`-gated test
`title_frame_renders` checks the picture has real content, and compares
it to a hash under `SMW_GOLDENS_DIR` when that is set (write with
`UPDATE_GOLDENS=1`); goldens stay local.

krom's open-source test ROMs (`cargo xtask fetch-test-roms`, then
`SNES_TEST_ROMS=target/test-roms/SNES cargo test -p snes-core --release
--test test_roms`) are compared pixel for pixel with the screenshots shipped
next to them (at the nearest integer scale, since hi-res and interlaced
demos ship 512×448 captures): on 2026-10-12, 56 of 73 match exactly,
including all 22 65816 instruction tests, BG map modes 2/4/8 bpp with
flips, HiColor HDMA (64 and 128 colours per tile row, and the pseudo-hires
variant), the interlaced Myst/RPG/Simpsons demos, Mode 7 HDMA and perspective,
9-bit and indirect HDMA, windows and the mode 7 demos' static frames. The strict
list also drives `MosaicMode3` to its screenshot with 15 scripted R
presses (16×16 mosaic) and accepts the resampled `InterlaceMoogle` capture
within 40‰. The rest are demos that animate or need unknown input
(page-cycling SPC700 result screens, rotating mode 7, wave HDMA, the
scrolling and multi-window demos), `InterlaceFont` (its screenshot sits
nine rows below every emulator's output, including the other interlace
demos' alignment) and `MosaicMode5` (above), which
`SNES_TEST_ROMS_REPORT=1` lists without asserting.
