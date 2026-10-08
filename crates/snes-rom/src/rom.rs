//! The ROM image container.

use std::path::Path;
use std::{fs, io};

use crate::{InternalHeader, LoRom, MapKind, Mapper, MapperKind, RomError, SnesAddr};

/// Size of the optional copier header some ROM dumps carry.
const COPIER_HEADER_LEN: usize = 512;

/// File offset of the internal header for LoROM images.
const LOROM_HEADER_PC: usize = 0x7FC0;

/// File offset of the internal header for HiROM images.
const HIROM_HEADER_PC: usize = 0xFFC0;

/// Lowest [`InternalHeader::plausibility`] score accepted as a real header.
///
/// A consistent checksum pair alone scores 4; so does a known map mode plus
/// a sane size code plus a printable title. An all-zero image scores 2.
const MIN_PLAUSIBILITY: u8 = 4;

/// Offset of the ROM size code within the internal header.
const SIZE_CODE_OFFSET: usize = 0x17;

/// Offset of the checksum complement within the internal header.
const CHECKSUM_COMPLEMENT_OFFSET: usize = 0x1C;

/// Offset of the checksum within the internal header.
const CHECKSUM_OFFSET: usize = 0x1E;

/// A loaded SNES ROM image with its detected address mapping.
///
/// The image is read through [`Rom::read`] and friends and modified through
/// [`Rom::write`], [`Rom::expand_to`] and [`Rom::fix_checksum`]; the parsed
/// [`InternalHeader`] is kept in sync with the bytes after every write.
#[derive(Debug, Clone)]
pub struct Rom {
    data: Vec<u8>,
    /// The 512-byte copier header stripped on load, if there was one. It is
    /// written back by [`Rom::save`] so the file keeps its original shape.
    copier_header: Option<Vec<u8>>,
    header: InternalHeader,
    /// File offset of the internal header.
    header_pc: usize,
    mapper: MapperKind,
}

impl Rom {
    /// Loads a ROM image from disk.
    ///
    /// # Errors
    ///
    /// Returns [`RomError::Io`] if the file cannot be read, plus anything
    /// [`Rom::from_bytes`] can return.
    pub fn load(path: &Path) -> Result<Self, RomError> {
        let data =
            fs::read(path).map_err(|source| RomError::Io { path: path.to_owned(), source })?;
        Self::from_bytes(data)
    }

    /// Wraps an in-memory image, stripping a 512-byte copier header if present
    /// and detecting the mapping from the internal header.
    ///
    /// # Errors
    ///
    /// - [`RomError::TooSmall`] if fewer than 32 KiB remain.
    /// - [`RomError::NoHeader`] if no plausible header is found.
    /// - [`RomError::UnsupportedMapping`] for HiROM, ExHiROM, SA-1 and ExLoROM
    ///   (not implemented yet).
    pub fn from_bytes(mut data: Vec<u8>) -> Result<Self, RomError> {
        let copier_header = (data.len() % 1024 == COPIER_HEADER_LEN)
            .then(|| data.drain(..COPIER_HEADER_LEN).collect::<Vec<u8>>());
        if data.len() < 0x8000 {
            return Err(RomError::TooSmall { len: data.len() });
        }
        let Detected { header, header_pc, mapper } = detect_mapping(&data)?;
        Ok(Self { data, copier_header, header, header_pc, mapper })
    }

    /// Size of the image in bytes (excluding any stripped copier header).
    #[must_use]
    pub const fn len(&self) -> usize {
        self.data.len()
    }

    /// Whether the image is empty. Always `false` for a successfully loaded ROM.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// The raw bytes of the image.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }

    /// Whether a 512-byte copier header was stripped on load.
    #[must_use]
    pub const fn had_copier_header(&self) -> bool {
        self.copier_header.is_some()
    }

    /// The copier header stripped on load, if any.
    #[must_use]
    pub fn copier_header(&self) -> Option<&[u8]> {
        self.copier_header.as_deref()
    }

    /// The parsed internal header.
    #[must_use]
    pub const fn header(&self) -> &InternalHeader {
        &self.header
    }

    /// The active address mapper.
    #[must_use]
    pub fn mapper(&self) -> &dyn Mapper {
        &self.mapper
    }

    /// Translates a bus address to a file offset within this image.
    ///
    /// # Errors
    ///
    /// [`RomError::Unmapped`] if the address is not ROM, or
    /// [`RomError::OutOfRange`] if it maps beyond the end of the image.
    pub fn pc(&self, addr: SnesAddr) -> Result<usize, RomError> {
        let pc = self.mapper.to_pc(addr).ok_or(RomError::Unmapped(addr))?;
        if pc >= self.data.len() {
            return Err(RomError::OutOfRange { addr, len: 1, rom_len: self.data.len() });
        }
        Ok(pc)
    }

