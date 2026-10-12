//! The SNES picture processor.
//!
//! Models the register file and the three video memories exactly as the
//! CPU sees them through `$2100-$213F`, and renders the picture as the
//! raster advances (`Ppu::run_to`; see `docs/emulation/ppu.md`). The PPU
//! does not own the H/V counters: the machine's clock passes the current
//! [`Position`] with every register access.

mod bits;
mod cgram;
mod frame;
mod oam;
mod regs;
mod render;
mod vram;

pub use cgram::Cgram;
pub use frame::{FRAME_HEIGHT, FRAME_WIDTH, Frame};
pub use oam::Oam;
pub use regs::{BgRegs, Mode7Regs, Regs};
pub use vram::Vram;

/// Where the raster is when a register is accessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    /// Scanline.
    pub v: u16,
    /// Dot within the line (0-339).
    pub h_dot: u16,
    /// First vblank line (225, or 240 with overscan).
    pub vdisp: u16,
}

impl Position {
    /// Whether the raster is in the vertical blanking period.
    #[must_use]
    pub const fn vblank(self) -> bool {
        self.v >= self.vdisp
    }
}

/// Counter latch and status flags behind `$213C-$213F`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Status {
    hcounter: u16,
    vcounter: u16,
    latched: bool,
    h_toggle: bool,
    v_toggle: bool,
    field: bool,
    range_over: bool,
    time_over: bool,
}

/// The S-PPU.
#[derive(Debug, Clone)]
pub struct Ppu {
    /// Decoded `$2100-$2133` state.
    pub regs: Regs,
    /// Tile and map memory.
    pub vram: Vram,
    /// Palette memory.
    pub cgram: Cgram,
    /// Sprite attributes.
    pub oam: Oam,
    status: Status,
    /// Open-bus value of the PPU1 data bus (`$2134-$2136`, `$2138-$213A`).
    ppu1_mdr: u8,
    /// Open-bus value of the PPU2 data bus (`$213B-$213F`).
    ppu2_mdr: u8,
    /// Whether `WRIO` bit 7 is high (counter latching enabled).
    latch_enabled: bool,
    /// PAL console.
    pal: bool,
    render: render::Renderer,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new(false)
    }
}

impl Ppu {
    /// Power-on state (forced blank).
    #[must_use]
    pub fn new(pal: bool) -> Self {
        Self {
            regs: Regs::new(),
            vram: Vram::new(),
            cgram: Cgram::new(),
            oam: Oam::new(),
            status: Status::default(),
            ppu1_mdr: 0,
            ppu2_mdr: 0,
            latch_enabled: true,
            pal,
            render: render::Renderer::default(),
        }
    }

    /// Reset: registers return to power-on values, memories keep their contents.
    pub fn reset(&mut self) {
        self.regs = Regs::new();
        self.status = Status::default();
        self.ppu1_mdr = 0;
        self.ppu2_mdr = 0;
        self.latch_enabled = true;
    }

    /// Whether video memory can be accessed: forced blank or vblank.
    #[must_use]
    pub const fn memory_accessible(&self, pos: Position) -> bool {
        self.regs.forced_blank || pos.vblank()
    }

    /// 239-line display (`SETINI` bit 2).
    #[must_use]
    pub const fn overscan(&self) -> bool {
        self.regs.overscan
    }

    /// Interlaced display (`SETINI` bit 0).
    #[must_use]
    pub const fn interlace(&self) -> bool {
        self.regs.interlace
    }

    /// Writes `$2100 + reg`.
    pub fn write(&mut self, reg: u8, v: u8, pos: Position) {
        if let Some(handler) = regs::WRITES.get(usize::from(reg)) {
            handler(self, v, pos);
        }
    }

    /// Reads `$2100 + reg`; `mdr` is the CPU's open-bus value.
    pub fn read(&mut self, reg: u8, mdr: u8, pos: Position) -> u8 {
        match reg {
            0x04 | 0x05 | 0x06 | 0x08 | 0x09 | 0x0A | 0x14 | 0x15 | 0x16 | 0x18 | 0x19 | 0x1A
            | 0x24 | 0x25 | 0x26 | 0x28 | 0x29 | 0x2A => self.ppu1_mdr,
            0x34..=0x36 => self.read_product(reg),
            0x37 => {
                self.latch_counters(pos);
                mdr
            }
            0x38 => {
                let v = self.oam.read();
                self.ppu1(v)
            }
            0x39 => {
                let readable = self.memory_accessible(pos);
                let v = self.vram.read_low(readable);
                self.ppu1(v)
            }
            0x3A => {
                let readable = self.memory_accessible(pos);
                let v = self.vram.read_high(readable);
                self.ppu1(v)
            }
            0x3B => {
                let v = self.cgram.read(self.ppu2_mdr);
                self.ppu2(v)
            }
            0x3C => self.read_hcounter(),
            0x3D => self.read_vcounter(),
            0x3E => self.ppu1((self.ppu1_mdr & 0x10) | self.stat77()),
            0x3F => self.read_stat78(),
            _ => mdr,
        }
    }

    const fn ppu1(&mut self, v: u8) -> u8 {
        self.ppu1_mdr = v;
        v
    }

    const fn ppu2(&mut self, v: u8) -> u8 {
        self.ppu2_mdr = v;
        v
    }

