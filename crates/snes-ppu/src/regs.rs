//! The `$2100-$2133` register file: decoded state plus a write dispatch
//! table with one small function per register.

use crate::bits::{signed8, signed13, signed16, w};
use crate::{Position, Ppu};

/// One background layer's registers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BgRegs {
    /// Tilemap base, word address (`BGnSC` bits 2-7 `<< 10`).
    pub screen_addr: u16,
    /// `BGnSC` bits 0-1: 0 = 32×32, 1 = 64×32, 2 = 32×64, 3 = 64×64 tiles.
    pub screen_size: u8,
    /// Character data base, word address (`BGnNBA` nibble `<< 12`).
    pub tile_addr: u16,
    /// Horizontal scroll (10 bits used).
    pub hofs: u16,
    /// Vertical scroll (10 bits used).
    pub vofs: u16,
    /// 16×16 tiles (`BGMODE` bits 4-7).
    pub tile_16: bool,
    /// Mosaic enabled (`MOSAIC` bits 0-3).
    pub mosaic: bool,
}

/// Mode 7 registers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mode7Regs {
    /// `M7SEL` bit 0: flip horizontally.
    pub flip_h: bool,
    /// `M7SEL` bit 1: flip vertically.
    pub flip_v: bool,
    /// `M7SEL` bits 6-7: 0/1 wrap, 2 transparent outside, 3 tile 0 outside.
    pub repeat: u8,
    /// Matrix A.
    pub a: i16,
    /// Matrix B.
    pub b: i16,
    /// Matrix C.
    pub c: i16,
    /// Matrix D.
    pub d: i16,
    /// Center X (13 bits, signed).
    pub x: i16,
    /// Center Y (13 bits, signed).
    pub y: i16,
    /// Scroll X (13 bits, signed).
    pub hofs: i16,
    /// Scroll Y (13 bits, signed).
    pub vofs: i16,
}

/// Everything written through `$2100-$2133`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regs {
    /// `INIDISP` bit 7.
    pub forced_blank: bool,
    /// `INIDISP` bits 0-3.
    pub brightness: u8,
    /// `OBSEL` bits 0-2 `<< 13`: sprite character base, word address.
    pub obj_tile_addr: u16,
    /// `OBSEL` bits 3-4: gap between the two sprite name tables (`+1 << 12` words).
    pub obj_name_select: u8,
    /// `OBSEL` bits 5-7: sprite size pair.
    pub obj_size: u8,
    /// `BGMODE` bits 0-2.
    pub bg_mode: u8,
    /// `BGMODE` bit 3.
    pub bg3_priority: bool,
    /// `MOSAIC` bits 4-7 (size − 1).
    pub mosaic_size: u8,
    /// BG1-BG4.
    pub bg: [BgRegs; 4],
    /// Mode 7.
    pub m7: Mode7Regs,
    /// `$2123-$2125` window mask settings, raw.
    pub window_sel: [u8; 3],
    /// `$2126-$2129` window 1 left/right, window 2 left/right.
    pub window_pos: [u8; 4],
    /// `$212A` BG window logic, raw.
    pub window_bg_logic: u8,
    /// `$212B` OBJ/color window logic, raw.
    pub window_obj_logic: u8,
    /// `TM` main screen layers (bits 0-4 = BG1-4, OBJ).
    pub main_screen: u8,
    /// `TS` sub screen layers.
    pub sub_screen: u8,
    /// `TMW` main screen window masking.
    pub main_window: u8,
    /// `TSW` sub screen window masking.
    pub sub_window: u8,
    /// `CGWSEL`, raw.
    pub cgwsel: u8,
    /// `CGADSUB`, raw.
    pub cgadsub: u8,
    /// `COLDATA` fixed color as BGR555.
    pub fixed_color: u16,
    /// `SETINI` bit 0.
    pub interlace: bool,
    /// `SETINI` bit 1.
    pub obj_interlace: bool,
    /// `SETINI` bit 2: 239-line display.
    pub overscan: bool,
    /// `SETINI` bit 3.
    pub pseudo_hires: bool,
    /// `SETINI` bit 6.
    pub extbg: bool,
    bgofs_latch: u8,
    m7_latch: u8,
}