    /// Reads `len` contiguous bytes starting at `addr`.
    ///
    /// Contiguity is in file space: in LoROM a read starting near `$05:FFFF`
    /// continues into `$06:8000`, matching how the console's address space
    /// wraps for sequential ROM data.
    ///
    /// # Errors
    ///
    /// [`RomError::Unmapped`] or [`RomError::OutOfRange`].
    pub fn read(&self, addr: SnesAddr, len: usize) -> Result<&[u8], RomError> {
        let start = self.pc(addr)?;
        let out_of_range = || RomError::OutOfRange { addr, len, rom_len: self.data.len() };
        let end = start.checked_add(len).ok_or_else(out_of_range)?;
        self.data.get(start..end).ok_or_else(out_of_range)
    }

    /// Reads one byte.
    ///
    /// # Errors
    ///
    /// See [`Rom::read`].
    pub fn read_u8(&self, addr: SnesAddr) -> Result<u8, RomError> {
        self.read(addr, 1)?.first().copied().ok_or(RomError::Unmapped(addr))
    }

    /// Reads a little-endian 16-bit word.
    ///
    /// # Errors
    ///
    /// See [`Rom::read`].
    pub fn read_u16_le(&self, addr: SnesAddr) -> Result<u16, RomError> {
        let b = self.read(addr, 2)?;
        Ok(u16::from_le_bytes([b.first().copied().unwrap_or(0), b.get(1).copied().unwrap_or(0)]))
    }

    /// Reads a little-endian 24-bit value (the usual SNES long pointer).
    ///
    /// # Errors
    ///
    /// See [`Rom::read`].
    pub fn read_u24_le(&self, addr: SnesAddr) -> Result<u32, RomError> {
        let b = self.read(addr, 3)?;
        let lo = b.first().copied().unwrap_or(0);
        let mid = b.get(1).copied().unwrap_or(0);
        let hi = b.get(2).copied().unwrap_or(0);
        Ok(u32::from_le_bytes([lo, mid, hi, 0]))
    }

    /// Reads a 24-bit little-endian pointer as an address.
    ///
    /// # Errors
    ///
    /// See [`Rom::read`]. A 24-bit value always fits in [`SnesAddr`].
    pub fn read_ptr(&self, addr: SnesAddr) -> Result<SnesAddr, RomError> {
        let raw = self.read_u24_le(addr)?;
        SnesAddr::new(raw).ok_or(RomError::Unmapped(addr))
    }

    /// Overwrites bytes starting at file offset `pc`.
    ///
    /// The parsed header is refreshed if the write touches it.
    ///
    /// # Errors
    ///
    /// [`RomError::WriteOutOfRange`] if the range does not fit in the image.
    pub fn write_pc(&mut self, pc: usize, bytes: &[u8]) -> Result<(), RomError> {
        let rom_len = self.data.len();
        let out_of_range = || RomError::WriteOutOfRange { pc, len: bytes.len(), rom_len };
        let end = pc.checked_add(bytes.len()).ok_or_else(out_of_range)?;
        self.data.get_mut(pc..end).ok_or_else(out_of_range)?.copy_from_slice(bytes);
        let header_end = self.header_pc.saturating_add(InternalHeader::LEN);
        if pc < header_end && end > self.header_pc {
            self.refresh_header();
        }
        Ok(())
    }

    /// Overwrites bytes starting at bus address `addr`, contiguous in file
    /// space like [`Rom::read`].
    ///
    /// # Errors
    ///
    /// [`RomError::Unmapped`] or [`RomError::WriteOutOfRange`].
    pub fn write(&mut self, addr: SnesAddr, bytes: &[u8]) -> Result<(), RomError> {
        let pc = self.pc(addr)?;
        self.write_pc(pc, bytes)
    }

    /// Grows the image to `len` bytes, padding with `0x00` and updating the
    /// header's size code. Lunar Magic expands to 1 MiB on first save.
    ///
    /// The checksum is not recomputed; call [`Rom::fix_checksum`] afterwards.
    ///
    /// # Errors
    ///
    /// - [`RomError::SizeNotPowerOfTwo`] unless `len` is a power of two.
    /// - [`RomError::Shrink`] if `len` is below the current size.
    /// - [`RomError::TooLargeForMapping`] if the mapper cannot address `len`.
    pub fn expand_to(&mut self, len: usize) -> Result<(), RomError> {
        if !len.is_power_of_two() {
            return Err(RomError::SizeNotPowerOfTwo { len });
        }
        if len < self.data.len() {
            return Err(RomError::Shrink { from: self.data.len(), to: len });
        }
        let max = self.mapper.max_size();
        if len > max {
            return Err(RomError::TooLargeForMapping { len, max, mapping: self.mapper.name() });
        }
        self.data.resize(len, 0);
        let code = u8::try_from(len.trailing_zeros().saturating_sub(10)).unwrap_or(u8::MAX);
        let pc = self.header_pc.saturating_add(SIZE_CODE_OFFSET);
        self.write_pc(pc, &[code])
    }

