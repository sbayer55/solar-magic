//! Runs the `SingleStepTests` SPC700 suite when `SPC700_TESTS_DIR` points at
//! its `v1/*.json` files (<https://github.com/SingleStepTests/spc700>), checking
//! registers, RAM and the exact cycle sequence (reads/writes with address and
//! value, internal cycles by position).

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use spc700::{ApuBus, Psw, Spc700};

/// `[address, value, "read" | "write" | "wait"]`.
type Cycle = (Option<u16>, Option<u8>, String);

#[derive(Deserialize)]
struct Case {
    name: String,
    initial: State,
    r#final: State,
    cycles: Vec<Cycle>,
}

#[derive(Deserialize)]
struct State {
    pc: u16,
    a: u8,
    x: u8,
    y: u8,
    sp: u8,
    psw: u8,
    ram: Vec<(u16, u8)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Event {
    Read(u16, u8),
    Write(u16, u8),
    Idle,
}

#[derive(Default)]
struct RecordingBus {
    mem: HashMap<u16, u8>,
    log: Vec<Event>,
}

impl ApuBus for RecordingBus {
    fn read(&mut self, addr: u16) -> u8 {
        let v = self.mem.get(&addr).copied().unwrap_or(0);
        self.log.push(Event::Read(addr, v));
        v
    }

    fn write(&mut self, addr: u16, value: u8) {
        self.mem.insert(addr, value);
        self.log.push(Event::Write(addr, value));
    }

    fn idle(&mut self) {
        self.log.push(Event::Idle);
    }
}

fn load(state: &State) -> (Spc700, RecordingBus) {
    let mut cpu = Spc700::new();
    cpu.pc = state.pc;
    cpu.a = state.a;
    cpu.x = state.x;
    cpu.y = state.y;
    cpu.sp = state.sp;
    cpu.psw = Psw::from_byte(state.psw);
    let mut bus = RecordingBus::default();
    for &(addr, value) in &state.ram {
        bus.mem.insert(addr, value);
    }
    (cpu, bus)
}

fn expected(cycles: &[Cycle]) -> Vec<(Event, bool)> {
    cycles
        .iter()
        .map(|&(addr, value, ref kind)| {
            let known = value.is_some();
            let (a, v) = (addr.unwrap_or(0), value.unwrap_or(0));
            match kind.as_str() {
                "read" => (Event::Read(a, v), known),
                "write" => (Event::Write(a, v), known),
                _ => (Event::Idle, true),
            }
        })
        .collect()
}

const fn same(got: Event, (want, known): (Event, bool)) -> bool {
    match (got, want) {
        (Event::Idle, Event::Idle) => true,
        (Event::Read(a, v), Event::Read(b, w)) | (Event::Write(a, v), Event::Write(b, w)) => {
            a == b && (!known || v == w)
        }
        _ => false,
    }
}

fn mismatches(cpu: &Spc700, bus: &RecordingBus, case: &Case) -> Vec<String> {
    let want = &case.r#final;
    let mut out = Vec::new();
    let mut check = |name: &str, got: u16, exp: u16| {
        if got != exp {
            out.push(format!("{name}: got {got:#X} want {exp:#X}"));
        }
    };
    check("pc", cpu.pc, want.pc);
    check("a", u16::from(cpu.a), u16::from(want.a));
    check("x", u16::from(cpu.x), u16::from(want.x));
    check("y", u16::from(cpu.y), u16::from(want.y));
    check("sp", u16::from(cpu.sp), u16::from(want.sp));
    check("psw", u16::from(cpu.psw.to_byte()), u16::from(want.psw));
    for &(addr, value) in &want.ram {
        let got = bus.mem.get(&addr).copied().unwrap_or(0);
        if got != value {
            out.push(format!("ram[{addr:#06X}]: got {got:#04X} want {value:#04X}"));
        }
    }
    let exp = expected(&case.cycles);
    if bus.log.len() != exp.len() || !bus.log.iter().zip(&exp).all(|(&g, &w)| same(g, w)) {
        let want_only: Vec<Event> = exp.iter().map(|w| w.0).collect();
        out.push(format!("cycles: got {:X?} want {want_only:X?}", bus.log));
    }
    out
}

#[allow(clippy::expect_used, clippy::arithmetic_side_effects, reason = "test harness")]
fn run_file(path: &Path) -> (usize, usize, Vec<String>) {
    let text = fs::read_to_string(path).expect("readable test file");
    let cases: Vec<Case> = serde_json::from_str(&text).expect("valid JSON");
    let mut failures = 0;
    let mut samples = Vec::new();
    for case in &cases {
        let (mut cpu, mut bus) = load(&case.initial);
        cpu.step(&mut bus);
        let bad = mismatches(&cpu, &bus, case);
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
    let Some(dir) = std::env::var_os("SPC700_TESTS_DIR") else {
        eprintln!("SPC700_TESTS_DIR not set; skipping single-step suite");
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
    let mut total_failures = 0usize;
    for path in &files {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let (cases, failures, samples) = run_file(path);
        if failures > 0 {
            eprintln!("{name}: {failures}/{cases} failed");
            for s in &samples {
                eprintln!("    {s}");
            }
            total_failures += failures;
        }
    }
    eprintln!("files: {}, failures: {total_failures}", files.len());
    assert_eq!(total_failures, 0, "SPC700 single-step tests failed");
}
