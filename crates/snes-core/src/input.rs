//! Controller state as the games see it.

use bitflags::bitflags;

bitflags! {
    /// One standard controller's buttons, in the order the hardware shifts
    /// them out (B first) and the auto-read registers store them
    /// (`JOYnH` = BYsSUDLR, `JOYnL` = AXLR0000).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
    pub struct Buttons: u16 {
        /// B.
        const B = 1 << 15;
        /// Y.
        const Y = 1 << 14;
        /// Select.
        const SELECT = 1 << 13;
        /// Start.
        const START = 1 << 12;
        /// D-pad up.
        const UP = 1 << 11;
        /// D-pad down.
        const DOWN = 1 << 10;
        /// D-pad left.
        const LEFT = 1 << 9;
        /// D-pad right.
        const RIGHT = 1 << 8;
        /// A.
        const A = 1 << 7;
        /// X.
        const X = 1 << 6;
        /// L.
        const L = 1 << 5;
        /// R.
        const R = 1 << 4;
    }
}

/// Both controller ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Input {
    /// Port 1.
    pub p1: Buttons,
    /// Port 2.
    pub p2: Buttons,
}