    /// Recomputes the checksum and writes it and its complement into the
    /// header. Returns the new checksum.
    ///
    /// The four checksum bytes are set to `FF FF 00 00` before summing so that
    /// they contribute the same `0x1FE` whatever the result is.
    pub fn fix_checksum(&mut self) -> u16 {
        let complement_pc = self.header_pc.saturating_add(CHECKSUM_COMPLEMENT_OFFSET);
        let checksum_pc = self.header_pc.saturating_add(CHECKSUM_OFFSET);
        // The header always fits, so these writes cannot fail.
        let _ = self.write_pc(complement_pc, &[0xFF, 0xFF, 0x00, 0x00]);
        let sum = self.compute_checksum();
        let _ = self.write_pc(complement_pc, &(!sum).to_le_bytes());
        let _ = self.write_pc(checksum_pc, &sum.to_le_bytes());
        sum
    }

    /// Writes the image to `path`, re-prepending the copier header if one was
    /// stripped on load. The file is written to a sibling temporary file and
    /// renamed into place so a failure never leaves a truncated ROM.
    ///
    /// # Errors
    ///
    /// [`RomError::Io`] for any filesystem failure.
    pub fn save(&self, path: &Path) -> Result<(), RomError> {
        let io = |source: io::Error| RomError::Io { path: path.to_owned(), source };
        let mut out = Vec::with_capacity(
            self.data.len().saturating_add(self.copier_header.as_ref().map_or(0, Vec::len)),
        );
        if let Some(header) = &self.copier_header {
            out.extend_from_slice(header);
        }
        out.extend_from_slice(&self.data);
        let file_name = path.file_name().map(std::ffi::OsStr::to_os_string).unwrap_or_default();
        let mut tmp_name = file_name;
        tmp_name.push(".tmp");
        let tmp = path.with_file_name(tmp_name);
        fs::write(&tmp, &out).map_err(io)?;
        fs::rename(&tmp, path).map_err(io)
    }

    /// Re-parses the header from the image after a write touched it.
    fn refresh_header(&mut self) {
        let end = self.header_pc.saturating_add(InternalHeader::LEN);
        if let Some(parsed) = self.data.get(self.header_pc..end).and_then(InternalHeader::parse) {
            self.header = parsed;
        }
    }

    /// Computes the SNES checksum of the image: the 16-bit sum of all bytes,
    /// with images whose size is not a power of two mirrored up to one.
    #[must_use]
    pub fn compute_checksum(&self) -> u16 {
        let len = self.data.len();
        let sum = |slice: &[u8]| slice.iter().fold(0u32, |acc, &b| acc.wrapping_add(u32::from(b)));
        if len.is_power_of_two() {
            return truncate_u16(sum(&self.data));
        }
        // Split into the largest power-of-two prefix plus a remainder that is
        // repeated until the total is the next power of two.
        let prefix_len = len.next_power_of_two() >> 1;
        let (prefix, rest) = self.data.split_at(prefix_len.min(len));
        let rest_sum = sum(rest);
        let repeats = u32::try_from(prefix_len.checked_div(rest.len()).unwrap_or(0)).unwrap_or(1);
        truncate_u16(sum(prefix).wrapping_add(rest_sum.wrapping_mul(repeats)))
    }

    /// Whether the stored header checksum matches [`Rom::compute_checksum`].
    #[must_use]
    pub fn checksum_matches(&self) -> bool {
        self.compute_checksum() == self.header.checksum
    }
}

const fn truncate_u16(v: u32) -> u16 {
    #[allow(clippy::as_conversions, reason = "value is masked to 16 bits before the cast")]
    {
        (v & 0xFFFF) as u16
    }
}

/// A parsed header, where it was found, and the mapper it implies.
struct Detected {
    header: InternalHeader,
    header_pc: usize,
    mapper: MapperKind,
}

