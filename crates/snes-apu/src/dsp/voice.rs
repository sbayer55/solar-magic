//! One voice: BRR decoding into a 12-sample ring, pitch counter with
//! Gaussian interpolation, and the ADSR/GAIN envelope.

use super::tables::GAUSS;
use super::{Dsp, SampleContext, clamp16, wrap16};

/// Bytes in a BRR block.
const BRR_BLOCK: u16 = 9;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum EnvMode {
    #[default]
    Release,
    Attack,
    Decay,
    Sustain,
}

/// Per-voice state.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Voice {
    /// Decoded samples (16-bit, i.e. 15-bit × 2), three groups of four.
    buf: [i32; 12],
    /// Group being written next (0, 4, 8).
    buf_pos: usize,
    /// Pitch counter: bits 12-13 sample, 4-11 Gaussian index.
    interp_pos: i32,
    brr_addr: u16,
    /// Byte offset within the block (1-8).
    brr_offset: u16,
    kon_delay: u8,
    env_mode: EnvMode,
    env: i32,
    hidden_env: i32,
}

/// Everything a voice needs for one sample.
pub(crate) struct VoiceInput<'a> {
    /// `$x0-$x9` of this voice.
    pub(crate) regs: &'a [u8; 10],
    /// Shared state.
    pub(crate) ctx: &'a SampleContext,
    /// Voice number.
    pub(crate) index: usize,
    /// The previous voice's output (pitch modulation source).
    pub(crate) prev_output: i32,
    /// This voice's flag bit.
    pub(crate) bit: u8,
}

/// What a voice produced this sample.
pub(crate) struct VoiceResult {
    pub(crate) output: i32,
    pub(crate) env_x: u8,
    pub(crate) out_x: u8,
    pub(crate) set_endx: bool,
    pub(crate) kon_clears_endx: bool,
}

fn ram8(ram: &[u8], addr: u16) -> u8 {
    ram.get(usize::from(addr)).copied().unwrap_or(0)
}

fn ram16(ram: &[u8], addr: u16) -> u16 {
    u16::from_le_bytes([ram8(ram, addr), ram8(ram, addr.wrapping_add(1))])
}

impl Voice {
    #[allow(clippy::too_many_lines, reason = "the per-sample pipeline reads best in order")]
    pub(crate) fn run(&mut self, ram: &[u8], input: &VoiceInput<'_>) -> VoiceResult {
        let VoiceInput { regs, ctx, index, prev_output, bit } = *input;
        let srcn = regs[4];
        let adsr1 = regs[5];
        let adsr2 = regs[6];
        let gain = regs[7];
        let dir_entry = (u16::from(ctx.dir) << 8).wrapping_add(u16::from(srcn).wrapping_mul(4));
        let start = ram16(ram, dir_entry);
        let loop_addr = ram16(ram, dir_entry.wrapping_add(2));
        if self.kon_delay == 5 {
            self.brr_addr = start;
            self.brr_offset = 1;
            self.buf_pos = 0;
        }
        let header = if self.kon_delay == 5 { 0 } else { ram8(ram, self.brr_addr) };
        let mut pitch = i32::from(u16::from_le_bytes([regs[2], regs[3] & 0x3F]));
        if index > 0 && ctx.pmon & bit != 0 {
            pitch = pitch.wrapping_add(((prev_output >> 5).wrapping_mul(pitch)) >> 10);
        }
        let mut kon_clears_endx = false;
        if self.kon_delay > 0 {
            self.env = 0;
            self.hidden_env = 0;
            self.kon_delay = self.kon_delay.wrapping_sub(1);
            self.interp_pos = if self.kon_delay & 3 != 0 { 0x4000 } else { 0 };
            pitch = 0;
        }
        let raw = if ctx.non & bit != 0 { wrap16(ctx.noise << 1) } else { self.interpolate() };
        let output = ((raw.wrapping_mul(self.env)) >> 11) & !1;
        let env_x = u8::try_from(self.env >> 4).unwrap_or(0);
        let out_x = (output >> 8).to_le_bytes()[0];
        if ctx.flg & 0x80 != 0 || header & 3 == 1 {
            self.env_mode = EnvMode::Release;
            self.env = 0;
        }
        if ctx.poll_keys {
            if ctx.kon & bit != 0 {
                self.kon_delay = 5;
                self.env_mode = EnvMode::Attack;
                kon_clears_endx = true;
            }
            if ctx.koff & bit != 0 {
                self.env_mode = EnvMode::Release;
            }
        }
        if self.kon_delay == 0 {
            self.run_envelope(adsr1, adsr2, gain, ctx.counter);
        }
        let mut set_endx = false;
        if self.interp_pos >= 0x4000 {
            self.decode_brr(ram, header);
            self.brr_offset = self.brr_offset.wrapping_add(2);
            if self.brr_offset >= BRR_BLOCK {
                self.brr_addr = self.brr_addr.wrapping_add(BRR_BLOCK);
                if header & 1 != 0 {
                    self.brr_addr = loop_addr;
                    set_endx = true;
                }
                self.brr_offset = 1;
            }
        }
        self.interp_pos = ((self.interp_pos & 0x3FFF).wrapping_add(pitch)).min(0x7FFF);
        VoiceResult { output, env_x, out_x, set_endx, kon_clears_endx }
    }