impl Default for Regs {
    fn default() -> Self {
        Self::new()
    }
}

impl Regs {
    /// Power-on state: forced blank, everything else zero.
    #[must_use]
    pub fn new() -> Self {
        Self {
            forced_blank: true,
            brightness: 0,
            obj_tile_addr: 0,
            obj_name_select: 0,
            obj_size: 0,
            bg_mode: 0,
            bg3_priority: false,
            mosaic_size: 0,
            bg: [BgRegs::default(); 4],
            m7: Mode7Regs::default(),
            window_sel: [0; 3],
            window_pos: [0; 4],
            window_bg_logic: 0,
            window_obj_logic: 0,
            main_screen: 0,
            sub_screen: 0,
            main_window: 0,
            sub_window: 0,
            cgwsel: 0,
            cgadsub: 0,
            fixed_color: 0,
            interlace: false,
            obj_interlace: false,
            overscan: false,
            pseudo_hires: false,
            extbg: false,
            bgofs_latch: 0,
            m7_latch: 0,
        }
    }

    fn with_bg(&mut self, n: usize, f: impl FnOnce(&mut BgRegs)) {
        if let Some(bg) = self.bg.get_mut(n) {
            f(bg);
        }
    }

    /// A horizontal scroll write: the shared write-twice latch supplies the
    /// low byte, except its low three bits, which come from the old value.
    fn write_hofs(&mut self, n: usize, v: u8) {
        let latch = self.bgofs_latch;
        self.with_bg(n, |bg| {
            bg.hofs = (w(v) << 8) | w(latch & !7) | ((bg.hofs >> 8) & 7);
        });
        self.bgofs_latch = v;
    }

    fn write_vofs(&mut self, n: usize, v: u8) {
        let latch = self.bgofs_latch;
        self.with_bg(n, |bg| bg.vofs = (w(v) << 8) | w(latch));
        self.bgofs_latch = v;
    }

    /// A mode 7 register write: the high byte arrives with this write, the
    /// low byte came through the mode 7 latch.
    const fn m7_word(&mut self, v: u8) -> u16 {
        let word = (w(v) << 8) | w(self.m7_latch);
        self.m7_latch = v;
        word
    }
}

/// A register write handler.
type WriteFn = fn(&mut Ppu, u8, Position);

const fn inidisp(ppu: &mut Ppu, v: u8, pos: Position) {
    // Toggling forced blank at the first vblank line reloads the OAM address.
    if ppu.regs.forced_blank && pos.v == pos.vdisp {
        ppu.oam.reload_address();
    }
    ppu.regs.forced_blank = v & 0x80 != 0;
    ppu.regs.brightness = v & 0x0F;
}

const fn obsel(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.obj_tile_addr = w(v & 7) << 13;
    ppu.regs.obj_name_select = (v >> 3) & 3;
    ppu.regs.obj_size = v >> 5;
}

const fn oamaddl(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.oam.set_address_low(v);
}

const fn oamaddh(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.oam.set_address_high(v);
}

fn oamdata(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.oam.write(v);
}

fn bgmode(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.bg_mode = v & 7;
    ppu.regs.bg3_priority = v & 8 != 0;
    for (n, bg) in ppu.regs.bg.iter_mut().enumerate() {
        bg.tile_16 = (v >> 4) & (1 << n) != 0;
    }
}

fn mosaic(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.mosaic_size = v >> 4;
    for (n, bg) in ppu.regs.bg.iter_mut().enumerate() {
        bg.mosaic = v & (1 << n) != 0;
    }
}

