# Lunar Magic extensions

What Lunar Magic adds to a ROM, as far as is known. Nothing here is
implemented yet; `smw_data::RomLayout::detect` only recognises the signature
and refuses the ROM.

## Detection

Lunar Magic writes an ASCII string beginning `Lunar Magic Version` into the
ROM; community notes place it at `$0F:F0A0`. Solar Magic searches the whole
image for the string rather than trusting the offset. Still to verify against
a real LM-saved ROM.

## Relocated / added tables (unverified, from sneslab.net notes)

| Table | Address | Note |
|---|---|---|
| Lunar Magic version string | `$0F:F0A0` | |
| Per-level sprite data bank bytes | `$0E:F100` | replaces the implied bank `$07` |
| Sprite data pointer routine | `$0E:F300` | |
| Custom overworld sprite data pointer | `$0E:F55D` | |
| Custom palette upload routine | `$0E:F570` | |
| Custom per-level palettes (24-bit pointers per level) | `$0E:F600` | RATS-tagged |
| BG Map16 page set pointer routine | `$0E:FD00` | |
| ExGFX 80–FF pointers | `$0F:F600` | |
| GFX decompression routine | `$0F:F900` | LM's own, replaces `$00:B8DE` |
| Secondary header byte 4 table | `$05:DE00` | |
| Secondary header byte 5 table | `$06:FC00` | |
| Secondary header byte 6 table | `$06:FE00` | |
| Secondary header bytes 5–7 routine | `$05:DD30` | |
| Map16 VRAM data getter | `$06:F540` | |
| Map16 "acts like" getter | `$06:F600` | |
| Custom Map16 block handlers | `$06:F660` | |
| Extended objects 01/02/03 (screen jump, 5-byte exit, vertical jump) | `$0D:E1D0`, `$0D:E1B0`, `$0D:E1E0` | |

Other known extensions: 4× Map16 pages, ExGFX 100–FFF, layer 3 editing,
expanded overworld, SA-1 and ExLoROM mappings, `.mwl` level export. Each needs
its own section once reverse-engineered.
