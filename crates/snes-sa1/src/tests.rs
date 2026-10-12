//! Unit tests over synthetic ROM images.

use std::sync::Arc;

use super::{Sa1, Side};

fn sa1_with(rom: Vec<u8>, bwram: usize) -> Sa1 {
    Sa1::new(Arc::from(rom), bwram, false)
}

/// Eight 1 MiB blocks, each filled with its index.
fn block_rom() -> Vec<u8> {
    (0..8u8).flat_map(|b| std::iter::repeat_n(b, 0x10_0000)).collect()
}

#[test]
fn super_mmc_maps_lorom_and_hirom_blocks() {
    let mut sa1 = sa1_with(block_rom(), 0);
    let read = |s: &Sa1, addr: u32| s.chip.rom_read(addr, Side::Cpu);
    // Power-on: LoROM halves linear over the first 4 MiB, HiROM banks 0-3.
    assert_eq!(read(&sa1, 0x00_8000), 0);
    assert_eq!(read(&sa1, 0x20_8000), 1);
    assert_eq!(read(&sa1, 0x80_8000), 2);
    assert_eq!(read(&sa1, 0xA0_8000), 3);
    assert_eq!(read(&sa1, 0xC0_0000), 0);
    assert_eq!(read(&sa1, 0xD0_0000), 1);
    assert_eq!(read(&sa1, 0xFF_FFFF), 3);
    // CXB = bank 5 mapped to LoROM too.
    sa1.chip.write_cpu(0x00_2220, 0x85);
    assert_eq!(read(&sa1, 0x00_8000), 5);
    assert_eq!(read(&sa1, 0xC0_0000), 5);
    // DXB = bank 6, HiROM only: the LoROM half stays linear.
    sa1.chip.write_cpu(0x00_2221, 0x06);
    assert_eq!(read(&sa1, 0x20_8000), 1);
    assert_eq!(read(&sa1, 0xD0_0000), 6);
}

#[test]
fn sa1_vectors_come_from_registers() {
    let mut sa1 = sa1_with(vec![0; 0x8000], 0);
    sa1.cpu_write(0x00_2203, 0x34, 0);
    sa1.cpu_write(0x00_2204, 0x12, 0);
    sa1.cpu_write(0x00_2207, 0x78, 0);
    sa1.cpu_write(0x00_2208, 0x56, 0);
    assert_eq!(sa1.chip.rom_read(0x00_FFFC, Side::Sa1), 0x34);
    assert_eq!(sa1.chip.rom_read(0x00_FFFD, Side::Sa1), 0x12);
    assert_eq!(sa1.chip.rom_read(0x00_FFEE, Side::Sa1), 0x78);
    // The S-CPU keeps the ROM vectors unless SCNT says otherwise.
    assert_eq!(sa1.chip.rom_read(0x00_FFEE, Side::Cpu), 0);
    sa1.chip.write_io_sa1(0x220E, 0xCD);
    sa1.chip.write_io_sa1(0x220F, 0xAB);
    sa1.chip.write_io_sa1(0x2209, 0x40);
    assert_eq!(sa1.chip.rom_read(0x00_FFEE, Side::Cpu), 0xCD);
    assert_eq!(sa1.chip.rom_read(0x00_FFEF, Side::Cpu), 0xAB);
}

