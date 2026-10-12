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
- Output dots 22-277 are composited one at a time as the raster passes
  them, so writes to the compositor's inputs (brightness, `TM`/`TS`,
  windows, colour math, fixed colour) take effect on the exact dot, while
  scroll, tilemap and character changes apply from the next line (the
  hardware fetches tiles in 8-dot slots; mid-line scroll changes are the
  gap to close for full dot accuracy).
- Compositing: for the main and sub screens, walk the mode's priority
  chart (`render/color.rs`, from fullsnes) with mode 1's BG3 promotion;
  colour math per `CGWSEL`/`CGADSUB` (region select for clip-to-black and
  prevent, subscreen or fixed colour, half unless the subscreen fell back
  to the fixed colour, sprite palettes 4-7 only), direct colour for 8 bpp
  BG1, then master brightness. Output is BGR555 in `Frame`; `Frame::to_rgba`
  converts.

Vertical scroll shows row `VOFS + line`, i.e. the picture is one line
"late" relative to the register, as on hardware. Sprites on line V use row
`V − 1 − Y`.

## Not yet modelled

Hi-res (modes 5/6, pseudo-hires) and interlace render at 256 pixels with
no field handling; offset-per-tile (modes 2/4/6) is ignored; OAM/CGRAM
display-period access corruption; the exact dot of each tile fetch.

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
next to them: on 2026-10-12, 48 of 72 match exactly or within 5‰, including
all 22 65816 instruction tests, BG map modes 2/4/8 bpp with flips, HiColor
HDMA (64 and 128 colours per tile row), Mode 7 HDMA, 9-bit and indirect
HDMA, windows and the mode 7 demos' static frames. The rest are the
unimplemented interlace and pseudo-hires modes, mode 5 mosaic, and demos
that animate (page-cycling SPC700 result screens, rotating mode 7, wave
HDMA), which `SNES_TEST_ROMS_REPORT=1` lists without asserting.
