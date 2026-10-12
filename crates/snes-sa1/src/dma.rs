//! SA-1 DMA: memory-to-memory transfers between ROM, BW-RAM and I-RAM, and
//! the two character conversions that turn packed pixels into planar
//! tiles in an I-RAM buffer. fullsnes "DMA Transfers", "Character
//! Conversion"; ares `sa1/dma.cpp` for the buffer layouts.

use crate::{Chip, Side};

/// Planar byte offset of plane `byte` within a tile row pair:
/// `{0, 1, 16, 17, 32, 33, 48, 49}`.
const fn plane_offset(byte: u32) -> u32 {
    ((byte & 6) << 3).wrapping_add(byte & 1)
}

impl Chip {
    /// Normal DMA (`DTC` bytes from `SDA` to `DDA`), run to completion;
    /// each byte costs one SA-1 cycle to I-RAM, two to BW-RAM.
    pub(crate) fn dma_normal(&mut self) {
        let count = self.io.dtc;
        for _ in 0..count {
            let source = self.io.dsa;
            let target = self.io.dda;
            self.io.dsa = source.wrapping_add(1) & 0xFF_FFFF;
            self.io.dda = target.wrapping_add(1) & 0xFF_FFFF;
            let data = match self.io.sd {
                0 => self.rom_read(source, Side::Sa1),
                1 => self.bwram_read(source),
                2 => self.iram_read(source),
                _ => continue,
            };
            self.step();
            if self.io.dd {
                self.step();
                self.bwram_write(target, data);
            } else {
                self.iram_write(target, data);
            }
        }
        self.io.dtc = 0;
        self.io.dma_irqfl = true;
        if self.io.dma_irqen {
            self.io.dma_irqcl = false;
        }
    }

    /// Type 1 conversion: from now on the S-CPU's BW-RAM reads return
    /// converted tiles; the S-CPU is told the first character is ready.
    pub(crate) const fn dma_cc1(&mut self) {
        self.io.bwram_dma = true;
        self.io.chdma_irqfl = true;
        if self.io.chdma_irqen {
            self.io.chdma_irqcl = false;
        }
    }

    /// Type 1 read: at each character boundary the next character is
    /// converted from the packed bitmap at `SDA` into the I-RAM buffer at
    /// `DDA`; bytes within a character come from that buffer.
    pub(crate) fn dma_cc1_read(&mut self, address: u32) -> u8 {
        let dmacb = u32::from(self.io.dmacb);
        let char_shift = 6u32.wrapping_sub(dmacb);
        let charmask = (1u32 << char_shift).wrapping_sub(1);
        if address & charmask == 0 {
            self.cc1_convert(address, dmacb);
        }
        self.iram_read(self.io.dda.wrapping_add(address & charmask) & 0x7FF)
    }

    fn cc1_convert(&mut self, address: u32, dmacb: u32) {
        let bpp = 2u32 << (2u32.wrapping_sub(dmacb));
        let dmasize = u32::from(self.io.dmasize);
        let bpl = (8u32 << dmasize) >> dmacb;
        let bwmask = u32::try_from(self.bwram.len()).unwrap_or(1).wrapping_sub(1);
        let tile = (address.wrapping_sub(self.io.dsa) & bwmask) >> (6u32.wrapping_sub(dmacb));
        let ty = tile >> dmasize;
        let tx = tile & ((1u32 << dmasize).wrapping_sub(1));
        let mut bwaddr = self
            .io
            .dsa
            .wrapping_add(ty.wrapping_mul(8).wrapping_mul(bpl))
            .wrapping_add(tx.wrapping_mul(bpp));
        for y in 0..8u32 {
            let mut data: u64 = 0;
            for byte in 0..bpp {
                data |=
                    u64::from(self.bwram_read(bwaddr.wrapping_add(byte) & bwmask)) << (byte << 3);
            }
            bwaddr = bwaddr.wrapping_add(bpl);
            let out = Self::planar_row(data, bpp);
            for byte in 0..bpp {
                let p = self.io.dda.wrapping_add(y << 1).wrapping_add(plane_offset(byte));
                let v = out.get(usize::try_from(byte).unwrap_or(0)).copied().unwrap_or(0);
                self.iram_write(p & 0x7FF, v);
            }
        }
    }

    /// Splits eight packed pixels (`bpp` bits each, pixel 0 in the low
    /// bits) into `bpp` bitplanes with pixel 0 as the high bit.
    fn planar_row(mut data: u64, bpp: u32) -> [u8; 8] {
        let mut out = [0u8; 8];
        for x in 0..8u8 {
            for plane in 0..bpp {
                if let Some(slot) = out.get_mut(usize::try_from(plane).unwrap_or(0)) {
                    *slot |= crate::bits::lo(
                        crate::bits::lo16(u32::try_from(data & 1).unwrap_or(0)).into(),
                    ) << (7u8.wrapping_sub(x));
                }
                data >>= 1;
            }
        }
        out
    }

    /// Type 2 conversion: one eight-pixel row from the register file into
    /// the I-RAM buffer; 16 rows make the two-character buffer.
    pub(crate) fn dma_cc2(&mut self) {
        let dmacb = u32::from(self.io.dmacb);
        let line = u32::from(self.cc2_line);
        let base = usize::try_from((line & 1) << 3).unwrap_or(0);
        let bpp = 2u32 << (2u32.wrapping_sub(dmacb));
        let mut address = self.io.dda & 0x7FF;
        address &= !((1u32 << (7u32.wrapping_sub(dmacb))).wrapping_sub(1));
        address = address.wrapping_add((line & 8).wrapping_mul(bpp));
        address = address.wrapping_add((line & 7).wrapping_mul(2));
        for byte in 0..bpp {
            let mut output = 0u8;
            for bit in 0..8u8 {
                let px = self.io.brf.get(base.wrapping_add(usize::from(bit))).copied().unwrap_or(0);
                output |= ((px >> byte) & 1) << (7u8.wrapping_sub(bit));
            }
            self.iram_write(address.wrapping_add(plane_offset(byte)), output);
        }
        self.cc2_line = crate::bits::lo(line.wrapping_add(1) & 15);
    }
}
