//! Drives the modelled boot ROM through the documented upload protocol from
//! the S-CPU side.

use snes_apu::{Apu, Ipl};

/// Runs the APU for about `n` more SPC cycles.
#[allow(clippy::arithmetic_side_effects, reason = "test harness")]
fn spin(apu: &mut Apu, n: u64) {
    let master = (apu.cycles() + n) * 2_684_659 / 128_000 + 1;
    apu.run_to(master);
}

/// Waits until `$2140` reads `want` (bounded).
#[allow(clippy::panic, reason = "test harness")]
fn wait_port0(apu: &mut Apu, want: u8) {
    for _ in 0..10_000 {
        if apu.cpu_read_port(0) == want {
            return;
        }
        spin(apu, 16);
    }
    panic!("port 0 never became {want:#04X} (is {:#04X})", apu.cpu_read_port(0));
}

#[test]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "test harness")]
fn upload_and_jump() {
    let mut apu = Apu::new(Ipl::Hle);
    spin(&mut apu, 5_000);
    assert_eq!((apu.cpu_read_port(0), apu.cpu_read_port(1)), (0xAA, 0xBB));

    // A tiny program at $0300: `MOV $F4,#$42 ; BRA -5`.
    let program = [0x8F, 0x42, 0xF4, 0x2F, 0xFB];
    apu.cpu_write_port(2, 0x00);
    apu.cpu_write_port(3, 0x03);
    apu.cpu_write_port(1, 0x01); // nonzero command: transfer
    apu.cpu_write_port(0, 0xCC);
    wait_port0(&mut apu, 0xCC);
    for (i, &b) in program.iter().enumerate() {
        let index = u8::try_from(i).unwrap();
        apu.cpu_write_port(1, b);
        apu.cpu_write_port(0, index);
        wait_port0(&mut apu, index);
    }
    assert_eq!(&apu.smp.ram()[0x300..0x305], &program);

    // End of block: new address = entry point, command 0 = jump, index + 2.
    apu.cpu_write_port(2, 0x00);
    apu.cpu_write_port(3, 0x03);
    apu.cpu_write_port(1, 0x00);
    let kick = u8::try_from(program.len() + 1).unwrap();
    apu.cpu_write_port(0, kick);
    wait_port0(&mut apu, kick);
    spin(&mut apu, 200);
    // The uploaded program runs and writes $42 to port 0.
    assert_eq!(apu.cpu_read_port(0), 0x42);
    assert!(apu.cpu.pc >= 0x0300 && apu.cpu.pc < 0x0305, "pc {:#06X}", apu.cpu.pc);
}

#[test]
fn clock_ratio_is_exact() {
    // 21 477 272 master cycles (one second) are 1 024 000 SPC cycles.
    assert_eq!(Apu::target_cycles(21_477_272), 1_024_000);
    assert_eq!(Apu::target_cycles(0), 0);
}
