//! A SNES-shaped bus for running the game's own routines: ROM through the
//! cartridge mapper, 128 KiB of WRAM with its low mirror, and hardware
//! registers that read as zero and swallow writes.

use std::collections::{BTreeMap, BTreeSet};

use snes_rom::{Rom, SnesAddr};
use w65c816::{Addr, Bus};

/// Size of work RAM.
pub const WRAM_LEN: usize = 0x2_0000;

/// Canonical bus address of WRAM index 0 (`$7E:0000`).
const WRAM_BASE: Addr = 0x7E_0000;

/// First APU I/O port (`$2140`–`$2143`).
const APU_PORT_BASE: u32 = 0x2140;
/// What the SPC700 boot ROM shows on ports 0 and 1 while it waits for an
/// upload; afterwards each port echoes the last byte the CPU wrote, which
/// is what the game's upload loops wait for.
const APU_READY: [u8; 4] = [0xAA, 0xBB, 0, 0];

/// Every address a traced run touched (see [`SnesBus::enable_trace`]).
/// WRAM addresses are canonical `$7E`/`$7F` ones, whichever mirror the code
/// used; ROM addresses are the bus addresses as read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trace {
    /// ROM (or overlay) bytes read.
    pub rom_reads: BTreeSet<Addr>,
    /// WRAM bytes written by the run.
    pub wram_writes: BTreeSet<Addr>,
    /// WRAM bytes read before the run wrote them (values seeded through
    /// [`SnesBus::set_wram`] count as written).
    pub wram_reads_uninit: BTreeSet<Addr>,
    /// Hardware register accesses: address → `(reads, writes)`.
    pub hardware: BTreeMap<Addr, (u64, u64)>,
    seeded: BTreeSet<Addr>,
}

impl Trace {
    /// Coalesces addresses into inclusive `(first, last)` runs, merging
    /// neighbours less than `gap` bytes apart.
    #[must_use]
    pub fn ranges(addrs: &BTreeSet<Addr>, gap: u32) -> Vec<(Addr, Addr)> {
        let mut out: Vec<(Addr, Addr)> = Vec::new();
        for &addr in addrs {
            match out.last_mut() {
                Some((_, last)) if addr.saturating_sub(*last) <= gap.max(1) => *last = addr,
                _ => out.push((addr, addr)),
            }
        }
        out
    }

    fn note_wram_read(&mut self, addr: Addr) {
        if !self.wram_writes.contains(&addr) && !self.seeded.contains(&addr) {
            self.wram_reads_uninit.insert(addr);
        }
    }

    fn note_hardware(&mut self, addr: Addr, write: bool) {
        let entry = self.hardware.entry(addr).or_default();
        if write {
            entry.1 = entry.1.saturating_add(1);
        } else {
            entry.0 = entry.0.saturating_add(1);
        }
    }
}

/// Bytes that shadow the ROM at a bus address range: how edited level data
/// is fed to the game's loader without touching the ROM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overlay {
    /// First bus address covered.
    pub start: Addr,
    /// The bytes.
    pub bytes: Vec<u8>,
}

impl Overlay {
    fn get(&self, addr: Addr) -> Option<u8> {
        let index = usize::try_from(addr.checked_sub(self.start)?).ok()?;
        self.bytes.get(index).copied()
    }
}

/// ROM plus WRAM, no hardware.
pub struct SnesBus<'a> {
    rom: &'a Rom,
    wram: Vec<u8>,
    overlays: Vec<Overlay>,
    hardware_writes: u64,
    trace: Option<Trace>,
    apu: [u8; 4],
    /// Whether the CPU has started the SPC handshake (first write to port 0);
    /// until then the ports show the boot ROM's ready bytes.
    apu_started: bool,
}

impl core::fmt::Debug for SnesBus<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SnesBus")
            .field("hardware_writes", &self.hardware_writes)
            .finish_non_exhaustive()
    }
}

