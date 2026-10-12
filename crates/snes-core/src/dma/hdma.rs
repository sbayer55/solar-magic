//! H-blank DMA: one unit per channel per scanline from a table in ROM/RAM,
//! reloaded at the top of every frame (fullsnes "HDMA Table Formats").

use crate::bits::join;
use crate::bus::SystemBus;

impl SystemBus {
    fn hdma_active(&self, ch: usize) -> bool {
        self.dma.hdma_enable & (1 << ch) != 0
            && self.dma.channels.get(ch).is_some_and(|c| !c.hdma_completed)
    }

    /// Reads one table byte for channel `ch` (8 cycles) and advances the
    /// table pointer.
    fn hdma_table_byte(&mut self, ch: usize) -> u8 {
        self.step(8);
        let Some(channel) = self.dma.channels.get_mut(ch) else {
            return 0;
        };
        let addr = join(channel.a_bank, channel.table_addr);
        channel.table_addr = channel.table_addr.wrapping_add(1);
        let valid = super::a_bus_valid(addr);
        if valid { self.read_untimed(addr) } else { self.mdr }
    }

    /// Loads the next table entry: the line counter, then (indirect mode)
    /// the 16-bit data pointer into `DAS`.
    fn hdma_reload(&mut self, ch: usize) {
        let count = self.hdma_table_byte(ch);
        let indirect = self.dma.channels.get(ch).is_some_and(|c| c.indirect());
        if let Some(channel) = self.dma.channels.get_mut(ch) {
            channel.line_counter = count;
            channel.hdma_completed = count == 0;
            channel.hdma_do_transfer = count != 0;
        }
        if indirect {
            let lo = self.hdma_table_byte(ch);
            let hi = self.hdma_table_byte(ch);
            if let Some(channel) = self.dma.channels.get_mut(ch) {
                channel.count = u16::from_le_bytes([lo, hi]);
            }
        }
    }

    /// Line 0: every enabled channel restarts its table.
    pub(crate) fn hdma_init(&mut self) {
        for channel in &mut self.dma.channels {
            channel.hdma_completed = false;
            channel.hdma_do_transfer = false;
        }
        if self.dma.hdma_enable == 0 {
            return;
        }
        self.step(8);
        for ch in 0..8 {
            if self.dma.hdma_enable & (1 << ch) == 0 {
                continue;
            }
            self.dma.gp_enable &= !(1 << ch);
            if let Some(channel) = self.dma.channels.get_mut(ch) {
                channel.table_addr = channel.a_addr;
                channel.line_counter = 0;
            }
            self.hdma_reload(ch);
        }
    }

    /// Dot 278 of a visible line: transfer, then count down and reload.
    pub(crate) fn hdma_run(&mut self) {
        if !(0..8).any(|ch| self.hdma_active(ch)) {
            return;
        }
        self.step(8);
        for ch in 0..8 {
            if !self.hdma_active(ch) {
                continue;
            }
            self.dma.gp_enable &= !(1 << ch);
            if self.dma.channels.get(ch).is_some_and(|c| c.hdma_do_transfer) {
                self.hdma_transfer_unit(ch);
            }
        }
        for ch in 0..8 {
            if !self.hdma_active(ch) {
                continue;
            }
            let reload = self.dma.channels.get_mut(ch).is_some_and(|channel| {
                channel.line_counter = channel.line_counter.wrapping_sub(1);
                channel.hdma_do_transfer = channel.line_counter & 0x80 != 0;
                channel.line_counter & 0x7F == 0
            });
            if reload {
                self.hdma_reload(ch);
            }
        }
    }

    fn hdma_transfer_unit(&mut self, ch: usize) {
        let Some(channel) = self.dma.channels.get(ch).copied() else {
            return;
        };
        for index in 0..usize::from(channel.unit_len()) {
            let Some(current) = self.dma.channels.get_mut(ch) else {
                return;
            };
            let a_addr = if current.indirect() {
                let addr = join(current.indirect_bank, current.count);
                current.count = current.count.wrapping_add(1);
                addr
            } else {
                let addr = join(current.a_bank, current.table_addr);
                current.table_addr = current.table_addr.wrapping_add(1);
                addr
            };
            self.dma_transfer(ch, a_addr, channel.b_reg(index));
        }
    }
}
