//! General-purpose DMA, started by `$420B`. Runs between CPU cycles,
//! channel 0 first; HDMA may interrupt it and cancels the channel it shares.

use crate::bits::join;
use crate::bus::SystemBus;

impl SystemBus {
    /// Moves one byte between the A bus and B-bus register `b_reg` for
    /// channel `ch`, after charging 8 cycles.
    pub(crate) fn dma_transfer(&mut self, ch: usize, a_addr: u32, b_reg: u8) {
        self.step(8);
        let Some(channel) = self.dma.channels.get(ch).copied() else {
            return;
        };
        let b_addr = 0x2100 | u32::from(b_reg);
        let valid_a = super::a_bus_valid(a_addr);
        // A WRAM address paired with the WRAM port moves nothing.
        if b_reg == 0x80 && super::a_bus_is_wram(a_addr) {
            return;
        }
        if channel.b_to_a() {
            let v = self.read_untimed(b_addr);
            if valid_a {
                self.write_untimed(a_addr, v);
            }
        } else {
            let v = if valid_a { self.read_untimed(a_addr) } else { self.mdr };
            self.write_untimed(b_addr, v);
        }
    }

    /// Runs every channel enabled in `MDMAEN`.
    pub(crate) fn gpdma_run(&mut self) {
        self.in_dma = true;
        self.dma.gp_pending = false;
        // DMA runs on an 8-cycle clock: wait for the next edge, then set up.
        let misalign = crate::bits::lo16_64(self.clock.now & 7);
        if misalign != 0 {
            self.step(u8::try_from(8u16.wrapping_sub(misalign)).unwrap_or(0));
        }
        self.step(8);
        for ch in 0..8 {
            if self.dma.gp_enable & (1 << ch) != 0 {
                self.gpdma_channel(ch);
            }
        }
        self.dma.gp_enable = 0;
        self.in_dma = false;
    }

    fn gpdma_channel(&mut self, ch: usize) {
        self.step(8);
        let mut index = 0usize;
        loop {
            // HDMA on this channel cancels the rest of the transfer.
            if self.dma.gp_enable & (1 << ch) == 0 {
                return;
            }
            let Some(channel) = self.dma.channels.get(ch).copied() else {
                return;
            };
            let a_addr = join(channel.a_bank, channel.a_addr);
            self.dma_transfer(ch, a_addr, channel.b_reg(index));
            index = index.wrapping_add(1);
            let Some(state) = self.dma.channels.get_mut(ch) else {
                return;
            };
            state.step_a();
            state.count = state.count.wrapping_sub(1);
            if state.count == 0 {
                break;
            }
        }
        self.dma.gp_enable &= !(1 << ch);
    }
}