impl<'a> SnesBus<'a> {
    /// Creates a bus over `rom` with zeroed WRAM.
    #[must_use]
    pub fn new(rom: &'a Rom) -> Self {
        Self {
            rom,
            wram: vec![0; WRAM_LEN],
            overlays: Vec::new(),
            hardware_writes: 0,
            trace: None,
            apu: APU_READY,
            apu_started: false,
        }
    }

    /// Starts recording every access into a [`Trace`]. Call it before
    /// seeding WRAM so the seeds are not reported as uninitialised reads.
    pub fn enable_trace(&mut self) {
        self.trace = Some(Trace::default());
    }

    /// Stops recording and returns what was recorded, if tracing was on.
    pub const fn take_trace(&mut self) -> Option<Trace> {
        self.trace.take()
    }

    /// Makes `overlay` shadow the ROM for reads of its address range.
    pub fn add_overlay(&mut self, overlay: Overlay) {
        self.overlays.push(overlay);
    }

    /// WRAM as a slice (`$7E:0000` is index 0).
    #[must_use]
    pub fn wram(&self) -> &[u8] {
        &self.wram
    }

    /// Writes to hardware registers seen so far (useful to confirm a routine
    /// stayed in pure RAM).
    #[must_use]
    pub const fn hardware_writes(&self) -> u64 {
        self.hardware_writes
    }

    /// Reads a WRAM byte by 24-bit address, if it is WRAM.
    #[must_use]
    pub fn wram_at(&self, addr: Addr) -> Option<u8> {
        wram_index(addr).and_then(|i| self.wram.get(i).copied())
    }

    /// Reads a byte the way the CPU would (WRAM, overlays, then ROM), without
    /// recording it in the trace. Hardware registers and unmapped addresses
    /// read as 0.
    #[must_use]
    pub fn peek(&self, addr: Addr) -> u8 {
        if let Some(v) = self.wram_at(addr) {
            return v;
        }
        if is_hardware(addr) {
            return 0;
        }
        if let Some(v) = self.overlays.iter().find_map(|o| o.get(addr)) {
            return v;
        }
        SnesAddr::new(addr & 0xFF_FFFF)
            .and_then(|a| self.rom.pc(a).ok())
            .and_then(|pc| self.rom.bytes().get(pc).copied())
            .unwrap_or(0)
    }

    /// Reads a little-endian 16-bit value with [`SnesBus::peek`].
    #[must_use]
    pub fn peek_u16(&self, addr: Addr) -> u16 {
        u16::from_le_bytes([self.peek(addr), self.peek(addr.wrapping_add(1))])
    }

    /// Reads a little-endian 24-bit value with [`SnesBus::peek`].
    #[must_use]
    pub fn peek_u24(&self, addr: Addr) -> u32 {
        u32::from_le_bytes([
            self.peek(addr),
            self.peek(addr.wrapping_add(1)),
            self.peek(addr.wrapping_add(2)),
            0,
        ])
    }

    /// Reads a 24-bit little-endian value from WRAM.
    #[must_use]
    pub fn wram_u24(&self, addr: Addr) -> Option<u32> {
        let lo = self.wram_at(addr)?;
        let mid = self.wram_at(addr.wrapping_add(1))?;
        let hi = self.wram_at(addr.wrapping_add(2))?;
        Some(u32::from_le_bytes([lo, mid, hi, 0]))
    }

    /// Stores a byte in WRAM by 24-bit address (ignored if not WRAM).
    pub fn set_wram(&mut self, addr: Addr, value: u8) {
        if let Some(i) = wram_index(addr)
            && let Some(slot) = self.wram.get_mut(i)
        {
            *slot = value;
            if let (Some(trace), Some(canonical)) = (self.trace.as_mut(), canonical_wram(i)) {
                trace.seeded.insert(canonical);
            }
        }
    }

    /// Stores a 16-bit little-endian value in WRAM.
    pub fn set_wram_u16(&mut self, addr: Addr, value: u16) {
        let [lo, hi] = value.to_le_bytes();
        self.set_wram(addr, lo);
        self.set_wram(addr.wrapping_add(1), hi);
    }

