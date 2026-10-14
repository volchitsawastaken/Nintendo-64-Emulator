//! N64 ROM loading: format normalize + header parse, never claims boot.
//!
//! Facts (big-endian on cart):
//! - `.z64` native BE, magic bytes `80 37 12 40` at +0
//! - `.n64` byte-swapped, magic `37 80 40 12`
//! - `.v64` half-swapped, magic `40 12 37 80`
//! - Header (64 bytes): PI settings +0x00, clock +0x04, entry +0x08,
//!   release +0x0C, CRC1 +0x10, CRC2 +0x14, name +0x20 (20 bytes ASCII).
//!   Entry is a MIPS address (usually 0x8000_0400); boot copies the first
//!   4 KiB through the PIF.

use n64_common::{EmuError, Result};
use n64_memory::Memory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RomFormat {
    Z64,
    N64,
    V64,
}

#[derive(Debug, Clone)]
pub struct RomHeader {
    pub pi_settings: u32,
    pub clock_rate: u32,
    pub entry: u32,
    pub release: u32,
    pub crc1: u32,
    pub crc2: u32,
    pub name: String,
    pub format: RomFormat,
}

#[derive(Debug, Clone)]
pub struct Rom {
    /// Normalized big-endian image bytes.
    pub data: Vec<u8>,
    pub header: RomHeader,
}

fn read_be32(b: &[u8], off: usize) -> Result<u32> {
    if off + 4 > b.len() {
        return Err(EmuError::InvalidRom("header OOB".into()));
    }
    Ok(u32::from_be_bytes([
        b[off],
        b[off + 1],
        b[off + 2],
        b[off + 3],
    ]))
}

/// Detect container format from the first 4 raw bytes.
pub fn detect_format(raw: &[u8]) -> Result<RomFormat> {
    if raw.len() < 4 {
        return Err(EmuError::InvalidRom("file too small for N64 ROM".into()));
    }
    match [raw[0], raw[1], raw[2], raw[3]] {
        [0x80, 0x37, 0x12, 0x40] => Ok(RomFormat::Z64),
        [0x37, 0x80, 0x40, 0x12] => Ok(RomFormat::N64),
        [0x40, 0x12, 0x37, 0x80] => Ok(RomFormat::V64),
        b => Err(EmuError::InvalidRom(format!(
            "bad magic {:02X}{:02X}{:02X}{:02X}, expected z64/n64/v64",
            b[0], b[1], b[2], b[3]
        ))),
    }
}

/// Normalize any container to big-endian bytes.
pub fn normalize(raw: &[u8]) -> Result<Vec<u8>> {
    let fmt = detect_format(raw)?;
    let mut out = raw.to_vec();
    match fmt {
        RomFormat::Z64 => {}
        RomFormat::N64 => {
            let (chunks, _) = out.as_chunks_mut::<2>();
            for c in chunks {
                c.swap(0, 1);
            }
        }
        RomFormat::V64 => {
            // v64 stores each 4-byte word byte-reversed vs z64.
            let (chunks, _) = out.as_chunks_mut::<4>();
            for c in chunks {
                c.reverse();
            }
        }
    }
    Ok(out)
}

/// Parse a raw ROM file (any container) into header + BE image.
pub fn parse_rom(raw: &[u8]) -> Result<Rom> {
    if raw.len() < 64 {
        return Err(EmuError::InvalidRom("file too small for header".into()));
    }
    let format = detect_format(raw)?;
    let data = normalize(raw)?;
    let pi_settings = read_be32(&data, 0x00)?;
    let clock_rate = read_be32(&data, 0x04)?;
    let entry = read_be32(&data, 0x08)?;
    let release = read_be32(&data, 0x0C)?;
    let crc1 = read_be32(&data, 0x10)?;
    let crc2 = read_be32(&data, 0x14)?;
    let raw_name = &data[0x20..0x34];
    let end = raw_name.iter().position(|&b| b == 0).unwrap_or(20);
    let name = String::from_utf8_lossy(&raw_name[..end]).trim().to_string();
    if entry & 3 != 0 {
        return Err(EmuError::InvalidRom(format!(
            "entry {:08x} misaligned",
            entry
        )));
    }
    Ok(Rom {
        data,
        header: RomHeader {
            pi_settings,
            clock_rate,
            entry,
            release,
            crc1,
            crc2,
            name,
            format,
        },
    })
}

/// Place ROM bytes at guest address. Used by tests and boot path.
pub fn load_bytes_to_guest(mem: &mut Memory, addr: u32, data: &[u8]) -> Result<()> {
    mem.write_bytes(addr, data)?;
    log::info!("loader: placed {} bytes at {:08x}", data.len(), addr);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_bytes(fmt: RomFormat) -> Vec<u8> {
        // BE header with entry 0x80000400, name "TESTROM".
        let mut be = vec![0u8; 64];
        be[0..4].copy_from_slice(&0x8037_1240u32.to_be_bytes());
        be[4..8].copy_from_slice(&0x0000_000Fu32.to_be_bytes());
        be[8..12].copy_from_slice(&0x8000_0400u32.to_be_bytes());
        be[12..16].copy_from_slice(&0x0000_0000u32.to_be_bytes());
        be[16..20].copy_from_slice(&0x1234_5678u32.to_be_bytes());
        be[20..24].copy_from_slice(&0x9ABC_DEF0u32.to_be_bytes());
        be[0x20..0x27].copy_from_slice(b"TESTROM");
        let mut raw = be.clone();
        match fmt {
            RomFormat::Z64 => {}
            RomFormat::N64 => {
                for c in raw.chunks_exact_mut(2) {
                    c.swap(0, 1);
                }
            }
            RomFormat::V64 => {
                for c in raw.chunks_exact_mut(4) {
                    c.reverse();
                }
            }
        }
        raw
    }

    #[test]
    fn all_containers_parse() {
        for fmt in [RomFormat::Z64, RomFormat::N64, RomFormat::V64] {
            let raw = header_bytes(fmt);
            let rom = parse_rom(&raw).unwrap();
            assert_eq!(rom.header.entry, 0x8000_0400);
            assert_eq!(rom.header.name, "TESTROM");
            assert_eq!(rom.header.format, fmt);
            assert_eq!(&rom.data[0..4], &[0x80, 0x37, 0x12, 0x40]);
        }
    }

    #[test]
    fn bad_magic_rejected() {
        let raw = vec![0u8; 64];
        assert!(parse_rom(&raw).is_err());
    }

    #[test]
    fn too_small_rejected() {
        assert!(parse_rom(&[0x80, 0x37]).is_err());
    }
}
