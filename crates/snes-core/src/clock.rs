//! The master clock: where the raster is, and which timing events a step
//! of the CPU crossed.
//!
//! Positions are master cycles within the current scanline (1364 per line,
//! 4 per dot). The clock steps line by line, so the short line (1360
//! cycles, line 240 of odd non-interlaced NTSC fields) and the long PAL
//! interlace line are exact. Dot 323 and 327 are six cycles long, which only
//! matters for the dot number reported to the PPU latches and the H timer.
//! Sources: fullsnes "SNES Timing H/V Events", bsnes `ppu/counter`.

use bitflags::bitflags;

/// Television standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Region {
    /// 60 Hz, 262 lines.
    Ntsc,
    /// 50 Hz, 312 lines.
    Pal,
}

bitflags! {
    /// Timing points crossed by one clock step.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Events: u16 {
        /// A new scanline began (H = 0).
        const LINE_START = 1 << 0;
        /// A new frame began (V wrapped to 0).
        const FRAME_START = 1 << 1;
        /// H = 0.5 dot: the vblank NMI flag is set on the first vblank line.
        const NMI_POINT = 1 << 2;
        /// H = 1 dot: the hblank flag clears.
        const HBLANK_CLEAR = 1 << 3;
        /// H = 2.5 dots: the V-only IRQ fires on its line.
        const VIRQ_POINT = 1 << 4;
        /// H = 6 dots: HDMA registers reload on line 0.
        const HDMA_INIT = 1 << 5;
        /// H ≈ 32.5 dots: auto-joypad reading starts on the first vblank line.
        const JOYPAD_START = 1 << 6;
        /// H ≈ 133.5 dots: DRAM refresh stalls the CPU for 40 cycles.
        const REFRESH = 1 << 7;
        /// H = 274 dots: the hblank flag sets.
        const HBLANK_SET = 1 << 8;
        /// H = 278 dots: HDMA transfers run on visible lines.
        const HDMA_RUN = 1 << 9;
        /// H = HTIME + 3.5 dots: the H timer matched.
        const HIRQ_POINT = 1 << 10;
    }
}

/// Fixed thresholds in master cycles within a line, with their events.
const THRESHOLDS: [(u16, Events); 9] = [
    (2, Events::NMI_POINT),
    (4, Events::HBLANK_CLEAR),
    (10, Events::VIRQ_POINT),
    (24, Events::HDMA_INIT),
    (130, Events::JOYPAD_START),
    (538, Events::REFRESH),
    (1096, Events::HBLANK_SET),
    (1112, Events::HDMA_RUN),
    (0, Events::empty()),
];

/// Master cycles in an ordinary scanline.
pub const LINE_CYCLES: u16 = 1364;
/// First vblank line without overscan.
pub const VDISP_NORMAL: u16 = 225;
/// First vblank line with overscan.
pub const VDISP_OVERSCAN: u16 = 240;

/// The machine's position in time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clock {
    /// Master cycles since power-on.
    pub now: u64,
    line_start: u64,
    /// Current scanline.
    pub v: u16,
    /// Frames since power-on.
    pub frame: u64,
    /// Interlace field (toggles every frame).
    pub field: bool,
    /// Television standard.
    pub region: Region,
    /// Interlaced display, as sampled at the start of this frame.
    pub interlace: bool,
    /// 239-line display, as sampled at the start of this line.
    pub overscan: bool,
    /// Length of the current line in master cycles.
    line_len: u16,
    /// The H timer's position in master cycles, or `None` when off the line.
    hirq_cycles: Option<u16>,
}

impl Clock {
    /// Power-on: frame 0, line 0, field 0.
    #[must_use]
    pub const fn new(region: Region) -> Self {
        Self {
            now: 0,
            line_start: 0,
            v: 0,
            frame: 0,
            field: false,
            region,
            interlace: false,
            overscan: false,
            line_len: LINE_CYCLES,
            hirq_cycles: None,
        }
    }

    /// Master cycles into the current line.
    #[must_use]
    pub const fn h_cycles(&self) -> u16 {
        crate::bits::lo16_64(self.now.wrapping_sub(self.line_start))
    }

    /// Dot within the current line (0-339), with the two long dots.
    #[must_use]
    pub const fn h_dot(&self) -> u16 {
        let h = self.h_cycles();
        if self.line_len == 1360 {
            h >> 2
        } else {
            let first: u16 = if h > 1292 { 2 } else { 0 };
            let second: u16 = if h > 1310 { 2 } else { 0 };
            let adjust = first.saturating_add(second);
            h.saturating_sub(adjust) >> 2
        }
    }

    /// First vblank line.
    #[must_use]
    pub const fn vdisp(&self) -> u16 {
        if self.overscan { VDISP_OVERSCAN } else { VDISP_NORMAL }
    }

    /// Whether the raster is in vblank.
    #[must_use]
    pub const fn vblank(&self) -> bool {
        self.v >= self.vdisp()
    }

    /// Whether the raster is in hblank (dot 274 to dot 0).
    #[must_use]
    pub const fn hblank(&self) -> bool {
        let h = self.h_cycles();
        h >= 1096 || h < 4
    }

    /// Lines in the current frame.
    #[must_use]
    pub const fn frame_lines(&self) -> u16 {
        let base: u16 = match self.region {
            Region::Ntsc => 262,
            Region::Pal => 312,
        };
        if self.interlace && self.field { base.saturating_add(1) } else { base }
    }

