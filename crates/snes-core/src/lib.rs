//! The SNES machine around the `w65c816` core: master clock, system bus,
//! CPU-side registers, DMA/HDMA, the cartridge, and the [`Console`] facade
//! the editor and CLI drive. See `docs/emulation/`.

mod bits;
pub mod bus;
pub mod cart;
pub mod clock;
pub mod console;
pub mod cpu_regs;
pub mod dma;
pub mod input;

pub use bus::{Overlay, SystemBus};
pub use cart::Cartridge;
pub use clock::{Clock, Events, Region};
pub use console::{Console, Hit, RunReport};
pub use input::{Buttons, Input};
pub use snes_ppu::{FRAME_HEIGHT, FRAME_WIDTH, Frame};