/// Reads the headers at both candidate locations and picks the most plausible.
fn detect_mapping(data: &[u8]) -> Result<Detected, RomError> {
    let header_at = |pc: usize| {
        data.get(pc..pc.checked_add(InternalHeader::LEN)?).and_then(InternalHeader::parse)
    };
    let lo = header_at(LOROM_HEADER_PC);
    let hi = header_at(HIROM_HEADER_PC);
    let score = |h: &Option<InternalHeader>| h.as_ref().map_or(0, InternalHeader::plausibility);
    let (header, header_pc) =
        if score(&lo) >= score(&hi) { (lo, LOROM_HEADER_PC) } else { (hi, HIROM_HEADER_PC) };
    let header = header.ok_or(RomError::NoHeader)?;
    if header.plausibility() < MIN_PLAUSIBILITY {
        return Err(RomError::NoHeader);
    }
    match header.map_kind() {
        MapKind::LoRom => Ok(Detected { header, header_pc, mapper: MapperKind::LoRom(LoRom) }),
        MapKind::HiRom => Err(RomError::UnsupportedMapping("HiROM")),
        MapKind::ExHiRom => Err(RomError::UnsupportedMapping("ExHiROM")),
        MapKind::ExLoRom => Err(RomError::UnsupportedMapping("ExLoROM")),
        MapKind::Sa1 => Err(RomError::UnsupportedMapping("SA-1")),
        MapKind::Unknown(_) => Err(RomError::NoHeader),
    }
}

#[cfg(test)]
mod tests {
    use super::Rom;
    use crate::{RomError, SnesAddr};

    /// Builds a 64 KiB LoROM image with the SMW header and a recognisable
    /// byte pattern (`pc & 0xFF`) everywhere else.
    fn synthetic_rom(copier_header: bool) -> Vec<u8> {
        let mut data: Vec<u8> =
            (0..0x1_0000u32).map(|i| u8::try_from(i & 0xFF).unwrap_or(0)).collect();
        data[0x7FC0..0x7FE0].copy_from_slice(&crate::test_fixtures::SMW_HEADER);
        // Fix the size code to 64 KiB (code 6) so the header stays plausible.
        data[0x7FD7] = 0x06;
        if copier_header {
            let mut with = vec![0xAAu8; 512];
            with.extend_from_slice(&data);
            with
        } else {
            data
        }
    }

    #[test]
    fn loads_synthetic_lorom() {
        let rom = Rom::from_bytes(synthetic_rom(false)).expect("valid");
        assert_eq!(rom.len(), 0x1_0000);
        assert!(!rom.had_copier_header());
        assert_eq!(rom.header().title, "SUPER MARIOWORLD");
        assert_eq!(rom.mapper().name(), "LoROM");
    }

    #[test]
    fn strips_copier_header() {
        let rom = Rom::from_bytes(synthetic_rom(true)).expect("valid");
        assert!(rom.had_copier_header());
        assert_eq!(rom.len(), 0x1_0000);
        assert_eq!(rom.read_u8(SnesAddr::lit(0x00_8000)).expect("mapped"), 0x00);
    }

    #[test]
    fn reads_follow_lorom_mapping() {
        let rom = Rom::from_bytes(synthetic_rom(false)).expect("valid");
        assert_eq!(rom.read_u8(SnesAddr::lit(0x00_8010)).expect("mapped"), 0x10);
        assert_eq!(rom.read_u8(SnesAddr::lit(0x01_8003)).expect("mapped"), 0x03);
        assert_eq!(rom.read_u16_le(SnesAddr::lit(0x00_8010)).expect("mapped"), 0x1110);
        assert_eq!(rom.read_u24_le(SnesAddr::lit(0x00_8010)).expect("mapped"), 0x12_1110);
        assert_eq!(
            rom.read_ptr(SnesAddr::lit(0x00_8010)).expect("mapped"),
            SnesAddr::lit(0x12_1110)
        );
        // Reads continue across the half-bank seam in file order.
        let seam = rom.read(SnesAddr::lit(0x00_FFFE), 4).expect("mapped");
        assert_eq!(seam, &[0xFE, 0xFF, 0x00, 0x01]);
    }

    #[test]
    fn errors_are_specific() {
        let rom = Rom::from_bytes(synthetic_rom(false)).expect("valid");
        assert!(matches!(rom.read_u8(SnesAddr::lit(0x00_0000)), Err(RomError::Unmapped(_))));
        assert!(matches!(rom.read_u8(SnesAddr::lit(0x7E_8000)), Err(RomError::Unmapped(_))));
        assert!(matches!(rom.read_u8(SnesAddr::lit(0x02_8000)), Err(RomError::OutOfRange { .. })));
        assert!(matches!(
            rom.read(SnesAddr::lit(0x01_FFFF), 2),
            Err(RomError::OutOfRange { len: 2, .. })
        ));
    }

