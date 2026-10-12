//! Table-driven DMA/HDMA checks on a synthetic cartridge.

use snes_core::{Cartridge, Region, SystemBus};
use snes_rom::Rom;
use w65c816::Bus;

/// A 64 KiB LoROM image carrying SMW's header.
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test harness"
)]
fn synthetic_rom() -> Rom {
    const HEADER: [u8; 32] = [
        b'S', b'U', b'P', b'E', b'R', b' ', b'M', b'A', b'R', b'I', b'O', b'W', b'O', b'R', b'L',
        b'D', b' ', b' ', b' ', b' ', b' ', 0x20, 0x02, 0x06, 0x01, 0x01, 0x01, 0x00, 0x25, 0x5F,
        0xDA, 0xA0,
    ];
    let mut data: Vec<u8> = (0..0x1_0000u32).map(|i| u8::try_from(i & 0xFF).unwrap()).collect();
    data[0x7FC0..0x7FE0].copy_from_slice(&HEADER);
    Rom::from_bytes(data).expect("valid synthetic ROM")
}

fn bus() -> SystemBus {
    SystemBus::new(Cartridge::new(&synthetic_rom()), Region::Ntsc)
}

#[allow(clippy::arithmetic_side_effects, reason = "test harness")]
fn program(bus: &mut SystemBus, ch: u32, regs: &[(u32, u8)]) {
    for &(reg, v) in regs {
        bus.write_untimed(0x4300 + (ch << 4) + reg, v);
    }
}

#[test]
fn gpdma_wram_to_vram_mode_1() {
    let mut bus = bus();
    for i in 0..8u32 {
        bus.wram.write(0x1000 + i, 0x10 + u8::try_from(i).unwrap());
    }
    bus.write_untimed(0x2100, 0x80); // forced blank: VRAM writable
    bus.write_untimed(0x2115, 0x80);
    bus.write_untimed(0x2116, 0x00);
    bus.write_untimed(0x2117, 0x20);
    program(&mut bus, 0, &[(0, 0x01), (1, 0x18), (2, 0x00), (3, 0x10), (4, 0x7E), (5, 8), (6, 0)]);
    let before = bus.clock.now;
    bus.write_untimed(0x420B, 0x01);
    bus.idle(); // the next CPU cycle starts the transfer
    assert_eq!(bus.ppu.vram.word(0x2000), 0x1110);
    assert_eq!(bus.ppu.vram.word(0x2001), 0x1312);
    assert_eq!(bus.ppu.vram.word(0x2003), 0x1716);
    assert_eq!(bus.dma.gp_enable, 0);
    assert_eq!(bus.dma.channels[0].count, 0);
    assert_eq!(bus.dma.channels[0].a_addr, 0x1008);
    // 8 setup + 8 channel + 8 per byte, plus the idle itself.
    assert!(bus.clock.now - before >= 8 + 8 + 64);
}

#[test]
fn gpdma_fixed_source_fills_cgram() {
    let mut bus = bus();
    bus.wram.write(0x0200, 0x7C);
    bus.write_untimed(0x2121, 0);
    // Mode 2 (xx, xx) with a fixed A address: 512 bytes of 0x7C into CGRAM.
    program(
        &mut bus,
        1,
        &[(0, 0x0A), (1, 0x22), (2, 0x00), (3, 0x02), (4, 0x7E), (5, 0x00), (6, 0x02)],
    );
    bus.write_untimed(0x420B, 0x02);
    bus.idle();
    assert_eq!(bus.ppu.cgram.color(0), 0x7C7C);
    assert_eq!(bus.ppu.cgram.color(255), 0x7C7C);
    assert_eq!(bus.dma.channels[1].a_addr, 0x0200);
}

#[test]
fn gpdma_wram_to_wram_port_is_void() {
    let mut bus = bus();
    bus.wram.write(0x0300, 0xAA);
    bus.write_untimed(0x2181, 0x00);
    bus.write_untimed(0x2182, 0x40);
    bus.write_untimed(0x2183, 0x00);
    program(&mut bus, 0, &[(0, 0x00), (1, 0x80), (2, 0x00), (3, 0x03), (4, 0x7E), (5, 1), (6, 0)]);
    bus.write_untimed(0x420B, 0x01);
    bus.idle();
    assert_eq!(bus.wram.read(0x4000), 0);
}

#[test]
fn hdma_direct_table_changes_brightness_per_line() {
    let mut bus = bus();
    // Table at $7E:0500: 3 lines of $0F, then 2 lines of $05, then end.
    for (i, b) in [0x03u8, 0x0F, 0x02, 0x05, 0x00].into_iter().enumerate() {
        bus.wram.write(0x0500 + u32::try_from(i).unwrap(), b);
    }
    program(&mut bus, 0, &[(0, 0x00), (1, 0x00), (2, 0x00), (3, 0x05), (4, 0x7E)]);
    bus.write_untimed(0x420C, 0x01);
    let run_to_line = |b: &mut SystemBus, line: u16| {
        while !(b.clock.v == line && b.clock.h_cycles() >= 1200) {
            b.step(8);
        }
    };
    run_to_line(&mut bus, 0);
    assert_eq!(bus.ppu.regs.brightness, 0x0F);
    assert!(bus.dma.channels[0].line_counter == 0x02);
    run_to_line(&mut bus, 2);
    assert_eq!(bus.ppu.regs.brightness, 0x0F);
    run_to_line(&mut bus, 3);
    assert_eq!(bus.ppu.regs.brightness, 0x05);
    run_to_line(&mut bus, 5);
    assert!(bus.dma.channels[0].hdma_completed);
    assert_eq!(bus.ppu.regs.brightness, 0x05);
}

#[test]
fn hdma_indirect_repeat_mode() {
    let mut bus = bus();
    // Indirect table: repeat 2 lines from pointer $0600 (two 1-byte units).
    bus.wram.write(0x0500, 0x82);
    bus.wram.write(0x0501, 0x00);
    bus.wram.write(0x0502, 0x06);
    bus.wram.write(0x0503, 0x00);
    bus.wram.write(0x0600, 0x07);
    bus.wram.write(0x0601, 0x09);
    program(&mut bus, 2, &[(0, 0x40), (1, 0x00), (2, 0x00), (3, 0x05), (4, 0x7E), (7, 0x7E)]);
    bus.write_untimed(0x420C, 0x04);
    let run_to_line = |b: &mut SystemBus, line: u16| {
        while !(b.clock.v == line && b.clock.h_cycles() >= 1200) {
            b.step(8);
        }
    };
    run_to_line(&mut bus, 0);
    assert_eq!(bus.ppu.regs.brightness, 0x07);
    run_to_line(&mut bus, 1);
    assert_eq!(bus.ppu.regs.brightness, 0x09);
    run_to_line(&mut bus, 2);
    assert!(bus.dma.channels[2].hdma_completed);
}