    fn read_product(&mut self, reg: u8) -> u8 {
        let bytes = regs::mode7_product(&self.regs);
        let v = bytes.get(usize::from(reg.wrapping_sub(0x34))).copied().unwrap_or(0);
        self.ppu1(v)
    }

    const fn read_hcounter(&mut self) -> u8 {
        let v = if self.status.h_toggle {
            (self.ppu2_mdr & 0xFE) | bits::hi(self.status.hcounter)
        } else {
            bits::lo(self.status.hcounter)
        };
        self.status.h_toggle = !self.status.h_toggle;
        self.ppu2(v)
    }

    const fn read_vcounter(&mut self) -> u8 {
        let v = if self.status.v_toggle {
            (self.ppu2_mdr & 0xFE) | bits::hi(self.status.vcounter)
        } else {
            bits::lo(self.status.vcounter)
        };
        self.status.v_toggle = !self.status.v_toggle;
        self.ppu2(v)
    }

    /// `STAT77`: time over, range over, PPU1 version 1.
    const fn stat77(&self) -> u8 {
        (if self.status.time_over { 0x80 } else { 0 })
            | (if self.status.range_over { 0x40 } else { 0 })
            | 0x01
    }

    /// `STAT78`: field, latch flag, open bus bit 5, PAL, PPU2 version 3.
    /// Reading resets the counter read toggles and, while latching is
    /// enabled, the latch flag.
    const fn read_stat78(&mut self) -> u8 {
        let v = (if self.status.field { 0x80 } else { 0 })
            | (if self.status.latched { 0x40 } else { 0 })
            | (self.ppu2_mdr & 0x20)
            | (if self.pal { 0x10 } else { 0 })
            | 0x03;
        self.status.h_toggle = false;
        self.status.v_toggle = false;
        if self.latch_enabled {
            self.status.latched = false;
        }
        self.ppu2(v)
    }

    /// Latches the H/V counters (`$2137` read, or `WRIO` bit 7 falling edge).
    pub const fn latch_counters(&mut self, pos: Position) {
        if self.latch_enabled {
            self.status.hcounter = pos.h_dot;
            self.status.vcounter = pos.v;
            self.status.latched = true;
        }
    }

    /// `WRIO` bit 7: a falling edge latches the counters.
    pub const fn set_latch_enabled(&mut self, enabled: bool, pos: Position) {
        if self.latch_enabled && !enabled {
            self.latch_counters(pos);
        }
        self.latch_enabled = enabled;
    }

    /// Start of the vertical blanking period: the OAM address is reloaded.
    pub const fn vblank_start(&mut self) {
        if !self.regs.forced_blank {
            self.oam.reload_address();
        }
    }

    /// Start of a frame.
    pub const fn frame_start(&mut self, field: bool) {
        self.status.field = field;
        self.status.range_over = false;
        self.status.time_over = false;
        self.render.restart_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLANK: Position = Position { v: 230, h_dot: 0, vdisp: 225 };

    #[test]
    fn scroll_write_twice_latch() {
        let mut ppu = Ppu::new(false);
        ppu.write(0x0D, 0x34, BLANK);
        ppu.write(0x0D, 0x12, BLANK);
        assert_eq!(ppu.regs.bg[0].hofs & 0x3FF, 0x234);
        ppu.write(0x0E, 0x78, BLANK);
        ppu.write(0x0E, 0x56, BLANK);
        assert_eq!(ppu.regs.bg[0].vofs, 0x5678);
    }

    #[test]
    fn mode7_product() {
        let mut ppu = Ppu::new(false);
        ppu.write(0x1B, 0x00, BLANK); // M7A = $FF00 = -256
        ppu.write(0x1B, 0xFF, BLANK);
        ppu.write(0x1C, 0x00, BLANK); // M7B high byte = 2
        ppu.write(0x1C, 0x02, BLANK);
        let lo = ppu.read(0x34, 0, BLANK);
        let mid = ppu.read(0x35, 0, BLANK);
        let hi = ppu.read(0x36, 0, BLANK);
        assert_eq!([lo, mid, hi], [0x00, 0xFE, 0xFF]); // -512
    }

    #[test]
    fn counter_latch_and_stat78() {
        let mut ppu = Ppu::new(false);
        let pos = Position { v: 0x101, h_dot: 0x12C, vdisp: 225 };
        assert_eq!(ppu.read(0x37, 0xAB, pos), 0xAB);
        assert_eq!(ppu.read(0x3C, 0, pos), 0x2C);
        assert_eq!(ppu.read(0x3C, 0, pos) & 1, 1);
        assert_eq!(ppu.read(0x3D, 0, pos), 0x01);
        assert_eq!(ppu.read(0x3D, 0, pos) & 1, 1);
        let stat = ppu.read(0x3F, 0, pos);
        assert_eq!(stat & 0x40, 0x40);
        assert_eq!(ppu.read(0x3F, 0, pos) & 0x40, 0);
    }

    #[test]
    fn coldata_sets_channels() {
        let mut ppu = Ppu::new(false);
        ppu.write(0x32, 0x20 | 0x1F, BLANK);
        ppu.write(0x32, 0x40 | 0x01, BLANK);
        ppu.write(0x32, 0x80 | 0x10, BLANK);
        assert_eq!(ppu.regs.fixed_color, 0x4000 | 0x0020 | 0x001F);
    }
}