#[test]
fn bwram_window_and_bitmap_views() {
    let mut sa1 = sa1_with(vec![0; 0x8000], 0x1_0000);
    sa1.chip.write_io_sa1(0x2227, 0x80); // CBWE
    sa1.chip.write_cpu(0x00_2226, 0x80); // SBWE
    // S-CPU block 2 at $6000 is BW-RAM $4000.
    sa1.chip.write_cpu(0x00_2224, 2);
    sa1.chip.write_cpu(0x00_6010, 0x5A);
    assert_eq!(sa1.chip.bwram()[0x4010], 0x5A);
    assert_eq!(sa1.chip.read_cpu(0x40_4010), Some(0x5A));
    // 4 bpp bitmap: two pixels per byte, first pixel in the low nibble.
    sa1.chip.write_sa1(0x60_0000, 0x3);
    sa1.chip.write_sa1(0x60_0001, 0x5);
    assert_eq!(sa1.chip.bwram()[0], 0x53);
    assert_eq!(sa1.chip.read_sa1(0x60_0001), 0x5);
    // 2 bpp bitmap: four pixels per byte.
    sa1.chip.write_io_sa1(0x223F, 0x80);
    for (i, px) in [1u8, 2, 3, 0].into_iter().enumerate() {
        sa1.chip.write_sa1(0x60_0008 + u32::try_from(i).unwrap(), px);
    }
    assert_eq!(sa1.chip.bwram()[2], 0b0011_1001);
    // SA-1 window through BMAP with the bitmap bit: block 0 of $60.
    sa1.chip.write_io_sa1(0x2225, 0x80);
    assert_eq!(sa1.chip.read_sa1(0x00_6008), 1);
}

#[test]
fn bwram_write_protection_needs_both_enables_clear() {
    let mut sa1 = sa1_with(vec![0; 0x8000], 0x2000);
    // BWPA = 1: the first 512 bytes are protected while SWEN and CWEN are off.
    sa1.chip.write_cpu(0x00_2228, 1);
    sa1.chip.write_cpu(0x40_0100, 0x11);
    sa1.chip.write_cpu(0x40_0200, 0x22);
    assert_eq!(sa1.chip.bwram()[0x100], 0);
    assert_eq!(sa1.chip.bwram()[0x200], 0x22);
    sa1.chip.write_cpu(0x00_2226, 0x80);
    sa1.chip.write_cpu(0x40_0100, 0x11);
    assert_eq!(sa1.chip.bwram()[0x100], 0x11);
}

#[test]
fn iram_protection_masks() {
    let mut sa1 = sa1_with(vec![0; 0x8000], 0);
    sa1.chip.write_cpu(0x00_3100, 0xAA);
    assert_eq!(sa1.chip.iram()[0x100], 0);
    sa1.chip.write_cpu(0x00_2229, 0x02);
    sa1.chip.write_cpu(0x00_3100, 0xAA);
    sa1.chip.write_cpu(0x00_3000, 0xBB);
    assert_eq!(sa1.chip.iram()[0x100], 0xAA);
    assert_eq!(sa1.chip.iram()[0x000], 0);
    assert_eq!(sa1.chip.read_cpu(0x80_3100), Some(0xAA));
    assert_eq!(sa1.chip.read_sa1(0x00_0100), 0xAA);
}

#[test]
fn arithmetic_unit() {
    let mut sa1 = sa1_with(vec![0; 0x8000], 0);
    let chip = &mut sa1.chip;
    let result = |c: &mut super::Chip| {
        (0..5u16).fold(0u64, |acc, i| acc | (u64::from(c.read_io_sa1(0x2306 + i)) << (i * 8)))
    };
    // 3 × -2
    chip.write_io_sa1(0x2250, 0);
    chip.write_io_sa1(0x2251, 3);
    chip.write_io_sa1(0x2252, 0);
    chip.write_io_sa1(0x2253, 0xFE);
    chip.write_io_sa1(0x2254, 0xFF);
    assert_eq!(result(chip), 0xFFFF_FFFA);
    // -7 / 2 = -4 remainder 1
    chip.write_io_sa1(0x2250, 1);
    chip.write_io_sa1(0x2251, 0xF9);
    chip.write_io_sa1(0x2252, 0xFF);
    chip.write_io_sa1(0x2253, 2);
    chip.write_io_sa1(0x2254, 0);
    assert_eq!(result(chip), 0x0001_FFFC);
    // Division by zero: zero.
    chip.write_io_sa1(0x2251, 9);
    chip.write_io_sa1(0x2253, 0);
    chip.write_io_sa1(0x2254, 0);
    assert_eq!(result(chip), 0);
    // Cumulative sum: 2×3 + 4×5 = 26.
    chip.write_io_sa1(0x2250, 2);
    for (a, b) in [(2u8, 3u8), (4, 5)] {
        chip.write_io_sa1(0x2251, a);
        chip.write_io_sa1(0x2252, 0);
        chip.write_io_sa1(0x2253, b);
        chip.write_io_sa1(0x2254, 0);
    }
    assert_eq!(result(chip), 26);
    assert_eq!(chip.read_io_sa1(0x230B), 0);
}

