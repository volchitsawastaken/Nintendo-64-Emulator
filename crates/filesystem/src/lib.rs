//! N64 saves: EEPROM / SRAM / FlashRAM / Controller Pak.
//!
//! Sizes (hardware truth):
//! - EEPROM 4Kbit = 512 B, 16Kbit = 2 KiB
//! - SRAM = 32 KiB (battery)
//! - FlashRAM = 128 KiB (blocks of 128 B)
//! - Controller Pak = 32 KiB (256 pages of 128 B + header)

use n64_common::{EmuError, Result};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveKind {
    Eeprom4k,
    Eeprom16k,
    Sram,
    FlashRam,
    ControllerPak,
}

impl SaveKind {
    pub fn size(&self) -> usize {
        match self {
            SaveKind::Eeprom4k => 512,
            SaveKind::Eeprom16k => 2048,
            SaveKind::Sram => 32 * 1024,
            SaveKind::FlashRam => 128 * 1024,
            SaveKind::ControllerPak => 32 * 1024,
        }
    }

    pub fn from_size(size: usize) -> Result<Self> {
        match size {
            512 => Ok(SaveKind::Eeprom4k),
            2048 => Ok(SaveKind::Eeprom16k),
            32768 => Ok(SaveKind::Sram),
            131072 => Ok(SaveKind::FlashRam),
            _ => Err(EmuError::Other(format!("unknown save size {}", size))),
        }
    }
}

/// One in-memory save image with a backing file.
pub struct SaveMem {
    pub kind: SaveKind,
    pub data: Vec<u8>,
    pub path: PathBuf,
    pub dirty: bool,
}

impl SaveMem {
    pub fn new(kind: SaveKind, path: PathBuf) -> Self {
        Self {
            kind,
            data: vec![0; kind.size()],
            path,
            dirty: false,
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        let kind = SaveKind::from_size(bytes.len())?;
        Ok(Self {
            kind,
            data: bytes,
            path: path.to_path_buf(),
            dirty: false,
        })
    }

    pub fn save(&mut self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&self.path, &self.data)?;
        self.dirty = false;
        Ok(())
    }

    pub fn read(&self, off: usize, len: usize) -> Result<&[u8]> {
        if off + len > self.data.len() {
            return Err(EmuError::Other(format!(
                "save read OOB off={} len={}",
                off, len
            )));
        }
        Ok(&self.data[off..off + len])
    }

    pub fn write(&mut self, off: usize, src: &[u8]) -> Result<()> {
        if off + src.len() > self.data.len() {
            return Err(EmuError::Other(format!(
                "save write OOB off={} len={}",
                off,
                src.len()
            )));
        }
        self.data[off..off + src.len()].copy_from_slice(src);
        self.dirty = true;
        Ok(())
    }
}

/// Identify content by extension. Errors surface per-entry, never guessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Rom,
    Save,
    Unknown,
}

pub fn identify(path: &Path) -> Result<ContentKind> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "z64" | "n64" | "v64" => Ok(ContentKind::Rom),
        "eep" | "sra" | "fla" | "mpk" | "sav" => Ok(ContentKind::Save),
        _ => Ok(ContentKind::Unknown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_roundtrip() {
        let mut s = SaveMem::new(SaveKind::Sram, PathBuf::from("test.sra"));
        s.write(0, &[1, 2, 3]).unwrap();
        assert_eq!(s.read(0, 3).unwrap(), &[1, 2, 3]);
        assert!(s.dirty);
        assert!(s.read(0, 1_000_000).is_err());
    }

    #[test]
    fn identify_kinds() {
        assert_eq!(identify(Path::new("mario.z64")).unwrap(), ContentKind::Rom);
        assert_eq!(identify(Path::new("save.sra")).unwrap(), ContentKind::Save);
        assert_eq!(identify(Path::new("x.txt")).unwrap(), ContentKind::Unknown);
    }
}
