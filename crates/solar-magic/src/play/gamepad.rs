//! Gamepad input (feature `gamepad`, on by default, through gilrs and its
//! bundled SDL controller database). The pad last used drives controller 1
//! alongside the keyboard: face buttons in SNES positions (south B, east A,
//! west Y, north X), shoulders or triggers L/R, D-pad or left stick for
//! directions. Xbox and `DualSense` pads map directly; the Steam
//! Controller and Steam Deck come through Steam Input's virtual pad.

use snes_core::Buttons;

use super::pad_kind::PadKind;
#[cfg(feature = "gamepad")]
use super::pad_kind::{STICK_DEADZONE, stick_to_dpad};

/// The pad driving controller 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PadInfo {
    /// Name reported by the driver or the SDL database.
    pub(crate) name: String,
    /// Controller family.
    pub(crate) kind: PadKind,
    /// Whether the buttons have a known layout (SDL database or driver).
    pub(crate) mapped: bool,
}

/// Pad button to SNES button.
#[cfg(feature = "gamepad")]
const MAP: [(gilrs::Button, Buttons); 14] = [
    (gilrs::Button::South, Buttons::B),
    (gilrs::Button::East, Buttons::A),
    (gilrs::Button::West, Buttons::Y),
    (gilrs::Button::North, Buttons::X),
    (gilrs::Button::LeftTrigger, Buttons::L),
    (gilrs::Button::RightTrigger, Buttons::R),
    (gilrs::Button::LeftTrigger2, Buttons::L),
    (gilrs::Button::RightTrigger2, Buttons::R),
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
    /// The pad that last pressed a button or pushed a stick.
    active: Option<gilrs::GamepadId>,
}

#[cfg(feature = "gamepad")]
impl Gamepads {
    /// Opens the gamepad backend (`None` when it is unavailable).
    pub(crate) fn new() -> Option<Self> {
        gilrs::Gilrs::new().ok().map(|gilrs| Self { gilrs, active: None })
    }

    /// Buttons held on the active pad.
    pub(crate) fn read(&mut self) -> Buttons {
        use gilrs::Axis;
        self.drain_events();
        let Some(pad) = self.pad() else {
            return Buttons::empty();
        };
        let mut held = Buttons::empty();
        for (button, snes) in MAP {
            if pad.is_pressed(button) {
                held |= snes;
            }
        }
        let axis = |a: Axis| pad.axis_data(a).map_or(0.0, gilrs::ev::state::AxisData::value);
        held | stick_to_dpad(axis(Axis::LeftStickX), axis(Axis::LeftStickY))
    }

    /// The active pad, for the panel.
    pub(crate) fn active(&self) -> Option<PadInfo> {
        let pad = self.pad()?;
        Some(PadInfo {
            name: pad.name().to_owned(),
            kind: PadKind::classify(pad.vendor_id(), pad.name()),
            mapped: pad.mapping_source() != gilrs::MappingSource::None,
        })
    }

    /// The pad last used, else the first connected one.
    fn pad(&self) -> Option<gilrs::Gamepad<'_>> {
        self.active
            .and_then(|id| self.gilrs.connected_gamepad(id))
            .or_else(|| self.gilrs.gamepads().next().map(|(_, pad)| pad))
    }

    /// Updates the cached pad state and follows the pad being used, so a
    /// second pad (or Steam Input's virtual pad beside the physical one)
    /// takes over controller 1 as soon as it is touched.
    fn drain_events(&mut self) {
        use gilrs::EventType;
        while let Some(event) = self.gilrs.next_event() {
            let used = match event.event {
                EventType::ButtonPressed(..) => true,
                EventType::AxisChanged(_, value, _) => value.abs() > STICK_DEADZONE,
                EventType::ButtonRepeated(..)
                | EventType::ButtonReleased(..)
                | EventType::ButtonChanged(..)
                | EventType::Connected
                | EventType::Disconnected
                | EventType::Dropped
                | EventType::ForceFeedbackEffectCompleted
                | _ => false,
            };
            if used {
                self.active = Some(event.id);
            } else if event.event == EventType::Disconnected && self.active == Some(event.id) {
                self.active = None;
            }
        }
    }
}

/// No gamepad support compiled in.
#[cfg(not(feature = "gamepad"))]
pub(crate) struct Gamepads;

#[cfg(not(feature = "gamepad"))]
#[allow(
    clippy::unused_self,
    clippy::needless_pass_by_ref_mut,
    reason = "same signatures as the gilrs-backed type"
)]
impl Gamepads {
    /// Always `None`.
    pub(crate) const fn new() -> Option<Self> {
        None
    }

    /// Nothing held.
    pub(crate) const fn read(&mut self) -> Buttons {
        Buttons::empty()
    }

    /// No pad.
    pub(crate) const fn active(&self) -> Option<PadInfo> {
        None
    }
}

impl std::fmt::Debug for Gamepads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Gamepads")
    }
}