#[test]
fn variable_length_bit_reader() {
    let mut rom = vec![0u8; 0x8000];
    rom[..3].copy_from_slice(&[0x34, 0x12, 0xAB]);
    let mut sa1 = sa1_with(rom, 0);
    let chip = &mut sa1.chip;
    // Address $C0:0000 is ROM offset 0 through the HiROM map.
    chip.write_io_sa1(0x2259, 0x00);
    chip.write_io_sa1(0x225A, 0x00);
    chip.write_io_sa1(0x225B, 0xC0);
    assert_eq!(chip.read_io_sa1(0x230C), 0x34);
    assert_eq!(chip.read_io_sa1(0x230D), 0x12);
    // Fixed mode: writing VBD advances by 4 bits.
    chip.write_io_sa1(0x2258, 4);
    assert_eq!(chip.read_io_sa1(0x230C), 0x23);
    assert_eq!(chip.read_io_sa1(0x230D), 0xB1);
    // Auto-increment mode by 8 bits: the high read advances.
    chip.write_io_sa1(0x2258, 0x88);
    assert_eq!(chip.read_io_sa1(0x230D), 0xB1);
    assert_eq!(chip.read_io_sa1(0x230C), 0xB1);
}

#[test]
fn normal_dma_rom_to_iram_raises_the_dma_irq() {
    let rom: Vec<u8> = (0..0x8000u32).map(|i| u8::try_from(i & 0xFF).unwrap()).collect();
    let mut sa1 = sa1_with(rom, 0);
    let chip = &mut sa1.chip;
    chip.write_io_sa1(0x220A, 0x20); // DMA IRQ enable
    chip.write_io_sa1(0x2230, 0x80); // enable, ROM → I-RAM
    chip.write_io_sa1(0x2232, 0x10);
    chip.write_io_sa1(0x2233, 0x00);
    chip.write_io_sa1(0x2234, 0xC0);
    chip.write_io_sa1(0x2238, 16);
    chip.write_io_sa1(0x2239, 0);
    chip.write_io_sa1(0x2235, 0x20);
    chip.write_io_sa1(0x2236, 0x00);
    assert_eq!(&chip.iram()[0x20..0x30], &rom_bytes(0x10, 16));
    assert_eq!(chip.read_io_sa1(0x2301) & 0x20, 0x20);
    assert!(!chip.io.dma_irqcl);
    chip.write_io_sa1(0x220B, 0x20);
    assert_eq!(chip.read_io_sa1(0x2301) & 0x20, 0);
}

fn rom_bytes(start: u8, n: usize) -> Vec<u8> {
    (0..n).map(|i| start.wrapping_add(u8::try_from(i).unwrap())).collect()
}

#[test]
fn character_conversion_type_2_builds_planar_tiles() {
    let mut sa1 = sa1_with(vec![0; 0x8000], 0);
    let chip = &mut sa1.chip;
    chip.write_io_sa1(0x2231, 1); // 4 bpp
    chip.write_io_sa1(0x2230, 0xA0); // enable, type 2
    chip.write_io_sa1(0x2235, 0x00);
    chip.write_io_sa1(0x2236, 0x00);
    for (i, px) in (1u8..=8).enumerate() {
        chip.write_io_sa1(0x2240 + u16::try_from(i).unwrap(), px);
    }
    assert_eq!(chip.iram()[0], 0xAA);
    assert_eq!(chip.iram()[1], 0x66);
    assert_eq!(chip.iram()[16], 0x1E);
    assert_eq!(chip.iram()[17], 0x01);
    // The second row lands two bytes further.
    for i in 8..16u16 {
        chip.write_io_sa1(0x2240 + i, 0x0F);
    }
    assert_eq!(&chip.iram()[2..4], &[0xFF, 0xFF]);
    assert_eq!(&chip.iram()[18..20], &[0xFF, 0xFF]);
}

