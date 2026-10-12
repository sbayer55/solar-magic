//! Battery-backed memory: LoROM SRAM and SA-1 BW-RAM through one API.

use snes_core::{Cartridge, Console, Region};
use snes_rom::{LoRom, MapperKind, Sa1Rom};

#[test]
fn lorom_sram_round_trips() {
    let cart = Cartridge::from_bytes(&vec![0u8; 0x8000], MapperKind::LoRom(LoRom), 0x800, false);
    let mut console = Console::with_cartridge(cart, Region::Ntsc);
    assert!(!console.battery_dirty());
    console.bus_mut().cart.write(0x70_0000, 0x5A, 0);
    assert!(console.battery_dirty());
    assert_eq!(console.battery().first(), Some(&0x5A));
    console.clear_battery_dirty();
    console.load_battery(&[1, 2, 3]);
    assert_eq!(console.battery().get(..3), Some(&[1, 2, 3][..]));
    assert!(!console.battery_dirty());
    assert_eq!(console.bus_mut().cart.read(0x70_0001, 0), Some(2));
}

#[test]
fn sa1_bwram_is_the_battery() {
    let cart = Cartridge::from_bytes(&vec![0u8; 0x8000], MapperKind::Sa1(Sa1Rom), 0x2000, false);
    let mut console = Console::with_cartridge(cart, Region::Ntsc);
    console.bus_mut().cart.write(0x00_2226, 0x80, 0); // SBWE
    console.bus_mut().cart.write(0x40_0010, 0x77, 0);
    assert!(console.battery_dirty());
    assert_eq!(console.battery().get(0x10), Some(&0x77));
    assert_eq!(console.battery().len(), 0x2000);
    console.load_battery(&[0xAB; 0x2000]);
    assert!(!console.battery_dirty());
    assert_eq!(console.peek(0x40_0010), 0xAB);
}