fn bgsc(ppu: &mut Ppu, n: usize, v: u8) {
    ppu.regs.with_bg(n, |bg| {
        bg.screen_addr = w(v & 0x7C) << 8;
        bg.screen_size = v & 3;
    });
}

fn bg1sc(ppu: &mut Ppu, v: u8, _: Position) {
    bgsc(ppu, 0, v);
}
fn bg2sc(ppu: &mut Ppu, v: u8, _: Position) {
    bgsc(ppu, 1, v);
}
fn bg3sc(ppu: &mut Ppu, v: u8, _: Position) {
    bgsc(ppu, 2, v);
}
fn bg4sc(ppu: &mut Ppu, v: u8, _: Position) {
    bgsc(ppu, 3, v);
}

fn bg12nba(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.with_bg(0, |bg| bg.tile_addr = w(v & 7) << 12);
    ppu.regs.with_bg(1, |bg| bg.tile_addr = w((v >> 4) & 7) << 12);
}

fn bg34nba(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.with_bg(2, |bg| bg.tile_addr = w(v & 7) << 12);
    ppu.regs.with_bg(3, |bg| bg.tile_addr = w((v >> 4) & 7) << 12);
}

fn bg1hofs(ppu: &mut Ppu, v: u8, _: Position) {
    let word = ppu.regs.m7_word(v);
    ppu.regs.m7.hofs = signed13(word);
    ppu.regs.write_hofs(0, v);
}

fn bg1vofs(ppu: &mut Ppu, v: u8, _: Position) {
    let word = ppu.regs.m7_word(v);
    ppu.regs.m7.vofs = signed13(word);
    ppu.regs.write_vofs(0, v);
}

fn bg2hofs(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.write_hofs(1, v);
}
fn bg2vofs(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.write_vofs(1, v);
}
fn bg3hofs(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.write_hofs(2, v);
}
fn bg3vofs(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.write_vofs(2, v);
}
fn bg4hofs(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.write_hofs(3, v);
}
fn bg4vofs(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.write_vofs(3, v);
}

fn vmain(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.vram.set_main(v);
}

fn vmaddl(ppu: &mut Ppu, v: u8, pos: Position) {
    let readable = ppu.memory_accessible(pos);
    ppu.vram.set_address_low(v, readable);
}

fn vmaddh(ppu: &mut Ppu, v: u8, pos: Position) {
    let readable = ppu.memory_accessible(pos);
    ppu.vram.set_address_high(v, readable);
}

fn vmdatal(ppu: &mut Ppu, v: u8, pos: Position) {
    let writable = ppu.memory_accessible(pos);
    ppu.vram.write_low(v, writable);
}

fn vmdatah(ppu: &mut Ppu, v: u8, pos: Position) {
    let writable = ppu.memory_accessible(pos);
    ppu.vram.write_high(v, writable);
}

const fn m7sel(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.m7.flip_h = v & 1 != 0;
    ppu.regs.m7.flip_v = v & 2 != 0;
    ppu.regs.m7.repeat = v >> 6;
}

const fn m7a(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.m7.a = signed16(ppu.regs.m7_word(v));
}
const fn m7b(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.m7.b = signed16(ppu.regs.m7_word(v));
}
const fn m7c(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.m7.c = signed16(ppu.regs.m7_word(v));
}
const fn m7d(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.m7.d = signed16(ppu.regs.m7_word(v));
}
const fn m7x(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.m7.x = signed13(ppu.regs.m7_word(v));
}
const fn m7y(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.m7.y = signed13(ppu.regs.m7_word(v));
}

const fn cgadd(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.cgram.set_address(v);
}

fn cgdata(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.cgram.write(v);
}

fn window_sel(ppu: &mut Ppu, n: usize, v: u8) {
    if let Some(slot) = ppu.regs.window_sel.get_mut(n) {
        *slot = v;
    }
}