/// The SA-1 program: multiply 3 × -2, store the 32-bit result at I-RAM
/// `$3000`, raise an IRQ on the S-CPU, spin.
const PROGRAM: [u8; 0x2C] = [
    0x18, 0xFB, // CLC, XCE
    0xE2, 0x20, // SEP #$20
    0xA9, 0xFF, 0x8D, 0x2A, 0x22, // LDA #$FF; STA $222A (CIWP)
    0xC2, 0x30, // REP #$30
    0xA9, 0x03, 0x00, 0x8D, 0x51, 0x22, // LDA #3; STA $2251
    0xA9, 0xFE, 0xFF, 0x8D, 0x53, 0x22, // LDA #-2; STA $2253 (starts)
    0xAD, 0x06, 0x23, 0x8D, 0x00, 0x30, // LDA $2306; STA $3000
    0xAD, 0x08, 0x23, 0x8D, 0x02, 0x30, // LDA $2308; STA $3002
    0xE2, 0x20, 0xA9, 0x80, 0x8D, 0x09, 0x22, // SEP #$20; LDA #$80; STA $2209
    0x80, 0xFE, // BRA *
];

#[test]
fn sa1_cpu_runs_a_program_and_interrupts_the_s_cpu() {
    let mut rom = vec![0u8; 0x8000];
    rom[..PROGRAM.len()].copy_from_slice(&PROGRAM);
    let mut sa1 = sa1_with(rom, 0);
    // Held in reset: nothing runs.
    sa1.run_to(1000);
    assert_eq!(sa1.cpu().steps, 0);
    sa1.cpu_write(0x00_2203, 0x00, 1000);
    sa1.cpu_write(0x00_2204, 0x80, 1000);
    sa1.cpu_write(0x00_2201, 0x80, 1000); // SIE: IRQ from SA-1
    sa1.cpu_write(0x00_2200, 0x00, 1000); // release reset
    assert!(!sa1.irq_pending());
    sa1.run_to(5000);
    assert_eq!(&sa1.chip.iram()[..4], &[0xFA, 0xFF, 0xFF, 0xFF]);
    assert_eq!(sa1.cpu_read(0x00_3000, 5000), Some(0xFA));
    assert!(sa1.irq_pending());
    assert_eq!(sa1.cpu_read(0x00_2300, 5000).map(|v| v & 0x80), Some(0x80));
    sa1.cpu_write(0x00_2202, 0x80, 5000);
    assert!(!sa1.irq_pending());
    assert!(sa1.chip.master() >= 5000);
}

#[test]
fn timer_irq_fires_on_the_programmed_line() {
    let mut rom = vec![0u8; 0x8000];
    rom[0] = 0x80;
    rom[1] = 0xFE; // BRA *
    let mut sa1 = sa1_with(rom, 0);
    let chip = &mut sa1.chip;
    chip.write_io_sa1(0x2210, 0x02); // VEN
    chip.write_io_sa1(0x2214, 3);
    chip.write_io_sa1(0x2215, 0);
    chip.write_io_sa1(0x220A, 0x40); // timer IRQ enable
    sa1.cpu_write(0x00_2203, 0x00, 0);
    sa1.cpu_write(0x00_2204, 0x80, 0);
    sa1.cpu_write(0x00_2200, 0x00, 0);
    sa1.run_to(2 * 1364);
    assert_eq!(sa1.chip.read_io_sa1(0x2301) & 0x40, 0);
    sa1.run_to(4 * 1364);
    assert_eq!(sa1.chip.read_io_sa1(0x2301) & 0x40, 0x40);
    // Latching the counters reads lines and dots.
    let _ = sa1.chip.read_io_sa1(0x2302);
    assert!(sa1.chip.read_io_sa1(0x2304) >= 3);
}
