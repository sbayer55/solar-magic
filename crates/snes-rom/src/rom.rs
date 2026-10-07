//! The ROM image container.

use std::fs;
use std::path::Path;

use crate::{InternalHeader, LoRom, MapKind, Mapper, RomError, SnesAddr, copier_header};

/// File offset of the internal header for LoROM images.
const LOROM_HEADER_PC: usize = 0x7FC0;

/// File offset of the internal header for HiROM images.
const HIROM_HEADER_PC: usize = 0xFFC0;

/// Lowest [`InternalHeader::plausibility`] score accepted as a real header.
///
/// A consistent checksum pair alone scores 4; so does a known map mode plus
/// a sane size code plus a printable title. An all-zero image scores 2.
const MIN_PLAUSIBILITY: u8 = 4;

/// A loaded SNES ROM image with its detected address mapping.
#[derive(Debug)]
pub struct Rom {
    data: Vec<u8>,
    had_copier_header: bool,
    header: InternalHeader,
    mapper: Box<dyn Mapper>,
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
        let had_copier_header = copier_header::detect(data.len());
        if had_copier_header {
            data.drain(..copier_header::LEN);
        }
        if data.len() < 0x8000 {
            return Err(RomError::TooSmall { len: data.len() });
        }
        let (header, mapper) = detect_mapping(&data)?;
        Ok(Self { data, had_copier_header, header, mapper })
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
        self.had_copier_header
    }

    /// The parsed internal header.
    #[must_use]
    pub const fn header(&self) -> &InternalHeader {
        &self.header
    }

    /// The active address mapper.
    #[must_use]
    pub fn mapper(&self) -> &dyn Mapper {
        self.mapper.as_ref()
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

/// A parsed header and the mapper it implies.
type Detected = (InternalHeader, Box<dyn Mapper>);

/// Reads the headers at both candidate locations and picks the most plausible.
fn detect_mapping(data: &[u8]) -> Result<Detected, RomError> {
    let header_at = |pc: usize| {
        data.get(pc..pc.checked_add(InternalHeader::LEN)?).and_then(InternalHeader::parse)
    };
    let lo = header_at(LOROM_HEADER_PC);
    let hi = header_at(HIROM_HEADER_PC);
    let score = |h: &Option<InternalHeader>| h.as_ref().map_or(0, InternalHeader::plausibility);
    let header = if score(&lo) >= score(&hi) { lo } else { hi }.ok_or(RomError::NoHeader)?;
    if header.plausibility() < MIN_PLAUSIBILITY {
        return Err(RomError::NoHeader);
    }
    match header.map_kind() {
        MapKind::LoRom => Ok((header, Box::new(LoRom))),
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
