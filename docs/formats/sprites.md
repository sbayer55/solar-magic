# Sprite data

Implemented in `crates/smw-data/src/level/sprites.rs`. Parser in the game:
`LoadSpriteLoopStrt` ($02:A82E); pointer setup at `$05:xxxx` (bank_05 line
~7250). Verified against the SMWDisX disassembly 2026-10-06.

## Pointer table

`$05:EC00`: 512 × 2-byte pointers, all into bank `$07` (the game hardcodes the
bank byte). Lunar Magic adds a per-level bank byte table at `$0E:F100`.

## Layout

```
byte 0        header: bits 0-5 sprite memory setting ($1692), bits 6-7 buoyancy ($190E)
then per sprite, 3 bytes:
  YYYYEEsy    Y = Y position bits 0-3 (within screen, in 16px tiles)
              E = extra bits (sprite-specific; Lunar Magic shows them as "extra bits")
              s = screen number bit 4
              y = Y position bit 4
  XXXXssss    X = X position within screen (0-15 tiles), s = screen number bits 0-3
  NNNNNNNN    sprite number (0xE7+ are scroll commands: $143E = N - $E7)
terminator    a byte 0xFF where the next sprite's first byte would be
```

Screen number = `s<<4 | ssss` (0–31). The game loads a sprite when its screen
equals the scroll boundary and its X matches the column being revealed.

## Vertical levels (verified in SMWDisX `CODE_02A93C`, 2026-10-07)

The loader swaps the roles of the two position fields exactly as it does for
objects: `YYYY` plus the `y` bit hold the X tile (0–31 across the 32-wide
screen; the `y` bit is X bit 4), the screen bits hold the vertical screen
number, and `XXXX` holds the Y tile within the 16-row screen. The extra bits
stay in bits 2–3 of byte 0. `Sprite::position` / `Sprite::with_position` in
`crates/smw-data/src/level/edit.rs` implement both orientations.