    /// Stores a 24-bit little-endian value in WRAM.
    pub fn set_wram_u24(&mut self, addr: Addr, value: u32) {
        let [lo, mid, hi, _] = value.to_le_bytes();
        self.set_wram(addr, lo);
        self.set_wram(addr.wrapping_add(1), mid);
        self.set_wram(addr.wrapping_add(2), hi);
    }
}

/// Maps a bus address to a WRAM index: banks `$7E`/`$7F` fully, plus the
/// `$0000-$1FFF` mirror in banks `$00-$3F` and `$80-$BF`.
pub(crate) fn wram_index(addr: Addr) -> Option<usize> {
    let bank = (addr >> 16) & 0xFF;
    let offset = addr & 0xFFFF;
    match bank {
        0x7E => usize::try_from(offset).ok(),
        0x7F => usize::try_from(offset | 0x1_0000).ok(),
        0x00..=0x3F | 0x80..=0xBF if offset < 0x2000 => usize::try_from(offset).ok(),
        _ => None,
    }
}

/// The `$7E`/`$7F` address of a WRAM index.
fn canonical_wram(index: usize) -> Option<Addr> {
    u32::try_from(index).ok()?.checked_add(WRAM_BASE)
}

/// Whether an address is a hardware register (B bus, CPU, DMA).
const fn is_hardware(addr: Addr) -> bool {
    let bank = (addr >> 16) & 0xFF;
    let offset = addr & 0xFFFF;
    matches!(bank, 0x00..=0x3F | 0x80..=0xBF) && offset >= 0x2000 && offset < 0x6000
}

impl Bus for SnesBus<'_> {
    fn read(&mut self, addr: Addr) -> u8 {
        if let Some(index) = wram_index(addr) {
            if let (Some(trace), Some(canonical)) = (self.trace.as_mut(), canonical_wram(index)) {
                trace.note_wram_read(canonical);
            }
            return self.wram.get(index).copied().unwrap_or(0);
        }
        if is_hardware(addr) {
            if let Some(trace) = self.trace.as_mut() {
                trace.note_hardware(addr, false);
            }
            let ports = if self.apu_started { &self.apu } else { &APU_READY };
            return apu_port(addr).and_then(|i| ports.get(i).copied()).unwrap_or(0);
        }
        if let Some(trace) = self.trace.as_mut() {
            trace.rom_reads.insert(addr & 0xFF_FFFF);
        }
        if let Some(v) = self.overlays.iter().find_map(|o| o.get(addr)) {
            return v;
        }
        SnesAddr::new(addr & 0xFF_FFFF)
            .and_then(|a| self.rom.pc(a).ok())
            .and_then(|pc| self.rom.bytes().get(pc).copied())
            .unwrap_or(0)
    }

    fn write(&mut self, addr: Addr, value: u8) {
        if let Some(index) = wram_index(addr) {
            if let Some(slot) = self.wram.get_mut(index) {
                *slot = value;
            }
            if let (Some(trace), Some(canonical)) = (self.trace.as_mut(), canonical_wram(index)) {
                trace.wram_writes.insert(canonical);
            }
        } else if is_hardware(addr) {
            self.hardware_writes = self.hardware_writes.saturating_add(1);
            if let Some(port) = apu_port(addr) {
                if port == 0 {
                    self.apu_started = true;
                }
                if let Some(slot) = self.apu.get_mut(port) {
                    *slot = value;
                }
            }
            if let Some(trace) = self.trace.as_mut() {
                trace.note_hardware(addr, true);
            }
        }
    }
}

