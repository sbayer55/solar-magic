//! Vblank NMI and H/V timer IRQ bookkeeping (`$4200`, `$4207-$420A`,
//! `$4210-$4212`), driven by the clock's events. fullsnes "SNES PPU
//! Interrupts": the NMI flag sets at H=0.5 of the first vblank line and
//! clears at V=0; a pending NMI is the 0→1 edge of `enable AND flag`; the
//! timer flag sets on the compare and clears on a `TIMEUP` read or when the
//! timer is disabled.

use crate::clock::{Clock, Events};

/// H/V timer mode (`NMITIMEN` bits 4-5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IrqMode {
    /// Disabled.
    #[default]
    Off,
    /// `H = HTIME`, any line.
    Horizontal,
    /// `V = VTIME`, `H = 0`.
    Vertical,
    /// `H = HTIME` on line `VTIME`.
    Both,
}

/// Interrupt sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Interrupts {
    /// `NMITIMEN` bit 7.
    pub nmi_enable: bool,
    /// `NMITIMEN` bits 4-5.
    pub irq_mode: IrqMode,
    /// `HTIME` (0-339).
    pub htime: u16,
    /// `VTIME` (0-261/311).
    pub vtime: u16,
    /// `RDNMI` bit 7.
    pub nmi_flag: bool,
    /// An NMI edge the CPU has not taken yet.
    pub nmi_pending: bool,
    /// `TIMEUP` bit 7; also the IRQ line.
    pub irq_flag: bool,
}

impl Interrupts {
    /// Power-on: everything off.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            nmi_enable: false,
            irq_mode: IrqMode::Off,
            htime: 0x1FF,
            vtime: 0x1FF,
            nmi_flag: false,
            nmi_pending: false,
            irq_flag: false,
        }
    }

    /// `$4200 NMITIMEN` (bit 0, auto-joypad, is handled by the caller).
    pub const fn write_nmitimen(&mut self, v: u8) {
        let enable = v & 0x80 != 0;
        if !self.nmi_enable && enable && self.nmi_flag {
            self.nmi_pending = true;
        }
        self.nmi_enable = enable;
        self.irq_mode = match (v >> 4) & 3 {
            0 => IrqMode::Off,
            1 => IrqMode::Horizontal,
            2 => IrqMode::Vertical,
            _ => IrqMode::Both,
        };
        if matches!(self.irq_mode, IrqMode::Off) {
            self.irq_flag = false;
        }
    }

    /// `$4210 RDNMI` read: returns the flag and clears it (a pending edge
    /// still fires).
    pub const fn read_rdnmi(&mut self) -> bool {
        let flag = self.nmi_flag;
        self.nmi_flag = false;
        flag
    }

    /// `$4211 TIMEUP` read: returns the flag and acknowledges the IRQ.
    pub const fn read_timeup(&mut self) -> bool {
        let flag = self.irq_flag;
        self.irq_flag = false;
        flag
    }

    /// Applies the timing events of one clock step.
    pub fn on_events(&mut self, ev: Events, clock: &Clock) {
        if ev.contains(Events::FRAME_START) {
            self.nmi_flag = false;
        }
        if ev.contains(Events::NMI_POINT) && clock.v == clock.vdisp() {
            self.nmi_flag = true;
            if self.nmi_enable {
                self.nmi_pending = true;
            }
        }
        let on_vtime = clock.v == self.vtime;
        let fire = match self.irq_mode {
            IrqMode::Off => false,
            IrqMode::Horizontal => ev.contains(Events::HIRQ_POINT),
            IrqMode::Vertical => ev.contains(Events::VIRQ_POINT) && on_vtime,
            IrqMode::Both if self.htime == 0 => ev.contains(Events::VIRQ_POINT) && on_vtime,
            IrqMode::Both => ev.contains(Events::HIRQ_POINT) && on_vtime,
        };
        if fire {
            self.irq_flag = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Region;

    #[test]
    fn nmi_edge_on_enable_with_flag_set() {
        let mut i = Interrupts::new();
        let mut clock = Clock::new(Region::Ntsc);
        clock.v = 225;
        i.on_events(Events::NMI_POINT, &clock);
        assert!(i.nmi_flag && !i.nmi_pending);
        i.write_nmitimen(0x80);
        assert!(i.nmi_pending);
        assert!(i.read_rdnmi());
        assert!(!i.read_rdnmi());
    }

    #[test]
    fn vertical_irq_only_on_its_line() {
        let mut i = Interrupts::new();
        i.vtime = 100;
        i.write_nmitimen(0x20);
        let mut clock = Clock::new(Region::Ntsc);
        clock.v = 99;
        i.on_events(Events::VIRQ_POINT, &clock);
        assert!(!i.irq_flag);
        clock.v = 100;
        i.on_events(Events::VIRQ_POINT, &clock);
        assert!(i.irq_flag);
        assert!(i.read_timeup());
        assert!(!i.irq_flag);
    }
}