    /// Four-point Gaussian interpolation over the ring buffer.
    fn interpolate(&self) -> i32 {
        let offset = usize::try_from((self.interp_pos >> 4) & 0xFF).unwrap_or(0);
        let base = self.buf_pos.wrapping_add(usize::try_from(self.interp_pos >> 12).unwrap_or(0));
        let sample = |k: usize| {
            self.buf.get(base.wrapping_add(k).checked_rem(12).unwrap_or(0)).copied().unwrap_or(0)
        };
        let g = |i: usize| GAUSS.get(i).copied().unwrap_or(0);
        let mut out = (g(255usize.wrapping_sub(offset)).wrapping_mul(sample(0))) >> 11;
        out = out.wrapping_add((g(511usize.wrapping_sub(offset)).wrapping_mul(sample(1))) >> 11);
        out = out.wrapping_add((g(256usize.wrapping_add(offset)).wrapping_mul(sample(2))) >> 11);
        out = wrap16(out);
        out = out.wrapping_add((g(offset).wrapping_mul(sample(3))) >> 11);
        clamp16(out) & !1
    }

    /// Decodes the next two BRR bytes (four samples) into the ring.
    fn decode_brr(&mut self, ram: &[u8], header: u8) {
        let b0 = ram8(ram, self.brr_addr.wrapping_add(self.brr_offset));
        let b1 = ram8(ram, self.brr_addr.wrapping_add(self.brr_offset).wrapping_add(1));
        let mut nybbles = i32::from(u16::from_be_bytes([b0, b1]));
        let shift = u32::from(header >> 4);
        let filter = header & 0x0C;
        let pos = self.buf_pos;
        self.buf_pos = if pos >= 8 { 0 } else { pos.wrapping_add(4) };
        for k in 0..4usize {
            // Sign-extend the top nybble.
            let mut s = wrap16(nybbles) >> 12;
            nybbles = (nybbles << 4) & 0xFFFF;
            s = s.wrapping_shl(shift) >> 1;
            if shift >= 0xD {
                s = (s >> 25) << 11;
            }
            let idx = pos.wrapping_add(k);
            let p1 = self
                .buf
                .get(idx.wrapping_add(11).checked_rem(12).unwrap_or(0))
                .copied()
                .unwrap_or(0);
            let p2 = self
                .buf
                .get(idx.wrapping_add(10).checked_rem(12).unwrap_or(0))
                .copied()
                .unwrap_or(0)
                >> 1;
            match filter {
                0x04 => {
                    s = s.wrapping_add(p1 >> 1);
                    s = s.wrapping_add(p1.wrapping_neg() >> 5);
                }
                0x08 => {
                    s = s.wrapping_add(p1).wrapping_sub(p2);
                    s = s.wrapping_add(p2 >> 4);
                    s = s.wrapping_add((p1.wrapping_mul(-3)) >> 6);
                }
                0x0C => {
                    s = s.wrapping_add(p1).wrapping_sub(p2);
                    s = s.wrapping_add((p1.wrapping_mul(-13)) >> 7);
                    s = s.wrapping_add((p2.wrapping_mul(3)) >> 4);
                }
                _ => {}
            }
            s = wrap16(clamp16(s).wrapping_mul(2));
            if let Some(slot) = self.buf.get_mut(idx.checked_rem(12).unwrap_or(0)) {
                *slot = s;
            }
        }
    }

    fn run_envelope(&mut self, adsr1: u8, adsr2: u8, gain: u8, counter: u16) {
        let mut env = self.env;
        if self.env_mode == EnvMode::Release {
            self.env = (env.wrapping_sub(8)).max(0);
            return;
        }
        let rate: u8;
        let exp_step = |e: i32| e.wrapping_sub(((e.wrapping_sub(1)) >> 8).wrapping_add(1));
        if adsr1 & 0x80 != 0 {
            match self.env_mode {
                EnvMode::Attack => {
                    rate = (adsr1 & 0x0F).wrapping_mul(2).wrapping_add(1);
                    env = env.wrapping_add(if rate < 31 { 0x20 } else { 0x400 });
                }
                EnvMode::Decay => {
                    rate = ((adsr1 >> 4) & 7).wrapping_mul(2).wrapping_add(16);
                    env = exp_step(env);
                }
                EnvMode::Sustain | EnvMode::Release => {
                    rate = adsr2 & 0x1F;
                    env = exp_step(env);
                }
            }
        } else if gain < 0x80 {
            env = i32::from(gain).wrapping_mul(16);
            rate = 31;
        } else {
            rate = gain & 0x1F;
            env = match (gain >> 5) & 3 {
                0 => env.wrapping_sub(0x20),
                1 => exp_step(env),
                2 => env.wrapping_add(0x20),
                _ => env.wrapping_add(if self.hidden_env < 0x600 { 0x20 } else { 8 }),
            };
        }
        if self.env_mode == EnvMode::Decay && (env >> 8) == i32::from(adsr2 >> 5) {
            self.env_mode = EnvMode::Sustain;
        }
        self.hidden_env = env;
        if !(0..=0x7FF).contains(&env) {
            env = if env < 0 { 0 } else { 0x7FF };
            if self.env_mode == EnvMode::Attack {
                self.env_mode = EnvMode::Decay;
            }
        }
        if Dsp::counter_poll(counter, rate) {
            self.env = env;
        }
    }
}
