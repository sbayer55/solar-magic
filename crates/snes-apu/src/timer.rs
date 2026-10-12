//! The three S-SMP timers (`$FA-$FF`): an 8-bit stage counts input ticks
//! (8 kHz for T0/T1, 64 kHz for T2), and each time it reaches the target
//! the 4-bit output increments. Reading the output clears it.

/// One timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timer {
    /// `TnDIV` (0 = 256).
    pub target: u8,
    /// Internal 8-bit counter.
    pub counter: u8,
    /// `TnOUT` (4 bits).
    pub output: u8,
    /// `CONTROL` bit n.
    pub enabled: bool,
}

impl Timer {
    /// `CONTROL` bit written: a 0→1 edge restarts the counter and output.
    pub const fn set_enabled(&mut self, enabled: bool) {
        if enabled && !self.enabled {
            self.counter = 0;
            self.output = 0;
        }
        self.enabled = enabled;
    }

    /// One input tick.
    pub const fn tick(&mut self) {
        if !self.enabled {
            return;
        }
        self.counter = self.counter.wrapping_add(1);
        if self.counter == self.target {
            self.counter = 0;
            self.output = self.output.wrapping_add(1) & 0x0F;
        }
    }

    /// `TnOUT` read: returns and clears.
    pub const fn read_output(&mut self) -> u8 {
        let v = self.output;
        self.output = 0;
        v
    }
}

#[cfg(test)]
mod tests {
    use super::Timer;

    #[test]
    fn counts_to_target_and_clears_on_read() {
        let mut t = Timer { target: 3, ..Timer::default() };
        t.set_enabled(true);
        for _ in 0..7 {
            t.tick();
        }
        assert_eq!(t.read_output(), 2);
        assert_eq!(t.read_output(), 0);
        assert_eq!(t.counter, 1);
    }

    #[test]
    fn zero_target_means_256() {
        let mut t = Timer::default();
        t.set_enabled(true);
        for _ in 0..255 {
            t.tick();
        }
        assert_eq!(t.output, 0);
        t.tick();
        assert_eq!(t.output, 1);
    }
}
