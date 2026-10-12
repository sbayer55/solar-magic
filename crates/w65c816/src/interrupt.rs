//! Hardware interrupts and reset.
//!
//! The CPU has no interrupt sources of its own: the machine around it decides
//! when the NMI edge or the IRQ level is asserted and calls [`Cpu::nmi`] or
//! [`Cpu::irq`] between instructions. Entry costs 7 cycles in emulation mode
//! and 8 in native mode (dummy opcode fetch, internal cycle, three or four
//! pushes, two vector reads).

use crate::bus::Bus;
use crate::cpu::Cpu;
use crate::flags::Flags;

/// Native-mode NMI vector.
const VEC_NMI_NATIVE: u32 = 0xFFEA;
/// Native-mode IRQ vector.
const VEC_IRQ_NATIVE: u32 = 0xFFEE;
/// Emulation-mode NMI vector.
const VEC_NMI_EMU: u32 = 0xFFFA;
/// Emulation-mode IRQ vector (shared with BRK).
const VEC_IRQ_EMU: u32 = 0xFFFE;
/// Reset vector (emulation mode only).
const VEC_RESET: u32 = 0xFFFC;

impl Cpu {
    /// Takes the non-maskable interrupt. Wakes a waiting processor; a stopped
    /// one stays stopped.
    pub fn nmi(&mut self, bus: &mut dyn Bus) {
        if self.stopped {
            return;
        }
        self.waiting = false;
        self.interrupt(bus, VEC_NMI_NATIVE, VEC_NMI_EMU);
    }

    /// Offers a level-triggered IRQ. Wakes a waiting processor even when
    /// interrupts are disabled (the instruction after `WAI` then runs), and
    /// takes the vector only when the I flag is clear. Returns whether the
    /// vector was taken.
    pub fn irq(&mut self, bus: &mut dyn Bus) -> bool {
        if self.stopped {
            return false;
        }
        self.waiting = false;
        if self.p.i {
            return false;
        }
        self.interrupt(bus, VEC_IRQ_NATIVE, VEC_IRQ_EMU);
        true
    }

    /// Hardware reset: emulation mode, 8-bit registers, interrupts disabled,
    /// stack page 1, `PC` from the reset vector. Clears `STP`/`WAI`.
    pub fn reset(&mut self, bus: &mut dyn Bus) {
        self.e = true;
        self.p = Flags { m: true, x: true, i: true, d: false, ..self.p };
        self.apply_flag_widths();
        self.s = 0x0100 | (self.s & 0xFF);
        self.d = 0;
        self.db = 0;
        self.pb = 0;
        self.waiting = false;
        self.stopped = false;
        self.pc = bus.read_u16(VEC_RESET);
    }

    fn interrupt(&mut self, bus: &mut dyn Bus, native_vec: u32, emu_vec: u32) {
        // Dummy opcode fetch, then an internal cycle.
        bus.read(self.pc24());
        bus.idle();
        if self.e {
            self.push16(bus, self.pc);
            // Hardware interrupts push B clear and bit 5 set.
            self.push8(bus, (self.p.to_byte() | 0x20) & !0x10);
        } else {
            let pb = self.pb;
            self.push8(bus, pb);
            self.push16(bus, self.pc);
            self.push8(bus, self.p.to_byte());
        }
        self.p.i = true;
        self.p.d = false;
        self.pb = 0;
        let vec = if self.e { emu_vec } else { native_vec };
        self.pc = bus.read_u16(vec);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FlatBus;

    #[test]
    fn nmi_native_pushes_four_bytes_and_vectors() {
        let mut bus = FlatBus::new();
        bus.load(VEC_NMI_NATIVE, &[0x34, 0x12]);
        let mut cpu = Cpu::new();
        cpu.pb = 0x05;
        cpu.pc = 0x8003;
        cpu.s = 0x01FF;
        cpu.p.i = false;
        cpu.nmi(&mut bus);
        assert_eq!((cpu.pb, cpu.pc), (0, 0x1234));
        assert_eq!(cpu.s, 0x01FB);
        assert!(cpu.p.i);
        assert_eq!(bus.read(0x01FF), 0x05);
        assert_eq!(bus.read(0x01FE), 0x80);
        assert_eq!(bus.read(0x01FD), 0x03);
    }

    #[test]
    fn irq_respects_i_flag_but_wakes_wai() {
        let mut bus = FlatBus::new();
        let mut cpu = Cpu::new();
        cpu.waiting = true;
        assert!(!cpu.irq(&mut bus));
        assert!(!cpu.waiting);
        cpu.p.i = false;
        assert!(cpu.irq(&mut bus));
    }

    #[test]
    fn reset_enters_emulation_mode() {
        let mut bus = FlatBus::new();
        bus.load(VEC_RESET, &[0x00, 0x80]);
        let mut cpu = Cpu::new();
        cpu.stopped = true;
        cpu.reset(&mut bus);
        assert!(cpu.e && !cpu.stopped);
        assert_eq!(cpu.pc, 0x8000);
        assert_eq!(cpu.s & 0xFF00, 0x0100);
    }
}