    const fn line_len_for(&self, v: u16) -> u16 {
        match self.region {
            Region::Ntsc if !self.interlace && self.field && v == 240 => 1360,
            Region::Pal if self.interlace && self.field && v == 311 => 1368,
            Region::Ntsc | Region::Pal => LINE_CYCLES,
        }
    }

    /// Programs the H timer: `HTIME` in dots.
    pub const fn set_htime(&mut self, htime: u16) {
        let cycles = (htime & 0x1FF).saturating_mul(4).saturating_add(14);
        self.hirq_cycles = if cycles < LINE_CYCLES { Some(cycles) } else { None };
    }

    /// Events whose threshold lies in `(from, to]`.
    fn crossed(&self, from: u16, to: u16) -> Events {
        let mut ev = Events::empty();
        for &(pos, flag) in &THRESHOLDS {
            if !flag.is_empty() && from < pos && pos <= to {
                ev |= flag;
            }
        }
        if let Some(pos) = self.hirq_cycles
            && from < pos
            && pos <= to
        {
            ev |= Events::HIRQ_POINT;
        }
        ev
    }

    /// Advances by `n` master cycles (at most one line) and reports what
    /// was crossed.
    pub fn advance(&mut self, n: u16) -> Events {
        let before = self.h_cycles();
        let after = before.saturating_add(n);
        let mut ev = Events::empty();
        if after >= self.line_len {
            ev |= self.crossed(before, self.line_len);
            let spill = after.wrapping_sub(self.line_len);
            self.line_start = self.line_start.wrapping_add(u64::from(self.line_len));
            ev |= self.next_line();
            ev |= self.crossed(0, spill);
        } else {
            ev |= self.crossed(before, after);
        }
        self.now = self.now.wrapping_add(u64::from(n));
        ev
    }

    fn next_line(&mut self) -> Events {
        let mut ev = Events::LINE_START;
        self.v = self.v.wrapping_add(1);
        if self.v >= self.frame_lines() {
            self.v = 0;
            self.frame = self.frame.wrapping_add(1);
            self.field = !self.field;
            ev |= Events::FRAME_START;
        }
        self.line_len = self.line_len_for(self.v);
        ev
    }

    /// The PPU-facing position.
    #[must_use]
    pub const fn position(&self) -> snes_ppu::Position {
        snes_ppu::Position { v: self.v, h_dot: self.h_dot(), vdisp: self.vdisp() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_is_262_lines_of_1364_cycles() {
        let mut c = Clock::new(Region::Ntsc);
        let mut lines = 0;
        let mut frames = 0;
        for _ in 0..(262 * 1364 / 4) {
            let ev = c.advance(4);
            lines += u32::from(ev.contains(Events::LINE_START));
            frames += u32::from(ev.contains(Events::FRAME_START));
        }
        assert_eq!((lines, frames, c.v, c.frame), (262, 1, 0, 1));
        assert!(c.field);
    }

    #[test]
    fn events_fire_once_per_line_in_order() {
        let mut c = Clock::new(Region::Ntsc);
        let mut seen = Vec::new();
        for _ in 0..(1364 / 4) {
            let ev = c.advance(4);
            for flag in [
                Events::NMI_POINT,
                Events::HBLANK_CLEAR,
                Events::VIRQ_POINT,
                Events::HDMA_INIT,
                Events::JOYPAD_START,
                Events::REFRESH,
                Events::HBLANK_SET,
                Events::HDMA_RUN,
                Events::LINE_START,
            ] {
                if ev.contains(flag) {
                    seen.push(flag);
                }
            }
        }
        assert_eq!(
            seen,
            [
                Events::NMI_POINT,
                Events::HBLANK_CLEAR,
                Events::VIRQ_POINT,
                Events::HDMA_INIT,
                Events::JOYPAD_START,
                Events::REFRESH,
                Events::HBLANK_SET,
                Events::HDMA_RUN,
                Events::LINE_START,
            ]
        );
    }

    #[test]
    fn short_line_on_odd_field() {
        let mut c = Clock::new(Region::Ntsc);
        // Run one full frame so field = 1, then to line 240.
        while !(c.frame == 1 && c.v == 240) {
            c.advance(8);
        }
        assert_eq!(c.line_len, 1360);
        assert_eq!(c.h_dot(), c.h_cycles() >> 2);
    }

    #[test]
    fn long_dots_shift_the_dot_number() {
        let mut c = Clock::new(Region::Ntsc);
        while c.h_cycles() < 1292 {
            c.advance(4);
        }
        assert_eq!(c.h_dot(), 323);
        c.advance(4);
        assert_eq!(c.h_dot(), 323); // dot 323 lasts 6 cycles
        c.advance(2);
        assert_eq!(c.h_dot(), 324);
    }

    #[test]
    fn h_timer_event() {
        let mut c = Clock::new(Region::Ntsc);
        c.set_htime(100);
        let mut fired_at = None;
        for _ in 0..400 {
            if c.advance(4).contains(Events::HIRQ_POINT) {
                fired_at = Some(c.h_cycles());
                break;
            }
        }
        assert_eq!(fired_at, Some(416)); // 100*4 + 14, rounded up to the step
    }
}