/// Which APU port (0–3) a hardware address is, if any.
fn apu_port(addr: Addr) -> Option<usize> {
    let offset = addr & 0xFFFF;
    (APU_PORT_BASE..APU_PORT_BASE.saturating_add(4))
        .contains(&offset)
        .then(|| usize::try_from(offset.wrapping_sub(APU_PORT_BASE)).ok())
        .flatten()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use w65c816::Bus as _;

    use super::{SnesBus, Trace, wram_index};

    #[test]
    fn ranges_merge_within_gap() {
        let set: BTreeSet<u32> = [0x10, 0x11, 0x12, 0x20, 0x21, 0x40].into_iter().collect();
        assert_eq!(Trace::ranges(&set, 1), vec![(0x10, 0x12), (0x20, 0x21), (0x40, 0x40)]);
        assert_eq!(Trace::ranges(&set, 16), vec![(0x10, 0x21), (0x40, 0x40)]);
    }

    #[test]
    fn trace_records_canonical_wram_and_uninitialised_reads() {
        // A 64 KiB LoROM image with a plausible internal header.
        let mut image = vec![0xEAu8; 0x1_0000];
        let header = *b"SUPER MARIOWORLD     ";
        image.get_mut(0x7FC0..0x7FD5).expect("in range").copy_from_slice(&header);
        image
            .get_mut(0x7FD5..0x7FE0)
            .expect("in range")
            .copy_from_slice(&[0x20, 0x02, 0x06, 0x01, 0x01, 0x01, 0x00, 0xFF, 0xFF, 0x00, 0x00]);
        let rom = snes_rom::Rom::from_bytes(image).expect("minimal rom");
        let mut bus = SnesBus::new(&rom);
        bus.enable_trace();
        bus.set_wram(0x00_0065, 1);
        assert_eq!(bus.read(0x00_0065), 1, "seeded value");
        assert_eq!(bus.read(0x7E_0066), 0, "never written");
        bus.write(0x00_0067, 9);
        assert_eq!(bus.read(0x80_0067), 9, "mirror of the same byte");
        bus.write(0x00_2100, 0);
        let _ = bus.read(0x05_8000);
        let trace = bus.take_trace().expect("tracing was on");
        assert_eq!(trace.wram_reads_uninit, std::iter::once(0x7E_0066).collect());
        assert_eq!(trace.wram_writes, std::iter::once(0x7E_0067).collect());
        assert_eq!(trace.hardware.get(&0x00_2100), Some(&(0, 1)));
        assert_eq!(trace.rom_reads, std::iter::once(0x05_8000).collect());
        assert!(bus.take_trace().is_none());
    }

    #[test]
    fn apu_ports_echo_writes() {
        let mut image = vec![0xEAu8; 0x1_0000];
        let header = *b"SUPER MARIOWORLD     ";
        image.get_mut(0x7FC0..0x7FD5).expect("in range").copy_from_slice(&header);
        image
            .get_mut(0x7FD5..0x7FE0)
            .expect("in range")
            .copy_from_slice(&[0x20, 0x02, 0x06, 0x01, 0x01, 0x01, 0x00, 0xFF, 0xFF, 0x00, 0x00]);
        let rom = snes_rom::Rom::from_bytes(image).expect("minimal rom");
        let mut bus = SnesBus::new(&rom);
        assert_eq!((bus.read(0x00_2140), bus.read(0x00_2141)), (0xAA, 0xBB));
        bus.write(0x00_2141, 0x01);
        assert_eq!(bus.read(0x00_2141), 0xBB, "data written before the handshake is not echoed");
        bus.write(0x00_2140, 0x42);
        assert_eq!(bus.read(0x00_2140), 0x42);
        assert_eq!(bus.read(0x00_2141), 0x01);
        assert_eq!(bus.read(0x80_2143), 0);
        assert_eq!(bus.read(0x00_2100), 0, "other registers still read 0");
    }

    #[test]
    fn wram_mapping() {
        assert_eq!(wram_index(0x7E_0000), Some(0));
        assert_eq!(wram_index(0x7F_C800), Some(0x1_C800));
        assert_eq!(wram_index(0x00_0065), Some(0x65));
        assert_eq!(wram_index(0x80_1FFF), Some(0x1FFF));
        assert_eq!(wram_index(0x00_2000), None);
        assert_eq!(wram_index(0x05_E000), None);
    }
}
