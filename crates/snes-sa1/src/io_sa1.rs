//! Registers the SA-1 owns (`$2209-$2215`, `$2225`, `$2227`, `$222A`,
//! `$2230`, `$2238-$224F`, `$2250-$225B`, `$2301-$230D`).
//! fullsnes "Interrupt/Control on SA-1 Side", "Timer", "DMA Transfers",
//! "Arithmetic Maths", "Variable-Length Bit Processing"; ares
//! `readIOSA1`/`writeIOSA1`.

use crate::Chip;
use crate::bits::{byte64, lo16, set_byte16, set_byte24};

impl Chip {
    /// An SA-1 register read (`$FF` where nothing answers, as ares).
    pub(crate) fn read_io_sa1(&mut self, reg: u16) -> u8 {
        match reg {
            0x2301 => {
                let io = &self.io;
                (io.smeg & 0x0F)
                    | if io.sa1_nmifl { 0x10 } else { 0 }
                    | if io.dma_irqfl { 0x20 } else { 0 }
                    | if io.timer_irqfl { 0x40 } else { 0 }
                    | if io.sa1_irqfl { 0x80 } else { 0 }
            }
            0x2302 => {
                self.latch_counters();
                self.io.hcr.to_le_bytes()[0]
            }
            0x2303 => self.io.hcr.to_le_bytes()[1],
            0x2304 => self.io.vcr.to_le_bytes()[0],
            0x2305 => self.io.vcr.to_le_bytes()[1],
            0x2306..=0x230A => {
                byte64(self.io.mr, lo16(u32::from(reg.wrapping_sub(0x2306))).to_le_bytes()[0])
            }
            0x230B => {
                if self.io.overflow {
                    0x80
                } else {
                    0
                }
            }
            0x230C => self.vdp_read(false),
            0x230D => self.vdp_read(true),
            _ => 0xFF,
        }
    }

    /// An SA-1 register write.
    pub(crate) fn write_io_sa1(&mut self, reg: u16, v: u8) {
        match reg {
            0x2209 => self.write_scnt(v),
            0x220A => self.write_cie(v),
            0x220B => self.write_cic(v),
            0x220C => self.io.snv = set_byte16(self.io.snv, 0, v),
            0x220D => self.io.snv = set_byte16(self.io.snv, 1, v),
            0x220E => self.io.siv = set_byte16(self.io.siv, 0, v),
            0x220F => self.io.siv = set_byte16(self.io.siv, 1, v),
            0x2210 => {
                self.io.hen = v & 1 != 0;
                self.io.ven = v & 2 != 0;
                self.io.hvselb = v & 0x80 != 0;
            }
            0x2211 => self.restart_timer(),
            0x2212 => self.io.hcnt = set_byte16(self.io.hcnt, 0, v),
            0x2213 => self.io.hcnt = set_byte16(self.io.hcnt, 1, v) & 0x1FF,
            0x2214 => self.io.vcnt = set_byte16(self.io.vcnt, 0, v),
            0x2215 => self.io.vcnt = set_byte16(self.io.vcnt, 1, v) & 0x1FF,
            0x2225 => {
                self.io.cbm = v & 0x7F;
                self.io.sw46 = v & 0x80 != 0;
            }
            0x2227 => self.io.cwen = v & 0x80 != 0,
            0x222A => self.io.ciwp = v,
            0x2230 => self.write_dcnt(v),
            0x2231..=0x2237 => self.write_io_shared(reg, v),
            0x2238 => self.io.dtc = set_byte16(self.io.dtc, 0, v),
            0x2239 => self.io.dtc = set_byte16(self.io.dtc, 1, v),
            0x223F => self.io.bbf = v & 0x80 != 0,
            0x2240..=0x224F => self.write_brf(reg, v),
            0x2250..=0x2254 => self.write_arith(reg, v),
            0x2258 => self.vbd_write(v),
            0x2259 => self.io.va = set_byte24(self.io.va, 0, v),
            0x225A => self.io.va = set_byte24(self.io.va, 1, v),
            0x225B => {
                self.io.va = set_byte24(self.io.va, 2, v);
                self.io.vbit = 0;
            }
            _ => {}
        }
    }

    /// `SCNT`: message, vector selects and IRQ to the S-CPU.
    const fn write_scnt(&mut self, v: u8) {
        let io = &mut self.io;
        io.cmeg = v & 0x0F;
        io.cpu_nvsw = v & 0x10 != 0;
        io.cpu_ivsw = v & 0x40 != 0;
        io.cpu_irq = v & 0x80 != 0;
        if io.cpu_irq {
            io.cpu_irqfl = true;
            if io.cpu_irqen {
                io.cpu_irqcl = false;
            }
        }
    }

    /// `CIE`: enabling an interrupt whose flag is already set delivers it.
    const fn write_cie(&mut self, v: u8) {
        let io = &mut self.io;
        if !io.sa1_nmien && v & 0x10 != 0 && io.sa1_nmifl {
            io.sa1_nmicl = false;
        }
        if !io.dma_irqen && v & 0x20 != 0 && io.dma_irqfl {
            io.dma_irqcl = false;
        }
        if !io.timer_irqen && v & 0x40 != 0 && io.timer_irqfl {
            io.timer_irqcl = false;
        }
        if !io.sa1_irqen && v & 0x80 != 0 && io.sa1_irqfl {
            io.sa1_irqcl = false;
        }
        io.sa1_nmien = v & 0x10 != 0;
        io.dma_irqen = v & 0x20 != 0;
        io.timer_irqen = v & 0x40 != 0;
        io.sa1_irqen = v & 0x80 != 0;
    }

    /// `CIC`: acknowledges the SA-1's interrupts.
    const fn write_cic(&mut self, v: u8) {
        let io = &mut self.io;
        io.sa1_nmicl = v & 0x10 != 0;
        io.dma_irqcl = v & 0x20 != 0;
        io.timer_irqcl = v & 0x40 != 0;
        io.sa1_irqcl = v & 0x80 != 0;
        if io.sa1_nmicl {
            io.sa1_nmifl = false;
        }
        if io.sa1_irqcl {
            io.sa1_irqfl = false;
        }
        if io.timer_irqcl {
            io.timer_irqfl = false;
        }
        if io.dma_irqcl {
            io.dma_irqfl = false;
        }
    }

    /// `DCNT`: DMA source/destination, conversion type, enable.
    const fn write_dcnt(&mut self, v: u8) {
        let io = &mut self.io;
        io.sd = v & 3;
        io.dd = v & 4 != 0;
        io.cdsel = v & 0x10 != 0;
        io.cden = v & 0x20 != 0;
        io.dprio = v & 0x40 != 0;
        io.dmaen = v & 0x80 != 0;
        if !io.dmaen {
            self.cc2_line = 0;
        }
    }

    /// `BRF`: the bitmap register file; completing a row of eight pixels
    /// converts it (type 2).
    fn write_brf(&mut self, reg: u16, v: u8) {
        let index = usize::from(reg.wrapping_sub(0x2240) & 0x0F);
        if let Some(slot) = self.io.brf.get_mut(index) {
            *slot = v;
        }
        if (index == 7 || index == 15) && self.io.dmaen && self.io.cden && !self.io.cdsel {
            self.dma_cc2();
        }
    }
}
