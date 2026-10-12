//! Registers the S-CPU owns (`$2200-$2208`, `$2220-$2224`, `$2226`,
//! `$2228-$2229`, `$2300`) and the shared DMA registers (`$2231-$2237`).
//! fullsnes "Interrupt/Control on SNES Side", "Memory Control"; ares
//! `writeIOCPU`/`writeIOShared`.

use crate::Chip;
use crate::bits::{set_byte16, set_byte24};

impl Chip {
    /// An S-CPU register read; `None` is open bus (`VC` does not exist).
    pub(crate) const fn read_io_cpu(&self, reg: u16) -> Option<u8> {
        if reg != 0x2300 {
            return None;
        }
        let io = &self.io;
        Some(
            (io.cmeg & 0x0F)
                | if io.cpu_nvsw { 0x10 } else { 0 }
                | if io.chdma_irqfl { 0x20 } else { 0 }
                | if io.cpu_ivsw { 0x40 } else { 0 }
                | if io.cpu_irqfl { 0x80 } else { 0 },
        )
    }

    /// An S-CPU register write.
    pub(crate) fn write_io_cpu(&mut self, reg: u16, v: u8) {
        match reg {
            0x2200 => self.write_ccnt(v),
            0x2201 => self.write_sie(v),
            0x2202 => self.write_sic(v),
            0x2203 => self.io.crv = set_byte16(self.io.crv, 0, v),
            0x2204 => self.io.crv = set_byte16(self.io.crv, 1, v),
            0x2205 => self.io.cnv = set_byte16(self.io.cnv, 0, v),
            0x2206 => self.io.cnv = set_byte16(self.io.cnv, 1, v),
            0x2207 => self.io.civ = set_byte16(self.io.civ, 0, v),
            0x2208 => self.io.civ = set_byte16(self.io.civ, 1, v),
            0x2220..=0x2223 => {
                if let Some(slot) = self.io.mmc.get_mut(usize::from(reg.wrapping_sub(0x2220))) {
                    *slot = (v & 7, v & 0x80 != 0);
                }
            }
            0x2224 => self.io.sbm = v & 0x1F,
            0x2226 => self.io.swen = v & 0x80 != 0,
            0x2228 => self.io.bwp = v & 0x0F,
            0x2229 => self.io.siwp = v,
            0x2231..=0x2237 => self.write_io_shared(reg, v),
            _ => {}
        }
    }

    /// `CCNT`: message, NMI/IRQ to the SA-1, wait, reset. A falling reset
    /// bit restarts the SA-1 CPU at `CRV` and clears its I-RAM protection.
    const fn write_ccnt(&mut self, v: u8) {
        let io = &mut self.io;
        if io.sa1_resb && v & 0x20 == 0 {
            self.pending_reset = true;
            io.ciwp = 0;
        }
        io.smeg = v & 0x0F;
        io.sa1_nmi = v & 0x10 != 0;
        io.sa1_resb = v & 0x20 != 0;
        io.sa1_rdyb = v & 0x40 != 0;
        io.sa1_irq = v & 0x80 != 0;
        if io.sa1_irq {
            io.sa1_irqfl = true;
            if io.sa1_irqen {
                io.sa1_irqcl = false;
            }
        }
        if io.sa1_nmi {
            io.sa1_nmifl = true;
            if io.sa1_nmien {
                io.sa1_nmicl = false;
            }
        }
    }

    /// `SIE`: enabling an interrupt whose flag is already set delivers it.
    const fn write_sie(&mut self, v: u8) {
        let io = &mut self.io;
        if !io.chdma_irqen && v & 0x20 != 0 && io.chdma_irqfl {
            io.chdma_irqcl = false;
        }
        if !io.cpu_irqen && v & 0x80 != 0 && io.cpu_irqfl {
            io.cpu_irqcl = false;
        }
        io.chdma_irqen = v & 0x20 != 0;
        io.cpu_irqen = v & 0x80 != 0;
    }

    /// `SIC`: acknowledges the S-CPU's interrupts.
    const fn write_sic(&mut self, v: u8) {
        let io = &mut self.io;
        io.chdma_irqcl = v & 0x20 != 0;
        io.cpu_irqcl = v & 0x80 != 0;
        if io.chdma_irqcl {
            io.chdma_irqfl = false;
        }
        if io.cpu_irqcl {
            io.cpu_irqfl = false;
        }
    }

    /// `$2231-$2237`, written by either side: conversion parameters and
    /// the DMA addresses; a destination write starts the transfer.
    pub(crate) fn write_io_shared(&mut self, reg: u16, v: u8) {
        match reg {
            0x2231 => {
                self.io.dmacb = (v & 3).min(2);
                self.io.dmasize = ((v >> 2) & 7).min(5);
                self.io.chdend = v & 0x80 != 0;
                if self.io.chdend {
                    self.io.bwram_dma = false;
                }
            }
            0x2232 => self.io.dsa = set_byte24(self.io.dsa, 0, v),
            0x2233 => self.io.dsa = set_byte24(self.io.dsa, 1, v),
            0x2234 => self.io.dsa = set_byte24(self.io.dsa, 2, v),
            0x2235 => self.io.dda = set_byte24(self.io.dda, 0, v),
            0x2236 => {
                self.io.dda = set_byte24(self.io.dda, 1, v);
                if self.io.dmaen {
                    if !self.io.cden && !self.io.dd {
                        self.dma_normal();
                    } else if self.io.cden && self.io.cdsel {
                        self.dma_cc1();
                    }
                }
            }
            0x2237 => {
                self.io.dda = set_byte24(self.io.dda, 2, v);
                if self.io.dmaen && !self.io.cden && self.io.dd {
                    self.dma_normal();
                }
            }
            _ => {}
        }
    }
}
