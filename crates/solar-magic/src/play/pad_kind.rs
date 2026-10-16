//! Controller families: which pad is connected, and how its buttons are
//! labelled. Kept free of gilrs so it builds and tests without the feature.

use snes_core::Buttons;

/// Stick deflection past which the left stick counts as a D-pad press.
pub(crate) const STICK_DEADZONE: f32 = 0.5;

/// A controller family, for button labels and hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PadKind {
    /// Xbox One / Series / 360 and `XInput` pads.
    Xbox,
    /// `DualSense` (PS5) and `DualShock` (PS4).
    PlayStation,
    /// Steam Controller, Steam Deck, or a Steam Input virtual pad.
    Steam,
    /// Anything else the SDL database maps.
    Other,
}

/// USB vendor IDs.
const VENDORS: [(u16, PadKind); 3] =
    [(0x045E, PadKind::Xbox), (0x054C, PadKind::PlayStation), (0x28DE, PadKind::Steam)];

/// Lowercase name fragments, checked in order. Steam comes first: Steam
/// Input's virtual pad reports the Xbox 360 vendor and product on Linux.
const STEAM_NAMES: [&str; 2] = ["steam", "valve"];
const NAMES: [(&str, PadKind); 5] = [
    ("xbox", PadKind::Xbox),
    ("dualsense", PadKind::PlayStation),
    ("ps5", PadKind::PlayStation),
    ("ps4", PadKind::PlayStation),
    ("wireless controller", PadKind::PlayStation),
];

/// Xbox-style face labels (also the Steam Controller and Steam Deck).
const XBOX_LABELS: [(&str, &str); 8] = [
    ("A", "B"),
    ("B", "A"),
    ("X", "Y"),
    ("Y", "X"),
    ("LB/LT", "L"),
    ("RB/RT", "R"),
    ("View", "Select"),
    ("Menu", "Start"),
];
const PLAYSTATION_LABELS: [(&str, &str); 8] = [
    ("✕", "B"),
    ("○", "A"),
    ("□", "Y"),
    ("△", "X"),
    ("L1/L2", "L"),
    ("R1/R2", "R"),
    ("Create", "Select"),
    ("Options", "Start"),
];
const GENERIC_LABELS: [(&str, &str); 8] = [
    ("south", "B"),
    ("east", "A"),
    ("west", "Y"),
    ("north", "X"),
    ("left shoulder", "L"),
    ("right shoulder", "R"),
    ("select", "Select"),
    ("start", "Start"),
];

impl PadKind {
    /// The family of a pad from its USB vendor ID and name.
    pub(crate) fn classify(vendor: Option<u16>, name: &str) -> Self {
        let name = name.to_ascii_lowercase();
        if STEAM_NAMES.iter().any(|s| name.contains(s)) {
            return Self::Steam;
        }
        let by_vendor = vendor.and_then(|v| VENDORS.iter().find(|&&(id, _)| id == v));
        if let Some(&(_, kind)) = by_vendor {
            return kind;
        }
        NAMES.iter().find(|&&(s, _)| name.contains(s)).map_or(Self::Other, |&(_, kind)| kind)
    }

    /// `(pad button, SNES button)` rows for the help line.
    pub(crate) const fn labels(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Xbox | Self::Steam => &XBOX_LABELS,
            Self::PlayStation => &PLAYSTATION_LABELS,
            Self::Other => &GENERIC_LABELS,
        }
    }

    /// The help line: `A: B   B: A   …   D-pad / left stick: D-pad`.
    pub(crate) fn summary(self) -> String {
        let mut line = String::from("D-pad / left stick: D-pad");
        for (pad, snes) in self.labels() {
            line.push_str("   ");
            line.push_str(pad);
            line.push_str(": ");
            line.push_str(snes);
        }
        line
    }
}

/// The D-pad directions a left-stick position presses (`y` up is positive).
pub(crate) fn stick_to_dpad(x: f32, y: f32) -> Buttons {
    let mut held = Buttons::empty();
    if x < -STICK_DEADZONE {
        held |= Buttons::LEFT;
    } else if x > STICK_DEADZONE {
        held |= Buttons::RIGHT;
    }
    if y > STICK_DEADZONE {
        held |= Buttons::UP;
    } else if y < -STICK_DEADZONE {
        held |= Buttons::DOWN;
    }
    held
}

#[cfg(test)]
mod tests {
    use snes_core::Buttons;

    use super::{PadKind, stick_to_dpad};

    #[test]
    fn classifies_by_vendor_and_name() {
        let cases = [
            (Some(0x045E), "Xbox Wireless Controller", PadKind::Xbox),
            (Some(0x045E), "Xbox Series Controller", PadKind::Xbox),
            (Some(0x054C), "DualSense Wireless Controller", PadKind::PlayStation),
            (None, "PS5 Controller", PadKind::PlayStation),
            (Some(0x045E), "Steam Virtual Gamepad", PadKind::Steam),
            (Some(0x28DE), "Valve Steam Deck", PadKind::Steam),
            (Some(0x28DE), "Wireless Steam Controller", PadKind::Steam),
            (None, "XInput Controller (xbox)", PadKind::Xbox),
            (Some(0x0F0D), "HORI Fighting Commander", PadKind::Other),
        ];
        for (vendor, name, kind) in cases {
            assert_eq!(PadKind::classify(vendor, name), kind, "{name}");
        }
    }

    #[test]
    fn labels_follow_the_family() {
        assert_eq!(PadKind::Xbox.labels().first(), Some(&("A", "B")));
        assert_eq!(PadKind::Steam.labels(), PadKind::Xbox.labels());
        assert_eq!(PadKind::PlayStation.labels().first(), Some(&("✕", "B")));
        assert!(PadKind::PlayStation.summary().contains("△: X"));
    }

    #[test]
    fn stick_presses_past_the_deadzone() {
        assert_eq!(stick_to_dpad(0.0, 0.0), Buttons::empty());
        assert_eq!(stick_to_dpad(0.4, -0.4), Buttons::empty());
        assert_eq!(stick_to_dpad(-0.9, 0.0), Buttons::LEFT);
        assert_eq!(stick_to_dpad(0.9, 0.9), Buttons::RIGHT | Buttons::UP);
        assert_eq!(stick_to_dpad(0.0, -0.9), Buttons::DOWN);
    }
}
