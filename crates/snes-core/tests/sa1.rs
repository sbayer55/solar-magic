//! An SA-1 cartridge in the console: the S-CPU programs the chip, the
//! SA-1 computes, and the result crosses back through I-RAM and the IRQ
//! line.

use snes_core::{Cartridge, Console, Input, Region};
use snes_rom::{MapperKind, Sa1Rom};

/// S-CPU program at `$00:8000`: set `CRV = $8100`, enable the SA-1's IRQ,
/// release reset, wait for `SFR` bit 7, copy I-RAM `$3000` to WRAM `$00`,
/// acknowledge, spin.
const CPU_PROGRAM: [u8; 0x2D] = [
    0x78, 0x18, 0xFB, // SEI, CLC, XCE
    0xE2, 0x20, // SEP #$20
    0x9C, 0x03, 0x22, // STZ $2203
    0xA9, 0x81, 0x8D, 0x04, 0x22, // LDA #$81; STA $2204
    0xA9, 0x80, 0x8D, 0x01, 0x22, // LDA #$80; STA $2201
    0x9C, 0x00, 0x22, // STZ $2200
    0xAD, 0x00, 0x23, 0x10, 0xFB, // wait: LDA $2300; BPL wait
    0xAD, 0x00, 0x30, 0x8D, 0x00, 0x00, // LDA $3000; STA $0000
    0xAD, 0x01, 0x30, 0x8D, 0x01, 0x00, // LDA $3001; STA $0001
    0xA9, 0x80, 0x8D, 0x02, 0x22, // LDA #$80; STA $2202
    0x80, 0xFE, // BRA *
];

/// SA-1 program at `$00:8100`: multiply 3 × -2, store the 32-bit result at
/// I-RAM `$3000`, raise an IRQ on the S-CPU, spin.
const SA1_PROGRAM: [u8; 0x2C] = [
    0x18, 0xFB, 0xE2, 0x20, // CLC, XCE, SEP #$20
    0xA9, 0xFF, 0x8D, 0x2A, 0x22, // LDA #$FF; STA $222A
    0xC2, 0x30, // REP #$30
    0xA9, 0x03, 0x00, 0x8D, 0x51, 0x22, // LDA #3; STA $2251
    0xA9, 0xFE, 0xFF, 0x8D, 0x53, 0x22, // LDA #-2; STA $2253
    0xAD, 0x06, 0x23, 0x8D, 0x00, 0x30, // LDA $2306; STA $3000
    0xAD, 0x08, 0x23, 0x8D, 0x02, 0x30, // LDA $2308; STA $3002
    0xE2, 0x20, 0xA9, 0x80, 0x8D, 0x09, 0x22, // SEP #$20; LDA #$80; STA $2209
    0x80, 0xFE, // BRA *
];

#[allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test image of known size"
)]
fn image() -> Vec<u8> {
    let mut rom = vec![0u8; 0x8000];
    rom[..CPU_PROGRAM.len()].copy_from_slice(&CPU_PROGRAM);
    rom[0x100..0x100 + SA1_PROGRAM.len()].copy_from_slice(&SA1_PROGRAM);
    rom[0x7FFC] = 0x00;
    rom[0x7FFD] = 0x80;
    rom
}

#[test]
fn sa1_result_reaches_wram() {
    let cart = Cartridge::from_bytes(&image(), MapperKind::Sa1(Sa1Rom), 0x2000, false);
    let mut console = Console::with_cartridge(cart, Region::Ntsc);
    let report =
        console.run_while(Input::default(), |c| c.peek(0x7E_0001) == 0 && c.bus().clock.frame < 5);
    assert!(report.hit.is_none(), "{report:?}");
    assert_eq!(console.peek(0x7E_0000), 0xFA);
    assert_eq!(console.peek(0x7E_0001), 0xFF);
    assert_eq!(console.peek(0x00_3002), 0xFF);
    assert!(console.bus().cart.copro.as_ref().is_some_and(|c| c.irq_pending()));
    // The next instructions acknowledge the IRQ.
    for _ in 0..4 {
        let _ = console.step_instruction();
    }
    assert!(!console.bus().cart.copro.as_ref().is_some_and(|c| c.irq_pending()));
    // Save states carry the chip.
    let copy = console.clone();
    assert_eq!(copy.peek(0x00_3000), 0xFA);
}

#[test]
fn sa1_reset_holds_the_chip() {
    let cart = Cartridge::from_bytes(&image(), MapperKind::Sa1(Sa1Rom), 0x2000, false);
    let mut console = Console::with_cartridge(cart, Region::Ntsc);
    console.run_frame(Input::default());
    assert_eq!(console.peek(0x7E_0000), 0xFA);
    console.reset();
    // Registers are back to power-on: the S-CPU program runs again.
    console.run_frame(Input::default());
    assert_eq!(console.peek(0x7E_0001), 0xFF);
}
