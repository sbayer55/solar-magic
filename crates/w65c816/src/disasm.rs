//! A disassembler for debuggers: one instruction at a time, with the
//! immediate width taken from the current M/X flags.

use crate::addressing::Mode;
use crate::table::OPCODES;

/// Mnemonics by opcode (the standard WDC names).
#[rustfmt::skip]
const MNEMONICS: [&str; 256] = [
    "BRK", "ORA", "COP", "ORA", "TSB", "ORA", "ASL", "ORA", // 00
    "PHP", "ORA", "ASL", "PHD", "TSB", "ORA", "ASL", "ORA", // 08
    "BPL", "ORA", "ORA", "ORA", "TRB", "ORA", "ASL", "ORA", // 10
    "CLC", "ORA", "INC", "TCS", "TRB", "ORA", "ASL", "ORA", // 18
    "JSR", "AND", "JSL", "AND", "BIT", "AND", "ROL", "AND", // 20
    "PLP", "AND", "ROL", "PLD", "BIT", "AND", "ROL", "AND", // 28
    "BMI", "AND", "AND", "AND", "BIT", "AND", "ROL", "AND", // 30
    "SEC", "AND", "DEC", "TSC", "BIT", "AND", "ROL", "AND", // 38
    "RTI", "EOR", "WDM", "EOR", "MVP", "EOR", "LSR", "EOR", // 40
    "PHA", "EOR", "LSR", "PHK", "JMP", "EOR", "LSR", "EOR", // 48
    "BVC", "EOR", "EOR", "EOR", "MVN", "EOR", "LSR", "EOR", // 50
    "CLI", "EOR", "PHY", "TCD", "JMP", "EOR", "LSR", "EOR", // 58
    "RTS", "ADC", "PER", "ADC", "STZ", "ADC", "ROR", "ADC", // 60
    "PLA", "ADC", "ROR", "RTL", "JMP", "ADC", "ROR", "ADC", // 68
    "BVS", "ADC", "ADC", "ADC", "STZ", "ADC", "ROR", "ADC", // 70
    "SEI", "ADC", "PLY", "TDC", "JMP", "ADC", "ROR", "ADC", // 78
    "BRA", "STA", "BRL", "STA", "STY", "STA", "STX", "STA", // 80
    "DEY", "BIT", "TXA", "PHB", "STY", "STA", "STX", "STA", // 88
    "BCC", "STA", "STA", "STA", "STY", "STA", "STX", "STA", // 90
    "TYA", "STA", "TXS", "TXY", "STZ", "STA", "STZ", "STA", // 98
    "LDY", "LDA", "LDX", "LDA", "LDY", "LDA", "LDX", "LDA", // A0
    "TAY", "LDA", "TAX", "PLB", "LDY", "LDA", "LDX", "LDA", // A8
    "BCS", "LDA", "LDA", "LDA", "LDY", "LDA", "LDX", "LDA", // B0
    "CLV", "LDA", "TSX", "TYX", "LDY", "LDA", "LDX", "LDA", // B8
    "CPY", "CMP", "REP", "CMP", "CPY", "CMP", "DEC", "CMP", // C0
    "INY", "CMP", "DEX", "WAI", "CPY", "CMP", "DEC", "CMP", // C8
    "BNE", "CMP", "CMP", "CMP", "PEI", "CMP", "DEC", "CMP", // D0
    "CLD", "CMP", "PHX", "STP", "JMP", "CMP", "DEC", "CMP", // D8
    "CPX", "SBC", "SEP", "SBC", "CPX", "SBC", "INC", "SBC", // E0
    "INX", "SBC", "NOP", "XBA", "CPX", "SBC", "INC", "SBC", // E8
    "BEQ", "SBC", "SBC", "SBC", "PEA", "SBC", "INC", "SBC", // F0
    "SED", "SBC", "PLX", "XCE", "JSR", "SBC", "INC", "SBC", // F8
];

/// Immediates of these opcodes are index-sized (`LDX`/`LDY`/`CPX`/`CPY`).
const INDEX_IMMEDIATES: [u8; 4] = [0xA0, 0xA2, 0xC0, 0xE0];
/// Immediates of these opcodes are always one byte.
const BYTE_IMMEDIATES: [u8; 5] = [0x00, 0x02, 0x42, 0xC2, 0xE2];

/// One decoded instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disassembly {
    /// `MNEMONIC operand`, e.g. `LDA $0E,X`.
    pub text: String,
    /// Bytes the instruction occupies (1-4).
    pub len: u8,
    /// The raw bytes.
    pub bytes: Vec<u8>,
}

/// Operand bytes for `mode`; immediates depend on the flags.
fn operand_len(opcode: u8, mode: Mode, m8: bool, x8: bool) -> u8 {
    match mode {
        Mode::Implied | Mode::Acc => 0,
        Mode::Imm => {
            if BYTE_IMMEDIATES.contains(&opcode) {
                1
            } else if INDEX_IMMEDIATES.contains(&opcode) {
                if x8 { 1 } else { 2 }
            } else if m8 {
                1
            } else {
                2
            }
        }
        Mode::Dp
        | Mode::DpX
        | Mode::DpY
        | Mode::DpInd
        | Mode::DpIndX
        | Mode::DpIndY
        | Mode::DpIndLong
        | Mode::DpIndLongY
        | Mode::StackRel
        | Mode::StackRelIndY
        | Mode::Rel8 => 1,
        Mode::Abs
        | Mode::AbsX
        | Mode::AbsY
        | Mode::AbsInd
        | Mode::AbsIndX
        | Mode::AbsIndLong
        | Mode::Rel16
        | Mode::BlockMove => 2,
        Mode::AbsLong | Mode::AbsLongX => 3,
    }
}

