//! The 256-entry opcode dispatch table.

use crate::addressing::Mode;
use crate::bus::Bus;
use crate::cpu::{Cpu, CpuError};
use crate::ops::{alu, branch, load_store, misc, stack};

/// An instruction implementation.
pub(crate) type OpFn = fn(&mut Cpu, &mut dyn Bus, Mode) -> Result<(), CpuError>;

/// Implementation and addressing mode for one opcode.
pub(crate) type Entry = (OpFn, Mode);

/// Indexed by opcode byte.
#[rustfmt::skip]
pub(crate) const OPCODES: [Entry; 256] = [
    (branch::brk, Mode::Imm), // 00 BRK
    (alu::ora, Mode::DpIndX), // 01 ORA
    (branch::cop, Mode::Imm), // 02 COP
    (alu::ora, Mode::StackRel), // 03 ORA
    (alu::tsb, Mode::Dp), // 04 TSB
    (alu::ora, Mode::Dp), // 05 ORA
    (alu::asl, Mode::Dp), // 06 ASL
    (alu::ora, Mode::DpIndLong), // 07 ORA
    (stack::php, Mode::Implied), // 08 PHP
    (alu::ora, Mode::Imm), // 09 ORA
    (alu::asl, Mode::Acc), // 0A ASL
    (stack::phd, Mode::Implied), // 0B PHD
    (alu::tsb, Mode::Abs), // 0C TSB
    (alu::ora, Mode::Abs), // 0D ORA
    (alu::asl, Mode::Abs), // 0E ASL
    (alu::ora, Mode::AbsLong), // 0F ORA
    (branch::bpl, Mode::Rel8), // 10 BPL
    (alu::ora, Mode::DpIndY), // 11 ORA
    (alu::ora, Mode::DpInd), // 12 ORA
    (alu::ora, Mode::StackRelIndY), // 13 ORA
    (alu::trb, Mode::Dp), // 14 TRB
    (alu::ora, Mode::DpX), // 15 ORA
    (alu::asl, Mode::DpX), // 16 ASL
    (alu::ora, Mode::DpIndLongY), // 17 ORA
    (misc::clc, Mode::Implied), // 18 CLC
    (alu::ora, Mode::AbsY), // 19 ORA
    (alu::inc, Mode::Acc), // 1A INC
    (misc::tcs, Mode::Implied), // 1B TCS
    (alu::trb, Mode::Abs), // 1C TRB
    (alu::ora, Mode::AbsX), // 1D ORA
    (alu::asl, Mode::AbsX), // 1E ASL
    (alu::ora, Mode::AbsLongX), // 1F ORA
    (branch::jsr, Mode::Abs), // 20 JSR
    (alu::and, Mode::DpIndX), // 21 AND
    (branch::jsl, Mode::AbsLong), // 22 JSL
    (alu::and, Mode::StackRel), // 23 AND
    (alu::bit, Mode::Dp), // 24 BIT
    (alu::and, Mode::Dp), // 25 AND
    (alu::rol, Mode::Dp), // 26 ROL
    (alu::and, Mode::DpIndLong), // 27 AND
    (stack::plp, Mode::Implied), // 28 PLP
    (alu::and, Mode::Imm), // 29 AND
    (alu::rol, Mode::Acc), // 2A ROL
    (stack::pld, Mode::Implied), // 2B PLD
    (alu::bit, Mode::Abs), // 2C BIT
    (alu::and, Mode::Abs), // 2D AND
    (alu::rol, Mode::Abs), // 2E ROL
    (alu::and, Mode::AbsLong), // 2F AND
    (branch::bmi, Mode::Rel8), // 30 BMI
    (alu::and, Mode::DpIndY), // 31 AND
    (alu::and, Mode::DpInd), // 32 AND
    (alu::and, Mode::StackRelIndY), // 33 AND
    (alu::bit, Mode::DpX), // 34 BIT
    (alu::and, Mode::DpX), // 35 AND
    (alu::rol, Mode::DpX), // 36 ROL
    (alu::and, Mode::DpIndLongY), // 37 AND
    (misc::sec, Mode::Implied), // 38 SEC
    (alu::and, Mode::AbsY), // 39 AND
    (alu::dec, Mode::Acc), // 3A DEC
    (misc::tsc, Mode::Implied), // 3B TSC
    (alu::bit, Mode::AbsX), // 3C BIT
    (alu::and, Mode::AbsX), // 3D AND
    (alu::rol, Mode::AbsX), // 3E ROL
    (alu::and, Mode::AbsLongX), // 3F AND
    (branch::rti, Mode::Implied), // 40 RTI
    (alu::eor, Mode::DpIndX), // 41 EOR
    (misc::wdm, Mode::Imm), // 42 WDM
    (alu::eor, Mode::StackRel), // 43 EOR
    (misc::mvp, Mode::BlockMove), // 44 MVP
    (alu::eor, Mode::Dp), // 45 EOR
    (alu::lsr, Mode::Dp), // 46 LSR
    (alu::eor, Mode::DpIndLong), // 47 EOR
    (stack::pha, Mode::Implied), // 48 PHA
    (alu::eor, Mode::Imm), // 49 EOR
    (alu::lsr, Mode::Acc), // 4A LSR
    (stack::phk, Mode::Implied), // 4B PHK
    (branch::jmp, Mode::Abs), // 4C JMP
    (alu::eor, Mode::Abs), // 4D EOR
    (alu::lsr, Mode::Abs), // 4E LSR
    (alu::eor, Mode::AbsLong), // 4F EOR
    (branch::bvc, Mode::Rel8), // 50 BVC
    (alu::eor, Mode::DpIndY), // 51 EOR
    (alu::eor, Mode::DpInd), // 52 EOR
    (alu::eor, Mode::StackRelIndY), // 53 EOR
    (misc::mvn, Mode::BlockMove), // 54 MVN
    (alu::eor, Mode::DpX), // 55 EOR
    (alu::lsr, Mode::DpX), // 56 LSR
    (alu::eor, Mode::DpIndLongY), // 57 EOR
    (misc::cli, Mode::Implied), // 58 CLI
    (alu::eor, Mode::AbsY), // 59 EOR
    (stack::phy, Mode::Implied), // 5A PHY
    (misc::tcd, Mode::Implied), // 5B TCD
    (branch::jmp, Mode::AbsLong), // 5C JMP
    (alu::eor, Mode::AbsX), // 5D EOR
    (alu::lsr, Mode::AbsX), // 5E LSR
    (alu::eor, Mode::AbsLongX), // 5F EOR
    (branch::rts, Mode::Implied), // 60 RTS
    (alu::adc, Mode::DpIndX), // 61 ADC
    (stack::per, Mode::Rel16), // 62 PER
    (alu::adc, Mode::StackRel), // 63 ADC
    (load_store::stz, Mode::Dp), // 64 STZ
    (alu::adc, Mode::Dp), // 65 ADC
    (alu::ror, Mode::Dp), // 66 ROR
    (alu::adc, Mode::DpIndLong), // 67 ADC
    (stack::pla, Mode::Implied), // 68 PLA
    (alu::adc, Mode::Imm), // 69 ADC
    (alu::ror, Mode::Acc), // 6A ROR
    (branch::rtl, Mode::Implied), // 6B RTL
    (branch::jmp, Mode::AbsInd), // 6C JMP
    (alu::adc, Mode::Abs), // 6D ADC
    (alu::ror, Mode::Abs), // 6E ROR
    (alu::adc, Mode::AbsLong), // 6F ADC
    (branch::bvs, Mode::Rel8), // 70 BVS
    (alu::adc, Mode::DpIndY), // 71 ADC
    (alu::adc, Mode::DpInd), // 72 ADC
    (alu::adc, Mode::StackRelIndY), // 73 ADC
    (load_store::stz, Mode::DpX), // 74 STZ
    (alu::adc, Mode::DpX), // 75 ADC
    (alu::ror, Mode::DpX), // 76 ROR
    (alu::adc, Mode::DpIndLongY), // 77 ADC
    (misc::sei, Mode::Implied), // 78 SEI
    (alu::adc, Mode::AbsY), // 79 ADC
    (stack::ply, Mode::Implied), // 7A PLY
    (misc::tdc, Mode::Implied), // 7B TDC
    (branch::jmp, Mode::AbsIndX), // 7C JMP
    (alu::adc, Mode::AbsX), // 7D ADC
    (alu::ror, Mode::AbsX), // 7E ROR
    (alu::adc, Mode::AbsLongX), // 7F ADC
    (branch::bra, Mode::Rel8), // 80 BRA
    (load_store::sta, Mode::DpIndX), // 81 STA
    (branch::brl, Mode::Rel16), // 82 BRL
    (load_store::sta, Mode::StackRel), // 83 STA
    (load_store::sty, Mode::Dp), // 84 STY
    (load_store::sta, Mode::Dp), // 85 STA
    (load_store::stx, Mode::Dp), // 86 STX
    (load_store::sta, Mode::DpIndLong), // 87 STA
    (misc::dey, Mode::Implied), // 88 DEY
    (alu::bit, Mode::Imm), // 89 BIT
    (misc::txa, Mode::Implied), // 8A TXA
    (stack::phb, Mode::Implied), // 8B PHB
    (load_store::sty, Mode::Abs), // 8C STY
    (load_store::sta, Mode::Abs), // 8D STA
    (load_store::stx, Mode::Abs), // 8E STX
    (load_store::sta, Mode::AbsLong), // 8F STA
    (branch::bcc, Mode::Rel8), // 90 BCC
    (load_store::sta, Mode::DpIndY), // 91 STA
    (load_store::sta, Mode::DpInd), // 92 STA
    (load_store::sta, Mode::StackRelIndY), // 93 STA
    (load_store::sty, Mode::DpX), // 94 STY
    (load_store::sta, Mode::DpX), // 95 STA
    (load_store::stx, Mode::DpY), // 96 STX
    (load_store::sta, Mode::DpIndLongY), // 97 STA
    (misc::tya, Mode::Implied), // 98 TYA
    (load_store::sta, Mode::AbsY), // 99 STA
    (misc::txs, Mode::Implied), // 9A TXS
    (misc::txy, Mode::Implied), // 9B TXY
    (load_store::stz, Mode::Abs), // 9C STZ
    (load_store::sta, Mode::AbsX), // 9D STA
    (load_store::stz, Mode::AbsX), // 9E STZ
    (load_store::sta, Mode::AbsLongX), // 9F STA
    (load_store::ldy, Mode::Imm), // A0 LDY
    (load_store::lda, Mode::DpIndX), // A1 LDA
    (load_store::ldx, Mode::Imm), // A2 LDX
    (load_store::lda, Mode::StackRel), // A3 LDA
    (load_store::ldy, Mode::Dp), // A4 LDY
    (load_store::lda, Mode::Dp), // A5 LDA
    (load_store::ldx, Mode::Dp), // A6 LDX
    (load_store::lda, Mode::DpIndLong), // A7 LDA
    (misc::tay, Mode::Implied), // A8 TAY
    (load_store::lda, Mode::Imm), // A9 LDA
    (misc::tax, Mode::Implied), // AA TAX
    (stack::plb, Mode::Implied), // AB PLB
    (load_store::ldy, Mode::Abs), // AC LDY
    (load_store::lda, Mode::Abs), // AD LDA
    (load_store::ldx, Mode::Abs), // AE LDX
    (load_store::lda, Mode::AbsLong), // AF LDA
    (branch::bcs, Mode::Rel8), // B0 BCS
    (load_store::lda, Mode::DpIndY), // B1 LDA
    (load_store::lda, Mode::DpInd), // B2 LDA
    (load_store::lda, Mode::StackRelIndY), // B3 LDA
    (load_store::ldy, Mode::DpX), // B4 LDY
    (load_store::lda, Mode::DpX), // B5 LDA
    (load_store::ldx, Mode::DpY), // B6 LDX
    (load_store::lda, Mode::DpIndLongY), // B7 LDA
    (misc::clv, Mode::Implied), // B8 CLV
    (load_store::lda, Mode::AbsY), // B9 LDA
    (misc::tsx, Mode::Implied), // BA TSX
    (misc::tyx, Mode::Implied), // BB TYX
    (load_store::ldy, Mode::AbsX), // BC LDY
    (load_store::lda, Mode::AbsX), // BD LDA
    (load_store::ldx, Mode::AbsY), // BE LDX
    (load_store::lda, Mode::AbsLongX), // BF LDA
    (alu::cpy, Mode::Imm), // C0 CPY
    (alu::cmp, Mode::DpIndX), // C1 CMP
    (misc::rep, Mode::Imm), // C2 REP
    (alu::cmp, Mode::StackRel), // C3 CMP
    (alu::cpy, Mode::Dp), // C4 CPY
    (alu::cmp, Mode::Dp), // C5 CMP
    (alu::dec, Mode::Dp), // C6 DEC
    (alu::cmp, Mode::DpIndLong), // C7 CMP
    (misc::iny, Mode::Implied), // C8 INY
    (alu::cmp, Mode::Imm), // C9 CMP
    (misc::dex, Mode::Implied), // CA DEX
    (misc::wai, Mode::Implied), // CB WAI
    (alu::cpy, Mode::Abs), // CC CPY
    (alu::cmp, Mode::Abs), // CD CMP
    (alu::dec, Mode::Abs), // CE DEC
    (alu::cmp, Mode::AbsLong), // CF CMP
    (branch::bne, Mode::Rel8), // D0 BNE
    (alu::cmp, Mode::DpIndY), // D1 CMP
    (alu::cmp, Mode::DpInd), // D2 CMP
    (alu::cmp, Mode::StackRelIndY), // D3 CMP
    (stack::pei, Mode::DpInd), // D4 PEI
    (alu::cmp, Mode::DpX), // D5 CMP
    (alu::dec, Mode::DpX), // D6 DEC
    (alu::cmp, Mode::DpIndLongY), // D7 CMP
    (misc::cld, Mode::Implied), // D8 CLD
    (alu::cmp, Mode::AbsY), // D9 CMP
    (stack::phx, Mode::Implied), // DA PHX
    (misc::stp, Mode::Implied), // DB STP
    (branch::jmp, Mode::AbsIndLong), // DC JMP
    (alu::cmp, Mode::AbsX), // DD CMP
    (alu::dec, Mode::AbsX), // DE DEC
    (alu::cmp, Mode::AbsLongX), // DF CMP
    (alu::cpx, Mode::Imm), // E0 CPX
    (alu::sbc, Mode::DpIndX), // E1 SBC
    (misc::sep, Mode::Imm), // E2 SEP
    (alu::sbc, Mode::StackRel), // E3 SBC
    (alu::cpx, Mode::Dp), // E4 CPX
    (alu::sbc, Mode::Dp), // E5 SBC
    (alu::inc, Mode::Dp), // E6 INC
    (alu::sbc, Mode::DpIndLong), // E7 SBC
    (misc::inx, Mode::Implied), // E8 INX
    (alu::sbc, Mode::Imm), // E9 SBC
    (misc::nop, Mode::Implied), // EA NOP
    (misc::xba, Mode::Implied), // EB XBA
    (alu::cpx, Mode::Abs), // EC CPX
    (alu::sbc, Mode::Abs), // ED SBC
    (alu::inc, Mode::Abs), // EE INC
    (alu::sbc, Mode::AbsLong), // EF SBC
    (branch::beq, Mode::Rel8), // F0 BEQ
    (alu::sbc, Mode::DpIndY), // F1 SBC
    (alu::sbc, Mode::DpInd), // F2 SBC
    (alu::sbc, Mode::StackRelIndY), // F3 SBC
    (stack::pea, Mode::Abs), // F4 PEA
    (alu::sbc, Mode::DpX), // F5 SBC
    (alu::inc, Mode::DpX), // F6 INC
    (alu::sbc, Mode::DpIndLongY), // F7 SBC
    (misc::sed, Mode::Implied), // F8 SED
    (alu::sbc, Mode::AbsY), // F9 SBC
    (stack::plx, Mode::Implied), // FA PLX
    (misc::xce, Mode::Implied), // FB XCE
    (branch::jsr, Mode::AbsIndX), // FC JSR
    (alu::sbc, Mode::AbsX), // FD SBC
    (alu::inc, Mode::AbsX), // FE INC
    (alu::sbc, Mode::AbsLongX), // FF SBC
];

#[cfg(test)]
mod tests {
    use super::OPCODES;
    use crate::addressing::Mode;

    #[test]
    fn spot_checks() {
        assert_eq!(OPCODES[0xA9].1, Mode::Imm);
        assert_eq!(OPCODES[0x8D].1, Mode::Abs);
        assert_eq!(OPCODES[0x7C].1, Mode::AbsIndX);
        assert_eq!(OPCODES[0x54].1, Mode::BlockMove);
    }
}
