//! The S-DSP: eight BRR voices with Gaussian interpolation and ADSR/GAIN
//! envelopes, a shared noise generator, an echo buffer with an 8-tap FIR, and
//! the output mixer, run once per 32 SPC cycles (32 kHz). Register-visible
//! behaviour (`ENDX`, `ENVX`/`OUTX`, `KON`/`KOFF` polled every other sample,
//! the five-sample key-on delay, `FLG` soft reset) follows fullsnes and the
//! `snes_spc`/ares model. `docs/emulation/apu.md`.

mod echo;
mod tables;
mod voice;

use echo::Echo;
pub(crate) use tables::{COUNTER_OFFSET, COUNTER_RANGE, COUNTER_RATE};
use voice::Voice;

/// Register offsets.
const MVOLL: usize = 0x0C;
const MVOLR: usize = 0x1C;
const EVOLL: usize = 0x2C;
const EVOLR: usize = 0x3C;
const KON: usize = 0x4C;
const KOFF: usize = 0x5C;
const FLG: usize = 0x6C;
const ENDX: usize = 0x7C;
const EFB: usize = 0x0D;
const PMON: usize = 0x2D;
const NON: usize = 0x3D;
const EON: usize = 0x4D;
const DIR: usize = 0x5D;
const ESA: usize = 0x6D;
const EDL: usize = 0x7D;

/// Clamps to a signed 16-bit range.
pub(crate) const fn clamp16(v: i32) -> i32 {
    if v > 0x7FFF {
        0x7FFF
    } else if v < -0x8000 {
        -0x8000
    } else {
        v
    }
}

/// Wraps to a signed 16-bit value (the DSP's non-saturating adders).
pub(crate) fn wrap16(v: i32) -> i32 {
    let [b0, b1, _, _] = v.to_le_bytes();
    i32::from(i16::from_le_bytes([b0, b1]))
}

/// Signed byte register.
pub(crate) fn signed(v: u8) -> i32 {
    i32::from(v.cast_signed())
}

/// Per-sample inputs every voice sees.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SampleContext {
    pub(crate) counter: u16,
    pub(crate) noise: i32,
    pub(crate) flg: u8,
    pub(crate) pmon: u8,
    pub(crate) non: u8,
    pub(crate) dir: u8,
    pub(crate) kon: u8,
    pub(crate) koff: u8,
    pub(crate) poll_keys: bool,
}

/// The DSP.
#[derive(Debug, Clone)]
pub struct Dsp {
    regs: [u8; 128],
    voices: [Voice; 8],
    echo: Echo,
    /// Shared envelope/noise rate counter.
    counter: u16,
    /// 15-bit noise LFSR.
    noise: i32,
    /// `KON` as written since the last poll.
    new_kon: u8,
    /// `ENDX` bits.
    endx: u8,
    every_other_sample: bool,
    /// Interleaved stereo output at 32 kHz, drained by the host.
    audio: Vec<i16>,
}

impl Default for Dsp {
    fn default() -> Self {
        Self::new()
    }
}

impl Dsp {
    /// Power-on: registers zero except `FLG = $E0` (reset, mute, echo off).
    #[must_use]
    pub fn new() -> Self {
        let mut regs = [0u8; 128];
        regs[FLG] = 0xE0;
        Self {
            regs,
            voices: [Voice::default(); 8],
            echo: Echo::default(),
            counter: 0,
            noise: 0x4000,
            new_kon: 0,
            endx: 0,
            every_other_sample: false,
            audio: Vec::new(),
        }
    }

    /// Reads a register (`$80-$FF` mirror `$00-$7F`).
    #[must_use]
    pub fn read(&self, addr: u8) -> u8 {
        self.regs.get(usize::from(addr & 0x7F)).copied().unwrap_or(0)
    }

    /// Writes a register; the mirror half is read-only.
    pub fn write(&mut self, addr: u8, v: u8) {
        if addr & 0x80 != 0 {
            return;
        }
        let index = usize::from(addr);
        match index {
            KON => self.new_kon = v,
            ENDX => {
                self.endx = 0;
                self.regs[ENDX] = 0;
                return;
            }
            _ => {}
        }
        if let Some(slot) = self.regs.get_mut(index) {
            *slot = v;
        }
    }

