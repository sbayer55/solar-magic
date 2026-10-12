//! The 256-entry opcode table: handler plus operand parameter.

use crate::addressing::Mode::{self, Abs, AbsX, AbsY, Dp, DpX, DpY, IndDpX, IndDpY, IndX, IndXInc};
use crate::ops::Alu::{Adc, And, Cmp, Eor, Or, Sbc};
use crate::ops::Cond;
use crate::ops::Reg::{A, Psw, Sp, X, Y};
use crate::ops::Rmw::{Asl, Dec, Inc, Lsr, Rol, Ror};
use crate::ops::{P, alu, bits, branch, misc, mov, word};
use crate::{ApuBus, Spc700};

/// An instruction implementation.
pub(crate) type OpFn = fn(&mut Spc700, &mut dyn ApuBus, P);

/// Handler and parameter for one opcode.
pub(crate) type Entry = (OpFn, P);

const fn a(op: crate::ops::Alu, mode: Mode) -> Entry {
    (alu::alu_a, P::Alu(op, mode))
}
const fn ai(op: crate::ops::Alu) -> Entry {
    (alu::alu_a_imm, P::AluMem(op))
}
const fn axy(op: crate::ops::Alu) -> Entry {
    (alu::alu_ix_iy, P::AluMem(op))
}
const fn add(op: crate::ops::Alu) -> Entry {
    (alu::alu_dp_dp, P::AluMem(op))
}
const fn adi(op: crate::ops::Alu) -> Entry {
    (alu::alu_dp_imm, P::AluMem(op))
}
const fn rm(op: crate::ops::Rmw, mode: Mode) -> Entry {
    (alu::rmw_mem, P::Rmw(op, mode))
}
const fn rr(op: crate::ops::Rmw, r: crate::ops::Reg) -> Entry {
    (alu::rmw_reg, P::RmwReg(op, r))
}
const fn ld(r: crate::ops::Reg, mode: Mode) -> Entry {
    (mov::mov_load, P::Load(r, mode))
}
const fn st(r: crate::ops::Reg, mode: Mode) -> Entry {
    (mov::mov_store, P::Store(r, mode))
}
const fn mi(r: crate::ops::Reg) -> Entry {
    (mov::mov_reg_imm, P::Reg(r))
}
const fn mr(dst: crate::ops::Reg, src: crate::ops::Reg) -> Entry {
    (mov::mov_reg_reg, P::Reg2(dst, src))
}
const fn br(c: Cond) -> Entry {
    (branch::branch_cond, P::Cond(c))
}
const fn tcall(n: u8) -> Entry {
    (branch::tcall, P::Num(n))
}
const fn set1(bit: u8) -> Entry {
    (bits::set1_clr1, P::Num(bit | 8))
}
const fn clr1(bit: u8) -> Entry {
    (bits::set1_clr1, P::Num(bit))
}
const fn bbs(bit: u8) -> Entry {
    (bits::bbs_bbc, P::Num(bit | 8))
}
const fn bbc(bit: u8) -> Entry {
    (bits::bbs_bbc, P::Num(bit))
}
const fn n(f: OpFn) -> Entry {
    (f, P::None)
}
const fn num(f: OpFn, v: u8) -> Entry {
    (f, P::Num(v))
}
const fn cmpr(r: crate::ops::Reg, mode: Mode) -> Entry {
    (alu::cmp_reg_mem, P::Load(r, mode))
}