    #[test]
    fn write_refreshes_header_and_checks_bounds() {
        let mut rom = Rom::from_bytes(synthetic_rom(false)).expect("valid");
        rom.write(SnesAddr::lit(0x00_FFC0), b"SOLAR").expect("in range");
        assert_eq!(rom.header().title, "SOLAR MARIOWORLD");
        rom.write_pc(0x1234, &[0xAB, 0xCD]).expect("in range");
        assert_eq!(rom.read(SnesAddr::lit(0x00_9234), 2).expect("mapped"), &[0xAB, 0xCD]);
        assert!(matches!(
            rom.write_pc(0xFFFF, &[1, 2]),
            Err(RomError::WriteOutOfRange { pc: 0xFFFF, len: 2, .. })
        ));
        assert!(matches!(rom.write(SnesAddr::lit(0x7E_0000), &[1]), Err(RomError::Unmapped(_))));
    }

    #[test]
    fn expand_pads_and_updates_size_code() {
        let mut rom = Rom::from_bytes(synthetic_rom(false)).expect("valid");
        let before = rom.bytes().to_vec();
        rom.expand_to(0x2_0000).expect("doubles");
        assert_eq!(rom.len(), 0x2_0000);
        assert_eq!(rom.header().rom_size_code, 7);
        assert_eq!(&rom.bytes()[..0x7FD7], &before[..0x7FD7]);
        assert_eq!(&rom.bytes()[0x7FD8..0x1_0000], &before[0x7FD8..]);
        assert!(rom.bytes()[0x1_0000..].iter().all(|&b| b == 0));
        assert!(matches!(rom.expand_to(0x3_0000), Err(RomError::SizeNotPowerOfTwo { .. })));
        assert!(matches!(rom.expand_to(0x1_0000), Err(RomError::Shrink { .. })));
        assert!(matches!(rom.expand_to(0x80_0000), Err(RomError::TooLargeForMapping { .. })));
    }

    #[test]
    fn fix_checksum_makes_header_verify() {
        let mut rom = Rom::from_bytes(synthetic_rom(false)).expect("valid");
        assert!(!rom.checksum_matches());
        let sum = rom.fix_checksum();
        assert_eq!(rom.header().checksum, sum);
        assert!(rom.header().checksum_consistent());
        assert!(rom.checksum_matches());
        // Idempotent: the checksum bytes contribute 0x1FE whatever they hold.
        assert_eq!(rom.fix_checksum(), sum);
    }

    #[test]
    fn save_round_trips_with_copier_header() {
        let rom = Rom::from_bytes(synthetic_rom(true)).expect("valid");
        let dir = std::env::temp_dir().join(format!("snes-rom-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("out.smc");
        rom.save(&path).expect("saves");
        let written = std::fs::read(&path).expect("reads back");
        assert_eq!(written, synthetic_rom(true));
        let again = Rom::from_bytes(written).expect("valid");
        assert_eq!(again.copier_header(), rom.copier_header());
        assert_eq!(again.bytes(), rom.bytes());
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn rejects_tiny_and_headerless() {
        assert!(matches!(Rom::from_bytes(vec![0; 100]), Err(RomError::TooSmall { len: 100 })));
        assert!(matches!(Rom::from_bytes(vec![0; 0x1_0000]), Err(RomError::NoHeader)));
    }

    #[test]
    fn checksum_of_power_of_two_image() {
        let rom = Rom::from_bytes(synthetic_rom(false)).expect("valid");
        // Compare against a direct sum; the header edit is included in both.
        let direct: u32 = rom.bytes().iter().map(|&b| u32::from(b)).sum();
        assert_eq!(u32::from(rom.compute_checksum()), direct & 0xFFFF);
    }

    /// Runs only when `SMW_ROM` points at a real ROM; verifies the header
    /// facts recorded in docs/formats/rom-mapping.md.
    #[test]
    fn real_rom_if_available() {
        let Some(path) = std::env::var_os("SMW_ROM") else {
            eprintln!("SMW_ROM not set; skipping real-ROM test");
            return;
        };
        let rom = Rom::load(std::path::Path::new(&path)).expect("loads");
        assert_eq!(rom.header().title, "SUPER MARIOWORLD");
        assert_eq!(rom.len(), 512 * 1024);
        assert!(rom.header().checksum_consistent());
        assert!(rom.checksum_matches(), "vanilla checksum should verify");
        // Layer 1 pointer for level 0 is $06:8654 in the USA ROM.
        assert_eq!(
            rom.read_ptr(SnesAddr::lit(0x05_E000)).expect("mapped"),
            SnesAddr::lit(0x06_8654)
        );
    }
}