    /// Moves the queued output samples into `out` (interleaved L/R, 32 kHz).
    pub fn take_audio(&mut self, out: &mut Vec<i16>) {
        out.append(&mut self.audio);
    }

    /// Queued sample frames.
    #[must_use]
    pub const fn queued_frames(&self) -> usize {
        self.audio.len() / 2
    }

    /// Whether the rate's counter fires this sample.
    pub(crate) fn counter_poll(counter: u16, rate: u8) -> bool {
        let r = usize::from(rate & 0x1F);
        let period = COUNTER_RATE.get(r).copied().unwrap_or(0);
        if period == 0 {
            return false;
        }
        let offset = COUNTER_OFFSET.get(r).copied().unwrap_or(0);
        counter.wrapping_add(offset).checked_rem(period) == Some(0)
    }

    #[allow(clippy::missing_const_for_fn, reason = "slice::get is not const on stable")]
    fn reg(&self, index: usize) -> u8 {
        self.regs.get(index).copied().unwrap_or(0)
    }

    /// One 32 kHz sample: all voices, echo, mixer.
    pub fn run_sample(&mut self, ram: &mut [u8]) {
        self.every_other_sample = !self.every_other_sample;
        let poll_keys = self.every_other_sample;
        let (kon, koff) = if poll_keys {
            let kon = self.new_kon;
            self.new_kon = 0;
            (kon, self.reg(KOFF))
        } else {
            (0, 0)
        };
        let flg = self.reg(FLG);
        if Self::counter_poll(self.counter, flg & 0x1F) {
            let feedback = (self.noise << 13) ^ (self.noise << 14);
            self.noise = (feedback & 0x4000) ^ (self.noise >> 1);
        }
        let ctx = SampleContext {
            counter: self.counter,
            noise: self.noise,
            flg,
            pmon: self.reg(PMON),
            non: self.reg(NON),
            dir: self.reg(DIR),
            kon,
            koff,
            poll_keys,
        };
        let (main, echo_in) = self.run_voices(ram, &ctx);
        self.mix_output(ram, flg, main, echo_in);
        self.counter = if self.counter == 0 {
            COUNTER_RANGE.wrapping_sub(1)
        } else {
            self.counter.wrapping_sub(1)
        };
    }

    /// Runs the eight voices; returns the main and echo mixes.
    fn run_voices(&mut self, ram: &[u8], ctx: &SampleContext) -> ([i32; 2], [i32; 2]) {
        let eon = self.reg(EON);
        let mut main = [0i32; 2];
        let mut echo_in = [0i32; 2];
        let mut prev_output = 0i32;
        let mut endx = self.endx;
        for (v, bit) in (0..8usize).zip([1u8, 2, 4, 8, 16, 32, 64, 128]) {
            let base = v.wrapping_mul(0x10);
            let vregs: [u8; 10] = core::array::from_fn(|i| self.reg(base.wrapping_add(i)));
            let Some(voice) = self.voices.get_mut(v) else { continue };
            let input = voice::VoiceInput { regs: &vregs, ctx, index: v, prev_output, bit };
            let result = voice.run(ram, &input);
            prev_output = result.output;
            if result.kon_clears_endx {
                endx &= !bit;
            }
            if result.set_endx {
                endx |= bit;
            }
            if let Some(slot) = self.regs.get_mut(base.wrapping_add(8)) {
                *slot = result.env_x;
            }
            if let Some(slot) = self.regs.get_mut(base.wrapping_add(9)) {
                *slot = result.out_x;
            }
            for ch in 0..2 {
                let vol = signed(vregs.get(ch).copied().unwrap_or(0));
                let mixed = (result.output.wrapping_mul(vol)) >> 7;
                if let Some(m) = main.get_mut(ch) {
                    *m = clamp16(m.wrapping_add(mixed));
                }
                if eon & bit != 0
                    && let Some(e) = echo_in.get_mut(ch)
                {
                    *e = clamp16(e.wrapping_add(mixed));
                }
            }
        }
        self.endx = endx;
        self.regs[ENDX] = endx;
        (main, echo_in)
    }

