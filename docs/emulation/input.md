# Controller input (Play panel)

The Play panel feeds controller 1 from the keyboard (while the picture is
focused or hovered) and from a gamepad, OR-ed together. Gamepads go through
[gilrs](https://crates.io/crates/gilrs) and its bundled SDL
GameControllerDB, behind the `gamepad` feature of `solar-magic` (on by
default; `--no-default-features` drops it and the Linux `libudev`
dependency). Code: `crates/solar-magic/src/play/{gamepad,pad_kind}.rs`.

## Which pad

The pad that last pressed a button or pushed a stick past the dead zone
(0.5) drives controller 1; with none used yet, the first connected pad.
Disconnecting the active pad falls back to the first connected one. The
panel shows its name and button legend.

## Mapping

Buttons map by position, so the SNES layout is kept (SNES B is the bottom
face button, as on the real pad):

| SNES   | Xbox / Steam | PlayStation | gilrs                         |
|--------|--------------|-------------|-------------------------------|
| B      | A            | ✕           | `South`                       |
| A      | B            | ○           | `East`                        |
| Y      | X            | □           | `West`                        |
| X      | Y            | △           | `North`                       |
| L      | LB or LT     | L1 or L2    | `LeftTrigger`, `LeftTrigger2` |
| R      | RB or RT     | R1 or R2    | `RightTrigger`, `RightTrigger2` |
| Select | View         | Create      | `Select`                      |
| Start  | Menu         | Options     | `Start`                       |
| D-pad  | D-pad / left stick | D-pad / left stick | `DPad*`, `LeftStickX/Y` |

The family (Xbox, PlayStation, Steam, other) comes from the USB vendor ID
(Microsoft `045E`, Sony `054C`, Valve `28DE`) and the pad name. Names
containing "Steam" or "Valve" are checked first, because Steam Input's
virtual pad reports the Xbox 360 IDs on Linux.

## Supported pads

- **Xbox One / Series / Elite / 360**: SDL database entries on macOS,
  Windows and Linux. On macOS, Xbox One and Series pads work over
  Bluetooth. Wired Xbox 360/One pads need a third-party driver there.
- **DualSense (PS5) / DualShock 4**: USB and Bluetooth on all three.
- **Steam Controller / Steam Deck**: through Steam Input. With Steam
  running, Steam exposes a virtual pad ("Steam Virtual Gamepad", an
  XInput device on Windows, a uinput Xbox 360 pad on Linux) that gilrs
  maps. On Linux the database also maps the raw Valve Steam Controller and
  Steam Deck. On macOS there is no system-wide virtual pad, and without
  Steam the Steam Controller is a keyboard and mouse ("lizard mode"). No
  native HID driver is planned (decision 2026-10-15).
- **Anything else**: if the SDL database has no entry, the panel warns
  that the pad is unrecognised. Add a mapping line (from SDL's
  `controllermap` or the online GameControllerDB) to the
  `SDL_GAMECONTROLLERCONFIG` environment variable, which gilrs reads at
  start-up.
