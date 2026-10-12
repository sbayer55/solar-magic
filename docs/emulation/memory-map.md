# System bus map

Implemented in `crates/snes-core/src/bus/mod.rs` (`read_untimed` /
`write_untimed`). fullsnes "Memory and I/O Map", read 2026-10-11.

| Address | Device | Notes |
|---|---|---|
| `$7E-$7F:$0000-$FFFF` | WRAM 128 KiB | `$00-$3F/$80-$BF:$0000-$1FFF` mirror the first 8 KiB |
| `$2100-$213F` | PPU | `snes_ppu::Ppu::{read, write}` with the raster position |
| `$2140-$217F` | APU ports | four ports mirrored every 4 bytes |
| `$2180` | `WMDATA` | 17-bit address in `$2181-$2183`, auto-increment |
| `$4016/$4017` | joypad serial | write bit 0 latches; reads shift B first; `$4017` bits 2-4 read 1 |
| `$4200-$420D` | CPU control | `NMITIMEN WRIO WRMPYA WRMPYB WRDIVL WRDIVH WRDIVB HTIMEL HTIMEH VTIMEL VTIMEH MDMAEN HDMAEN MEMSEL` |
| `$4210-$421F` | CPU status | `RDNMI TIMEUP HVBJOY RDIO RDDIVL RDDIVH RDMPYL RDMPYH JOY1L..JOY4H` |
| `$4300-$437F` | DMA channels | `$43x0-$43xB`, `$43xF` mirrors `$43xB`; `$43xC-$43xE` open bus |
| `$70-$7D/$F0-$FF:$0000-$7FFF` | LoROM SRAM | mirrored over its size (2 KiB for SMW), `FF` when unformatted |
| everything else in a cartridge bank | cartridge | overlays first, then the coprocessor, then the mapper (`docs/formats/rom-mapping.md`) |

Open bus: unmapped reads return the memory data register (the last byte
transferred). `RDNMI` bits 4-6, `TIMEUP` bits 0-6, `HVBJOY` bits 1-5,
`$4016` bits 2-7 and `$4017` bits 5-7 come from it.

Power-on values (fullsnes): `WRIO = FF`, ALU operands `FF`, `HTIME/VTIME =
1FF`, `NMITIMEN/MDMAEN/HDMAEN/MEMSEL = 0`, every DMA register `FF`, PPU in
forced blank, WRAM zeroed here (garbage on hardware; zero keeps runs
reproducible).

Writes to `$2100-$2133` are decoded into `snes_ppu::Regs`; VRAM writes are
dropped and reads return zero outside forced blank/vblank; OAM and CGRAM
accesses are not yet restricted during active display.