    /// Echo, master volume, mute; queues one stereo frame.
    fn mix_output(&mut self, ram: &mut [u8], flg: u8, main: [i32; 2], echo_in: [i32; 2]) {
        let echo_regs = echo::EchoRegs {
            esa: self.reg(ESA),
            edl: self.reg(EDL),
            efb: signed(self.reg(EFB)),
            fir: core::array::from_fn(|i| signed(self.reg(0x0F | (i << 4)))),
            write_enabled: flg & 0x20 == 0,
        };
        let fir_out = self.echo.run(ram, &echo_regs, echo_in);
        let mvol = [signed(self.reg(MVOLL)), signed(self.reg(MVOLR))];
        let evol = [signed(self.reg(EVOLL)), signed(self.reg(EVOLR))];
        for ch in 0..2 {
            let m = main.get(ch).copied().unwrap_or(0);
            let e = fir_out.get(ch).copied().unwrap_or(0);
            let mut out = clamp16((m.wrapping_mul(mvol.get(ch).copied().unwrap_or(0))) >> 7);
            out = clamp16(
                out.wrapping_add((e.wrapping_mul(evol.get(ch).copied().unwrap_or(0))) >> 7),
            );
            if flg & 0x40 != 0 {
                out = 0;
            }
            self.audio.push(i16::try_from(out).unwrap_or(0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaussian_quadruples_sum_to_about_2048() {
        for i in 0..256usize {
            let s = tables::GAUSS[i]
                + tables::GAUSS[255 - i]
                + tables::GAUSS[256 + i]
                + tables::GAUSS[511 - i];
            assert!((0x7FF..=0x801).contains(&s), "i={i} sum={s:#X}");
        }
    }

    #[test]
    fn counter_rates_fire_at_their_period() {
        // Rate 31 fires every sample, rate 1 every 2048, rate 0 never.
        assert!((0..100).all(|c| Dsp::counter_poll(c, 31)));
        let fires_1: usize = (0..COUNTER_RANGE).filter(|&c| Dsp::counter_poll(c, 1)).count();
        assert_eq!(fires_1, usize::from(COUNTER_RANGE / 2048));
        assert!((0..100).all(|c| !Dsp::counter_poll(c, 0)));
    }

    /// A looping filter-0 BRR block of alternating +7/-8 at full shift keys
    /// on with direct gain and produces a non-silent, bounded output.
    #[test]
    fn keyed_voice_produces_sound() {
        let mut ram = vec![0u8; 0x1_0000];
        // DIR at $0200: entry 0 -> sample at $0300 (start and loop).
        ram[0x200] = 0x00;
        ram[0x201] = 0x03;
        ram[0x202] = 0x00;
        ram[0x203] = 0x03;
        // Header: shift 12, filter 0, loop+end; nybbles 7/9 (= +7/-7).
        ram[0x300] = 0xC3;
        for b in &mut ram[0x301..0x309] {
            *b = 0x79;
        }
        let mut dsp = Dsp::new();
        dsp.write(0x6C, 0x20); // FLG: no reset, no mute, echo write off
        dsp.write(0x5D, 0x02); // DIR = $0200
        dsp.write(0x0C, 0x7F);
        dsp.write(0x1C, 0x7F);
        dsp.write(0x00, 0x7F);
        dsp.write(0x01, 0x7F);
        dsp.write(0x02, 0x00);
        dsp.write(0x03, 0x10); // pitch $1000 = 32 kHz
        dsp.write(0x04, 0x00);
        dsp.write(0x05, 0x00); // GAIN mode
        dsp.write(0x07, 0x7F); // direct gain, maximum
        dsp.write(0x4C, 0x01);
        for _ in 0..200 {
            dsp.run_sample(&mut ram);
        }
        let mut out = Vec::new();
        dsp.take_audio(&mut out);
        assert_eq!(out.len(), 400);
        let peak = out.iter().map(|s| i32::from(*s).abs()).max().unwrap_or(0);
        assert!(peak > 1000, "peak {peak}");
        assert_eq!(dsp.read(0x7C) & 1, 1, "ENDX set after the looping block ends");
        assert_eq!(dsp.read(0x08), 0x7F, "ENVX at direct gain maximum");
    }
}