/// Formats the operand.
fn operand(mode: Mode, bytes: &[u8], pc: u32) -> String {
    let b = |i: usize| bytes.get(i).copied().unwrap_or(0);
    let v8 = b(0);
    let v16 = u16::from_le_bytes([b(0), b(1)]);
    let v24 = u32::from_le_bytes([b(0), b(1), b(2), 0]);
    let next = pc.wrapping_add(1).wrapping_add(u32::try_from(bytes.len()).unwrap_or(0));
    match mode {
        Mode::Implied => String::new(),
        Mode::Acc => "A".to_owned(),
        Mode::Imm => match bytes.len() {
            1 => format!("#${v8:02X}"),
            _ => format!("#${v16:04X}"),
        },
        Mode::Dp => format!("${v8:02X}"),
        Mode::DpX => format!("${v8:02X},X"),
        Mode::DpY => format!("${v8:02X},Y"),
        Mode::DpInd => format!("(${v8:02X})"),
        Mode::DpIndX => format!("(${v8:02X},X)"),
        Mode::DpIndY => format!("(${v8:02X}),Y"),
        Mode::DpIndLong => format!("[${v8:02X}]"),
        Mode::DpIndLongY => format!("[${v8:02X}],Y"),
        Mode::Abs => format!("${v16:04X}"),
        Mode::AbsX => format!("${v16:04X},X"),
        Mode::AbsY => format!("${v16:04X},Y"),
        Mode::AbsLong => format!("${v24:06X}"),
        Mode::AbsLongX => format!("${v24:06X},X"),
        Mode::StackRel => format!("${v8:02X},S"),
        Mode::StackRelIndY => format!("(${v8:02X},S),Y"),
        Mode::Rel8 => {
            let target = next.wrapping_add_signed(i32::from(v8.cast_signed())) & 0xFFFF;
            format!("${target:04X}")
        }
        Mode::Rel16 => {
            let target = next.wrapping_add_signed(i32::from(v16.cast_signed())) & 0xFFFF;
            format!("${target:04X}")
        }
        Mode::AbsInd => format!("(${v16:04X})"),
        Mode::AbsIndX => format!("(${v16:04X},X)"),
        Mode::AbsIndLong => format!("[${v16:04X}]"),
        // Operand order in the encoding is destination bank, source bank.
        Mode::BlockMove => format!("${:02X},${:02X}", b(1), b(0)),
    }
}

/// Decodes the instruction at `pc24`, reading bytes through `read`
/// (which sees the current bank for the instruction's operand bytes).
pub fn disassemble(read: &mut dyn FnMut(u32) -> u8, pc24: u32, m8: bool, x8: bool) -> Disassembly {
    let opcode = read(pc24);
    let mode = OPCODES.get(usize::from(opcode)).map_or(Mode::Implied, |entry| entry.1);
    let len = operand_len(opcode, mode, m8, x8);
    let bank = pc24 & 0xFF_0000;
    let pc = pc24 & 0xFFFF;
    let mut bytes = vec![opcode];
    for i in 1..=u32::from(len) {
        bytes.push(read(bank | (pc.wrapping_add(i) & 0xFFFF)));
    }
    let mnemonic = MNEMONICS.get(usize::from(opcode)).copied().unwrap_or("???");
    let operand = operand(mode, bytes.get(1..).unwrap_or(&[]), pc);
    let text =
        if operand.is_empty() { mnemonic.to_owned() } else { format!("{mnemonic} {operand}") };
    Disassembly { text, len: len.wrapping_add(1), bytes }
}

#[cfg(test)]
mod tests {
    use super::disassemble;

    fn dis(bytes: &[u8], m8: bool, x8: bool) -> (String, u8) {
        let mut read =
            |a: u32| bytes.get(usize::try_from(a & 0xFFFF).unwrap_or(0)).copied().unwrap_or(0);
        let d = disassemble(&mut read, 0x00_0000, m8, x8);
        (d.text, d.len)
    }

    #[test]
    fn immediates_follow_the_flags() {
        assert_eq!(dis(&[0xA9, 0x12, 0x34], true, true), ("LDA #$12".to_owned(), 2));
        assert_eq!(dis(&[0xA9, 0x12, 0x34], false, true), ("LDA #$3412".to_owned(), 3));
        assert_eq!(dis(&[0xA2, 0x12, 0x34], false, true), ("LDX #$12".to_owned(), 2));
        assert_eq!(dis(&[0xA2, 0x12, 0x34], true, false), ("LDX #$3412".to_owned(), 3));
        assert_eq!(dis(&[0xC2, 0x30], false, false), ("REP #$30".to_owned(), 2));
    }

    #[test]
    fn addressing_modes_format() {
        assert_eq!(dis(&[0xBF, 0x00, 0x80, 0x05], true, true).0, "LDA $058000,X");
        assert_eq!(dis(&[0xB7, 0x0E], true, true).0, "LDA [$0E],Y");
        assert_eq!(dis(&[0x80, 0xFE], true, true).0, "BRA $0000");
        assert_eq!(dis(&[0x82, 0x10, 0x00], true, true).0, "BRL $0013");
        assert_eq!(dis(&[0x54, 0x7E, 0x05], true, true).0, "MVN $05,$7E");
        assert_eq!(dis(&[0x0A], true, true).0, "ASL A");
        assert_eq!(dis(&[0xFC, 0x34, 0x12], true, true).0, "JSR ($1234,X)");
        assert_eq!(dis(&[0xEA], true, true), ("NOP".to_owned(), 1));
    }
}
