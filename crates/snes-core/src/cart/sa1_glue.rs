//! The SA-1 behind the [`Coprocessor`] trait.

use snes_sa1::Sa1;

use super::Coprocessor;

/// An SA-1 cartridge's chip.
#[derive(Debug, Clone)]
pub struct Sa1Copro(pub Sa1);

impl Coprocessor for Sa1Copro {
    fn read(&mut self, addr: u32, now: u64) -> Option<u8> {
        self.0.cpu_read(addr, now)
    }

    fn peek(&self, addr: u32) -> Option<u8> {
        self.0.peek(addr)
    }

    fn write(&mut self, addr: u32, v: u8, now: u64) -> bool {
        self.0.cpu_write(addr, v, now)
    }

    fn run_to(&mut self, now: u64) {
        self.0.run_to(now);
    }

    fn irq_pending(&self) -> bool {
        self.0.irq_pending()
    }

    fn reset(&mut self, now: u64) {
        self.0.reset(now);
    }

    fn battery(&self) -> Option<&[u8]> {
        Some(self.0.chip.bwram())
    }

    fn battery_mut(&mut self) -> Option<&mut [u8]> {
        Some(self.0.chip.bwram_mut())
    }

    fn boxed_clone(&self) -> Box<dyn Coprocessor> {
        Box::new(self.clone())
    }
}
