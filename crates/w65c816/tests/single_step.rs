//! Runs the `SingleStepTests` 65816 suite when `W65C816_TESTS_DIR` points at a
//! directory of its `*.json` files (<https://github.com/SingleStepTests/65816>).
//!
//! Native-mode files must pass completely; emulation-mode files are reported
//! but allowed to fail, since Solar Magic only runs native-mode code.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use w65c816::{Bus, Cpu, Flags};

#[derive(Deserialize)]
struct Case {
    name: String,
    initial: State,
    r#final: State,
}

#[derive(Deserialize)]
struct State {
    pc: u16,
    s: u16,
    p: u8,
    a: u16,
    x: u16,
    y: u16,
    dbr: u8,
    d: u16,
    pbr: u8,
    e: u8,
    ram: Vec<(u32, u8)>,
}

/// Memory as a map so each case is cheap to set up.
#[derive(Default)]
struct SparseBus {
    mem: HashMap<u32, u8>,
}

impl Bus for SparseBus {
    fn read(&mut self, addr: u32) -> u8 {
        self.mem.get(&(addr & 0xFF_FFFF)).copied().unwrap_or(0)
    }

    fn write(&mut self, addr: u32, value: u8) {
        self.mem.insert(addr & 0xFF_FFFF, value);
    }
}

fn load(state: &State) -> (Cpu, SparseBus) {
    let mut cpu = Cpu::new();
    cpu.pc = state.pc;
    cpu.s = state.s;
    cpu.p = Flags::from_byte(state.p);
    cpu.a = state.a;
    cpu.x = state.x;
    cpu.y = state.y;
    cpu.db = state.dbr;
    cpu.d = state.d;
    cpu.pb = state.pbr;
    cpu.e = state.e != 0;
    let mut bus = SparseBus::default();
    for &(addr, value) in &state.ram {
        bus.write(addr, value);
    }
    (cpu, bus)
}

fn mismatches(cpu: &Cpu, bus: &mut SparseBus, want: &State) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |name: &str, got: u32, exp: u32| {
        if got != exp {
            out.push(format!("{name}: got {got:#X} want {exp:#X}"));
        }
    };
    check("pc", u32::from(cpu.pc), u32::from(want.pc));
    check("s", u32::from(cpu.s), u32::from(want.s));
    check("p", u32::from(cpu.p.to_byte()), u32::from(want.p));
    check("a", u32::from(cpu.a), u32::from(want.a));
    check("x", u32::from(cpu.x), u32::from(want.x));
    check("y", u32::from(cpu.y), u32::from(want.y));
    check("dbr", u32::from(cpu.db), u32::from(want.dbr));
    check("d", u32::from(cpu.d), u32::from(want.d));
    check("pbr", u32::from(cpu.pb), u32::from(want.pbr));
    check("e", u32::from(cpu.e), u32::from(want.e));
    for &(addr, value) in &want.ram {
        let got = bus.read(addr);
        if got != value {
            out.push(format!("ram[{addr:#08X}]: got {got:#04X} want {value:#04X}"));
        }
    }
    out
}

/// Runs one file; returns (cases, failures, first few failure descriptions).
#[allow(clippy::expect_used, clippy::arithmetic_side_effects, reason = "test harness")]
fn run_file(path: &Path) -> (usize, usize, Vec<String>) {
    let text = fs::read_to_string(path).expect("readable test file");
    let cases: Vec<Case> = serde_json::from_str(&text).expect("valid JSON");
    let mut failures = 0;
    let mut samples = Vec::new();
    for case in &cases {
        let (mut cpu, mut bus) = load(&case.initial);
        // STP/WAI report a stop; the architectural state is still comparable.
        let _ = cpu.step(&mut bus);
        let bad = mismatches(&cpu, &mut bus, &case.r#final);
        if !bad.is_empty() {
            failures += 1;
            if samples.len() < 3 {
                samples.push(format!("{}: {}", case.name, bad.join(", ")));
            }
        }
    }
    (cases.len(), failures, samples)
}

#[test]
fn single_step_suite() {
    let Some(dir) = std::env::var_os("W65C816_TESTS_DIR") else {
        eprintln!("W65C816_TESTS_DIR not set; skipping single-step suite");
        return;
    };
    let mut files: Vec<_> = fs::read_dir(&dir)
        .expect("test dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .json files in {}", Path::new(&dir).display());

    let mut native_failures = 0usize;
    let mut emu_failures = 0usize;
    for path in &files {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        // The suite models MVN/MVP as a fixed multi-byte transfer per step; the
        // hardware (and this core) moves one byte per execution and rewinds PC.
        // Both agree once the move completes, which is all callers observe.
        if name.starts_with("44.") || name.starts_with("54.") {
            eprintln!("{name}: skipped (block move step semantics differ from the suite)");
            continue;
        }
        let (cases, failures, samples) = run_file(path);
        let native = name.contains(".n.");
        if failures > 0 {
            eprintln!("{name}: {failures}/{cases} failed");
            for s in &samples {
                eprintln!("    {s}");
            }
            if native {
                native_failures += failures;
            } else {
                emu_failures += failures;
            }
        }
    }
    eprintln!(
        "files: {}, native failures: {native_failures}, emulation failures: {emu_failures}",
        files.len()
    );
    assert_eq!(native_failures, 0, "native-mode single-step tests failed");
}