fn w12sel(ppu: &mut Ppu, v: u8, _: Position) {
    window_sel(ppu, 0, v);
}
fn w34sel(ppu: &mut Ppu, v: u8, _: Position) {
    window_sel(ppu, 1, v);
}
fn wobjsel(ppu: &mut Ppu, v: u8, _: Position) {
    window_sel(ppu, 2, v);
}

fn window_pos(ppu: &mut Ppu, n: usize, v: u8) {
    if let Some(slot) = ppu.regs.window_pos.get_mut(n) {
        *slot = v;
    }
}

fn wh0(ppu: &mut Ppu, v: u8, _: Position) {
    window_pos(ppu, 0, v);
}
fn wh1(ppu: &mut Ppu, v: u8, _: Position) {
    window_pos(ppu, 1, v);
}
fn wh2(ppu: &mut Ppu, v: u8, _: Position) {
    window_pos(ppu, 2, v);
}
fn wh3(ppu: &mut Ppu, v: u8, _: Position) {
    window_pos(ppu, 3, v);
}

const fn wbglog(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.window_bg_logic = v;
}
const fn wobjlog(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.window_obj_logic = v;
}
const fn tm(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.main_screen = v & 0x1F;
}
const fn ts(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.sub_screen = v & 0x1F;
}
const fn tmw(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.main_window = v & 0x1F;
}
const fn tsw(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.sub_window = v & 0x1F;
}
const fn cgwsel(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.cgwsel = v;
}
const fn cgadsub(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.cgadsub = v;
}

/// `COLDATA`: bits 5-7 pick the channels to set to the 5-bit value.
const fn coldata(ppu: &mut Ppu, v: u8, _: Position) {
    let value = w(v & 0x1F);
    let mut color = ppu.regs.fixed_color;
    if v & 0x20 != 0 {
        color = (color & !0x001F) | value;
    }
    if v & 0x40 != 0 {
        color = (color & !0x03E0) | (value << 5);
    }
    if v & 0x80 != 0 {
        color = (color & !0x7C00) | (value << 10);
    }
    ppu.regs.fixed_color = color;
}

const fn setini(ppu: &mut Ppu, v: u8, _: Position) {
    ppu.regs.interlace = v & 0x01 != 0;
    ppu.regs.obj_interlace = v & 0x02 != 0;
    ppu.regs.overscan = v & 0x04 != 0;
    ppu.regs.pseudo_hires = v & 0x08 != 0;
    ppu.regs.extbg = v & 0x40 != 0;
}

/// Write handlers for `$2100-$2133`, indexed by the low register byte.
#[rustfmt::skip]
pub(crate) const WRITES: [WriteFn; 0x34] = [
    inidisp, obsel, oamaddl, oamaddh, oamdata, bgmode, mosaic, bg1sc,       // 00-07
    bg2sc, bg3sc, bg4sc, bg12nba, bg34nba, bg1hofs, bg1vofs, bg2hofs,       // 08-0F
    bg2vofs, bg3hofs, bg3vofs, bg4hofs, bg4vofs, vmain, vmaddl, vmaddh,     // 10-17
    vmdatal, vmdatah, m7sel, m7a, m7b, m7c, m7d, m7x,                       // 18-1F
    m7y, cgadd, cgdata, w12sel, w34sel, wobjsel, wh0, wh1,                  // 20-27
    wh2, wh3, wbglog, wobjlog, tm, ts, tmw, tsw,                            // 28-2F
    cgwsel, cgadsub, coldata, setini,                                       // 30-33
];

/// `$2134-$2136`: the signed product `M7A × (high byte of M7B)`, 24 bits.
pub(crate) fn mode7_product(regs: &Regs) -> [u8; 3] {
    let [_, b_high] = regs.m7.b.to_le_bytes();
    let b = i32::from(signed8(b_high));
    crate::bits::product24(i32::from(regs.m7.a).wrapping_mul(b))
}
