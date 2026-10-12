//! Debugger state: what the emulator thread captures for the Debug panel
//! (registers, disassembly, watches, a memory page, the trace) and what
//! the panel asks for.

use std::collections::VecDeque;

use snes_core::{Console, Hit};

/// Instructions kept in the trace.
pub(crate) const TRACE_LEN: usize = 64;
/// Instructions disassembled from the program counter.
const DISASM_LINES: usize = 10;
/// Bytes in the memory page.
pub(crate) const MEMORY_LEN: usize = 256;

/// What the panel wants captured.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DebugConfig {
    /// Addresses whose bytes are shown every frame.
    pub(crate) watches: Vec<u32>,
    /// First address of the memory page.
    pub(crate) memory_base: u32,
    /// Record every executed instruction (slower).
    pub(crate) trace: bool,
}

/// The S-CPU's registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct CpuSnapshot {
    pub(crate) a: u16,
    pub(crate) x: u16,
    pub(crate) y: u16,
    pub(crate) s: u16,
    pub(crate) d: u16,
    pub(crate) db: u8,
    pub(crate) pb: u8,
    pub(crate) pc: u16,
    pub(crate) p: u8,
    pub(crate) e: bool,
    pub(crate) waiting: bool,
    pub(crate) steps: u64,
}

impl CpuSnapshot {
    const fn of(console: &Console) -> Self {
        let cpu = console.cpu();
        Self {
            a: cpu.a,
            x: cpu.x,
            y: cpu.y,
            s: cpu.s,
            d: cpu.d,
            db: cpu.db,
            pb: cpu.pb,
            pc: cpu.pc,
            p: cpu.p.to_byte(),
            e: cpu.e,
            waiting: cpu.waiting,
            steps: cpu.steps,
        }
    }

    /// `nvmxdizc` with set flags in capitals.
    pub(crate) fn flags(self) -> String {
        "NVMXDIZC"
            .chars()
            .enumerate()
            .map(|(i, c)| {
                let bit = 7u8.saturating_sub(u8::try_from(i).unwrap_or(0));
                if self.p & (1 << bit) != 0 { c } else { c.to_ascii_lowercase() }
            })
            .collect()
    }
}

/// One executed or upcoming instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Line {
    pub(crate) pc24: u32,
    pub(crate) text: String,
}

/// A snapshot for the panel.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DebugSnapshot {
    pub(crate) cpu: CpuSnapshot,
    /// Instructions from the program counter on.
    pub(crate) disasm: Vec<Line>,
    pub(crate) watches: Vec<(u32, u8)>,
    pub(crate) memory_base: u32,
    pub(crate) memory: Vec<u8>,
    /// Oldest first.
    pub(crate) trace: Vec<Line>,
    pub(crate) breakpoints: Vec<u32>,
    /// Why the run stopped, if it did.
    pub(crate) stop: Option<String>,
}

/// Recorded program counters with the flag widths at the time.
#[derive(Debug, Clone, Default)]
pub(crate) struct TraceRing {
    items: VecDeque<(u32, bool, bool)>,
}

impl TraceRing {
    /// Records the instruction about to execute.
    pub(crate) fn push(&mut self, console: &Console) {
        if self.items.len() >= TRACE_LEN {
            self.items.pop_front();
        }
        let cpu = console.cpu();
        self.items.push_back((cpu.pc24(), cpu.m8(), cpu.x8()));
    }
}

fn disassemble_at(console: &Console, pc24: u32, m8: bool, x8: bool) -> (Line, u32) {
    let mut read = |a: u32| console.peek(a);
    let d = w65c816::disassemble(&mut read, pc24, m8, x8);
    let bytes: Vec<String> = d.bytes.iter().map(|b| format!("{b:02X}")).collect();
    let text = format!("{:<12} {}", bytes.join(" "), d.text);
    (Line { pc24, text }, u32::from(d.len))
}

impl DebugSnapshot {
    /// Captures everything the panel shows.
    pub(crate) fn capture(
        console: &Console,
        cfg: &DebugConfig,
        trace: &TraceRing,
        hit: Option<Hit>,
    ) -> Self {
        let cpu = CpuSnapshot::of(console);
        let (m8, x8) = (console.cpu().m8(), console.cpu().x8());
        let mut disasm = Vec::with_capacity(DISASM_LINES);
        let mut pc24 = console.cpu().pc24();
        for _ in 0..DISASM_LINES {
            let (line, len) = disassemble_at(console, pc24, m8, x8);
            disasm.push(line);
            let bank = pc24 & 0xFF_0000;
            pc24 = bank | ((pc24 & 0xFFFF).wrapping_add(len) & 0xFFFF);
        }
        let watches = cfg.watches.iter().map(|&a| (a, console.peek(a))).collect();
        let memory = (0..MEMORY_LEN)
            .map(|i| console.peek(cfg.memory_base.wrapping_add(u32::try_from(i).unwrap_or(0))))
            .collect();
        let trace = if cfg.trace {
            trace.items.iter().map(|&(pc, m, x)| disassemble_at(console, pc, m, x).0).collect()
        } else {
            Vec::new()
        };
        let stop = hit.map(|h| match h {
            Hit::Breakpoint(pc) => format!("breakpoint at ${pc:06X}"),
            Hit::Stopped(e) => format!("CPU stopped: {e}"),
        });
        Self {
            cpu,
            disasm,
            watches,
            memory_base: cfg.memory_base,
            memory,
            trace,
            breakpoints: console.breakpoints().to_vec(),
            stop,
        }
    }
}

/// Parses a hex address like `7E0019` or `$7E:0019`.
pub(crate) fn parse_address(text: &str) -> Option<u32> {
    let cleaned: String = text.chars().filter(char::is_ascii_hexdigit).collect();
    if cleaned.is_empty() || cleaned.len() > 6 {
        return None;
    }
    u32::from_str_radix(&cleaned, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_parse_loosely() {
        assert_eq!(parse_address("$7E:0019"), Some(0x7E_0019));
        assert_eq!(parse_address("19"), Some(0x19));
        assert_eq!(parse_address(""), None);
        assert_eq!(parse_address("12345678"), None);
    }

    #[test]
    fn flags_render() {
        let cpu = CpuSnapshot { p: 0b1010_0001, ..CpuSnapshot::default() };
        assert_eq!(cpu.flags(), "NvMxdizC");
    }
}