/// Indexed by opcode byte.
#[rustfmt::skip]
pub(crate) const OPCODES: [Entry; 256] = [
    // 00-0F
    n(misc::nop), tcall(0), set1(0), bbs(0), a(Or, Dp), a(Or, Abs), a(Or, IndX), a(Or, IndDpX),
    ai(Or), add(Or), num(bits::carry_logic, 0), rm(Asl, Dp), rm(Asl, Abs), (misc::push, P::Reg(Psw)), num(bits::tset1_tclr1, 1), n(branch::brk),
    // 10-1F
    br(Cond::Pl), tcall(1), clr1(0), bbc(0), a(Or, DpX), a(Or, AbsX), a(Or, AbsY), a(Or, IndDpY),
    adi(Or), axy(Or), num(word::incw_decw, 1), rm(Asl, DpX), rr(Asl, A), rr(Dec, X), cmpr(X, Abs), n(branch::jmp_iabsx),
    // 20-2F
    num(misc::flag2, 0), tcall(2), set1(1), bbs(1), a(And, Dp), a(And, Abs), a(And, IndX), a(And, IndDpX),
    ai(And), add(And), num(bits::carry_logic, 1), rm(Rol, Dp), rm(Rol, Abs), (misc::push, P::Reg(A)), (branch::cbne, P::Load(A, Dp)), br(Cond::Always),
    // 30-3F
    br(Cond::Mi), tcall(3), clr1(1), bbc(1), a(And, DpX), a(And, AbsX), a(And, AbsY), a(And, IndDpY),
    adi(And), axy(And), num(word::incw_decw, 0), rm(Rol, DpX), rr(Rol, A), rr(Inc, X), cmpr(X, Dp), n(branch::call),
    // 40-4F
    num(misc::flag2, 1), tcall(4), set1(2), bbs(2), a(Eor, Dp), a(Eor, Abs), a(Eor, IndX), a(Eor, IndDpX),
    ai(Eor), add(Eor), num(bits::carry_logic, 2), rm(Lsr, Dp), rm(Lsr, Abs), (misc::push, P::Reg(X)), num(bits::tset1_tclr1, 0), n(branch::pcall),
    // 50-5F
    br(Cond::Vc), tcall(5), clr1(2), bbc(2), a(Eor, DpX), a(Eor, AbsX), a(Eor, AbsY), a(Eor, IndDpY),
    adi(Eor), axy(Eor), n(word::cmpw), rm(Lsr, DpX), rr(Lsr, A), mr(X, A), cmpr(Y, Abs), n(branch::jmp_abs),
    // 60-6F
    num(misc::flag2, 2), tcall(6), set1(3), bbs(3), a(Cmp, Dp), a(Cmp, Abs), a(Cmp, IndX), a(Cmp, IndDpX),
    ai(Cmp), add(Cmp), num(bits::carry_logic, 3), rm(Ror, Dp), rm(Ror, Abs), (misc::push, P::Reg(Y)), n(branch::dbnz_dp), n(branch::ret),
    // 70-7F
    br(Cond::Vs), tcall(7), clr1(3), bbc(3), a(Cmp, DpX), a(Cmp, AbsX), a(Cmp, AbsY), a(Cmp, IndDpY),
    adi(Cmp), axy(Cmp), num(word::addw_subw, 0), rm(Ror, DpX), rr(Ror, A), mr(A, X), cmpr(Y, Dp), n(branch::reti),
    // 80-8F
    num(misc::flag2, 3), tcall(8), set1(4), bbs(4), a(Adc, Dp), a(Adc, Abs), a(Adc, IndX), a(Adc, IndDpX),
    ai(Adc), add(Adc), num(bits::carry_logic, 4), rm(Dec, Dp), rm(Dec, Abs), mi(Y), (misc::pop, P::Reg(Psw)), n(mov::mov_dp_imm),
    // 90-9F
    br(Cond::Cc), tcall(9), clr1(4), bbc(4), a(Adc, DpX), a(Adc, AbsX), a(Adc, AbsY), a(Adc, IndDpY),
    adi(Adc), axy(Adc), num(word::addw_subw, 1), rm(Dec, DpX), rr(Dec, A), mr(X, Sp), n(word::div), n(misc::xcn),
    // A0-AF
    num(misc::flag3, 0), tcall(10), set1(5), bbs(5), a(Sbc, Dp), a(Sbc, Abs), a(Sbc, IndX), a(Sbc, IndDpX),
    ai(Sbc), add(Sbc), n(bits::mov1_c_mem), rm(Inc, Dp), rm(Inc, Abs), (alu::cmp_reg_imm, P::Reg(Y)), (misc::pop, P::Reg(A)), st(A, IndXInc),
    // B0-BF
    br(Cond::Cs), tcall(11), clr1(5), bbc(5), a(Sbc, DpX), a(Sbc, AbsX), a(Sbc, AbsY), a(Sbc, IndDpY),
    adi(Sbc), axy(Sbc), n(word::movw_ya_dp), rm(Inc, DpX), rr(Inc, A), mr(Sp, X), n(misc::das), ld(A, IndXInc),
    // C0-CF
    num(misc::flag3, 1), tcall(12), set1(6), bbs(6), st(A, Dp), st(A, Abs), st(A, IndX), st(A, IndDpX),
    (alu::cmp_reg_imm, P::Reg(X)), st(X, Abs), n(bits::mov1_mem_c), st(Y, Dp), st(Y, Abs), mi(X), (misc::pop, P::Reg(X)), n(word::mul),
    // D0-DF
    br(Cond::Ne), tcall(13), clr1(6), bbc(6), st(A, DpX), st(A, AbsX), st(A, AbsY), st(A, IndDpY),
    st(X, Dp), st(X, DpY), n(word::movw_dp_ya), st(Y, DpX), rr(Dec, Y), mr(A, Y), (branch::cbne, P::Load(A, DpX)), n(misc::daa),
    // E0-EF
    num(misc::flag2, 4), tcall(14), set1(7), bbs(7), ld(A, Dp), ld(A, Abs), ld(A, IndX), ld(A, IndDpX),
    mi(A), ld(X, Abs), n(bits::not1), ld(Y, Dp), ld(Y, Abs), num(misc::flag3, 2), (misc::pop, P::Reg(Y)), n(misc::halt),
    // F0-FF
    br(Cond::Eq), tcall(15), clr1(7), bbc(7), ld(A, DpX), ld(A, AbsX), ld(A, AbsY), ld(A, IndDpY),
    ld(X, Dp), ld(X, DpY), n(mov::mov_dp_dp), ld(Y, DpX), rr(Inc, Y), mr(Y, A), n(branch::dbnz_y), n(misc::halt),
];
