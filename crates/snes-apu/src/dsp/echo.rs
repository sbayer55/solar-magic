//! The echo ring buffer in ARAM and its 8-tap FIR filter.

use super::{clamp16, wrap16};

/// Echo registers for one sample.
pub(crate) struct EchoRegs {
    pub(crate) esa: u8,
    pub(crate) edl: u8,
    pub(crate) efb: i32,
    pub(crate) fir: [i32; 8],
    pub(crate) write_enabled: bool,
}

/// Echo state.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Echo {
    /// Last eight echo-buffer reads per channel.
    hist: [[i32; 8]; 2],
    hist_pos: usize,
    /// Byte offset into the ring buffer.
    offset: u16,
    /// Ring buffer length in bytes, sampled when the offset wraps.
    length: u16,
}

fn read16(ram: &[u8], addr: u16) -> i32 {
    let lo = ram.get(usize::from(addr)).copied().unwrap_or(0);
    let hi = ram.get(usize::from(addr.wrapping_add(1))).copied().unwrap_or(0);
    i32::from(i16::from_le_bytes([lo, hi]))
}

fn write16(ram: &mut [u8], addr: u16, v: i32) {
    let [lo, hi] = i16::try_from(clamp16(v)).unwrap_or(0).to_le_bytes();
    if let Some(slot) = ram.get_mut(usize::from(addr)) {
        *slot = lo;
    }
    if let Some(slot) = ram.get_mut(usize::from(addr.wrapping_add(1))) {
        *slot = hi;
    }
}

impl Echo {
    /// Reads the oldest entry, filters, writes the new entry; returns the
    /// FIR output per channel (before `EVOL`).
    pub(crate) fn run(&mut self, ram: &mut [u8], regs: &EchoRegs, echo_in: [i32; 2]) -> [i32; 2] {
        let base = (u16::from(regs.esa) << 8).wrapping_add(self.offset);
        let mut fir_out = [0i32; 2];
        for ch in 0..2 {
            let addr = base.wrapping_add(u16::try_from(ch).unwrap_or(0).wrapping_mul(2));
            let sample = read16(ram, addr) >> 1;
            if let Some(slot) = self.hist.get_mut(ch).and_then(|h| h.get_mut(self.hist_pos)) {
                *slot = sample;
            }
            let hist = self.hist.get(ch).copied().unwrap_or([0; 8]);
            let tap = |i: usize| {
                hist.get(self.hist_pos.wrapping_add(i).wrapping_add(1).checked_rem(8).unwrap_or(0))
                    .copied()
                    .unwrap_or(0)
            };
            let mut sum = 0i32;
            for i in 0..7 {
                sum = sum.wrapping_add(
                    (tap(i).wrapping_mul(regs.fir.get(i).copied().unwrap_or(0))) >> 6,
                );
            }
            sum = wrap16(sum);
            sum =
                sum.wrapping_add((tap(7).wrapping_mul(regs.fir.get(7).copied().unwrap_or(0))) >> 6);
            let out = clamp16(sum) & !1;
            if let Some(slot) = fir_out.get_mut(ch) {
                *slot = out;
            }
            let feedback = echo_in
                .get(ch)
                .copied()
                .unwrap_or(0)
                .wrapping_add((out.wrapping_mul(regs.efb)) >> 7);
            if regs.write_enabled {
                write16(ram, addr, clamp16(feedback) & !1);
            }
        }
        self.hist_pos = self.hist_pos.wrapping_add(1).checked_rem(8).unwrap_or(0);
        self.offset = self.offset.wrapping_add(4);
        if self.offset >= self.length {
            self.offset = 0;
            self.length = u16::from(regs.edl & 0x0F) << 11;
        }
        fir_out
    }
}
