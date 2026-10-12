//! Gamepad input (feature `gamepad`, through gilrs). The first connected
//! pad drives controller 1 alongside the keyboard: face buttons in SNES
//! positions (south B, east A, west Y, north X), shoulders L/R, D-pad or
//! left stick for directions.

use snes_core::Buttons;

/// Pad button to SNES button.
#[cfg(feature = "gamepad")]
const MAP: [(gilrs::Button, Buttons); 12] = [
    (gilrs::Button::South, Buttons::B),
    (gilrs::Button::East, Buttons::A),
    (gilrs::Button::West, Buttons::Y),
    (gilrs::Button::North, Buttons::X),
    (gilrs::Button::LeftTrigger, Buttons::L),
    (gilrs::Button::RightTrigger, Buttons::R),
    (gilrs::Button::Start, Buttons::START),
    (gilrs::Button::Select, Buttons::SELECT),
    (gilrs::Button::DPadUp, Buttons::UP),
    (gilrs::Button::DPadDown, Buttons::DOWN),
    (gilrs::Button::DPadLeft, Buttons::LEFT),
    (gilrs::Button::DPadRight, Buttons::RIGHT),
];

/// Connected gamepads.
#[cfg(feature = "gamepad")]
pub(crate) struct Gamepads {
    gilrs: gilrs::Gilrs,
}

#[cfg(feature = "gamepad")]
impl Gamepads {
    /// Opens the gamepad backend (`None` when it is unavailable).
    pub(crate) fn new() -> Option<Self> {
        gilrs::Gilrs::new().ok().map(|gilrs| Self { gilrs })
    }

    /// Buttons held on the first connected pad.
    pub(crate) fn read(&mut self) -> Buttons {
        use gilrs::Axis;
        while self.gilrs.next_event().is_some() {}
        let Some((_, pad)) = self.gilrs.gamepads().next() else {
            return Buttons::empty();
        };
        let mut held = Buttons::empty();
        for (button, snes) in MAP {
            if pad.is_pressed(button) {
                held |= snes;
            }
        }
        let axis = |a: Axis| pad.axis_data(a).map_or(0.0, gilrs::ev::state::AxisData::value);
        let (x, y) = (axis(Axis::LeftStickX), axis(Axis::LeftStickY));
        if x < -0.5 {
            held |= Buttons::LEFT;
        } else if x > 0.5 {
            held |= Buttons::RIGHT;
        }
        if y > 0.5 {
            held |= Buttons::UP;
        } else if y < -0.5 {
            held |= Buttons::DOWN;
        }
        held
    }
}

/// No gamepad support compiled in.
#[cfg(not(feature = "gamepad"))]
pub(crate) struct Gamepads;

#[cfg(not(feature = "gamepad"))]
impl Gamepads {
    /// Always `None`.
    pub(crate) const fn new() -> Option<Self> {
        None
    }

    /// Nothing held.
    pub(crate) const fn read(&mut self) -> Buttons {
        Buttons::empty()
    }
}

impl std::fmt::Debug for Gamepads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Gamepads")
    }
}
