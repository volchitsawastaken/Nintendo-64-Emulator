//! N64 guest memory: RDRAM + CPU segment mirrors + SP + MMIO guard.
//!
//! Guest map (M1):
//! - `0x00000000..0x00800000` physical RDRAM (8 MB; 4 MB base, 8 MB with pak)
//! - `0x80000000..0x80800000` KSEG0 cached mirror of RDRAM
//! - `0xA0000000..0xA0800000` KSEG1 uncached mirror of RDRAM
//! - `0x04000000..0x04001000` SP DMEM (4 KiB)
//! - `0x04001000..0x04002000` SP IMEM (4 KiB)
//! - `0x03F00000..0x04000000` MMIO guard (always denied, reachable)
//!
//! All mapped RAM is RWX for now; `Perm` is honored for the MMIO guard
//! deny message and will be enforced per-region later. `fetch_u32`
//! exists so EXEC faults are distinguishable in traces.

use n64_common::{EmuError, Result};

/// Physical RDRAM: 8 MiB (covers base + expansion pak).
pub const RDRAM_SIZE: usize = 8 * 1024 * 1024;
/// Base RDRAM without expansion pak: 4 MiB.
pub const RDRAM_BASE_SIZE: usize = 4 * 1024 * 1024;
/// KSEG0 cached mirror base.
pub const KSEG0_BASE: u32 = 0x8000_0000;
/// KSEG1 uncached mirror base.
pub const KSEG1_BASE: u32 = 0xA000_0000;
/// SP DMEM base (4 KiB).
pub const SP_DMEM_BASE: u32 = 0x0400_0000;
/// SP IMEM base (4 KiB).
pub const SP_IMEM_BASE: u32 = 0x0400_1000;
pub const SP_MEM_SIZE: usize = 4096;
/// MMIO guard window (RI/MI/VI/AI/PI/SI/DP, real regs land here later).
pub const MMIO_BASE: u32 = 0x03F0_0000;
pub const MMIO_SIZE: u32 = 0x0010_0000;

bitflags::bitflags! {
    /// Access permissions per region.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Perm: u8 {
        const READ = 0b001;
        const WRITE = 0b010;
        const EXEC = 0b100;
    }
}

/// Guest memory.
pub struct Memory {
    rdram: Vec<u8>,
    dmem: Vec<u8>,
    imem: Vec<u8>,
    /// Usable RDRAM bytes (4 MiB or 8 MiB). Access above this faults.
    usable: usize,
}

impl Memory {
    pub fn new() -> Self {
        Self {
            rdram: vec![0; RDRAM_SIZE],
            dmem: vec![0; SP_MEM_SIZE],
            imem: vec![0; SP_MEM_SIZE],
            usable: RDRAM_BASE_SIZE,
        }
    }

    pub fn with_expansion_pak(expanded: bool) -> Self {
        let mut m = Self::new();
        m.usable = if expanded {
            RDRAM_SIZE
        } else {
            RDRAM_BASE_SIZE
        };
        m
    }

    pub fn set_expansion_pak(&mut self, expanded: bool) {
        self.usable = if expanded {
            RDRAM_SIZE
        } else {
            RDRAM_BASE_SIZE
        };
    }

    pub fn usable_size(&self) -> usize {
        self.usable
    }

    fn translate_rdram(&self, addr: u32, size: usize, need: Perm) -> Result<Option<usize>> {
        let end = (addr as u64)
            .checked_add(size as u64)
            .ok_or_else(|| EmuError::MemoryFault {
                addr,
                size,
                kind: "overflow".into(),
            })?;

        // MMIO guard is reachable and always denied for direct access.
        let mmio_end = MMIO_BASE as u64 + MMIO_SIZE as u64;
        if (addr as u64) < mmio_end && end > MMIO_BASE as u64 {
            return Err(EmuError::MemoryFault {
                addr,
                size,
                kind: format!("permission denied in mmio-guard (need {:?})", need),
            });
        }

        let usable = self.usable as u64;
        if end <= usable {
            return Ok(Some(addr as usize));
        }
        // KSEG0 / KSEG1 mirrors collapse onto RDRAM.
        let k0 = KSEG0_BASE as u64;
        if (addr as u64) >= k0 && end <= k0 + usable {
            return Ok(Some((addr - KSEG0_BASE) as usize));
        }
        let k1 = KSEG1_BASE as u64;
        if (addr as u64) >= k1 && end <= k1 + usable {
            return Ok(Some((addr - KSEG1_BASE) as usize));
        }
        Ok(None)
    }

    fn translate(&self, addr: u32, size: usize, need: Perm) -> Result<MemRef> {
        // SP DMEM / IMEM first (they sit inside the MMIO hole region
        // numerically, so check before the guard... they are below it).
        let end = addr as u64 + size as u64;
        let d0 = SP_DMEM_BASE as u64;
        if (addr as u64) >= d0 && end <= d0 + SP_MEM_SIZE as u64 {
            return Ok(MemRef::Dmem((addr - SP_DMEM_BASE) as usize));
        }
        let i0 = SP_IMEM_BASE as u64;
        if (addr as u64) >= i0 && end <= i0 + SP_MEM_SIZE as u64 {
            return Ok(MemRef::Imem((addr - SP_IMEM_BASE) as usize));
        }
        if let Some(off) = self.translate_rdram(addr, size, need)? {
            return Ok(MemRef::Rdram(off));
        }
        Err(EmuError::MemoryFault {
            addr,
            size,
            kind: format!(
                "out-of-range (rdram {}MB + kseg mirrors + sp)",
                self.usable / (1024 * 1024)
            ),
        })
    }

    fn check_align(addr: u32, align: u32) -> Result<()> {
        if !addr.is_multiple_of(align) {
            return Err(EmuError::AlignmentFault { addr });
        }
        Ok(())
    }

    fn load(&self, r: MemRef, out: &mut [u8]) {
        match r {
            MemRef::Rdram(off) => out.copy_from_slice(&self.rdram[off..off + out.len()]),
            MemRef::Dmem(off) => out.copy_from_slice(&self.dmem[off..off + out.len()]),
            MemRef::Imem(off) => out.copy_from_slice(&self.imem[off..off + out.len()]),
        }
    }

    fn store(&mut self, r: MemRef, data: &[u8]) {
        match r {
            MemRef::Rdram(off) => self.rdram[off..off + data.len()].copy_from_slice(data),
            MemRef::Dmem(off) => self.dmem[off..off + data.len()].copy_from_slice(data),
            MemRef::Imem(off) => self.imem[off..off + data.len()].copy_from_slice(data),
        }
    }

    pub fn read_u8(&self, addr: u32) -> Result<u8> {
        let r = self.translate(addr, 1, Perm::READ)?;
        let mut b = [0u8; 1];
        self.load(r, &mut b);
        Ok(b[0])
    }

    pub fn write_u8(&mut self, addr: u32, v: u8) -> Result<()> {
        let r = self.translate(addr, 1, Perm::WRITE)?;
        self.store(r, &[v]);
        Ok(())
    }

    pub fn read_u16(&self, addr: u32) -> Result<u16> {
        Self::check_align(addr, 2)?;
        let r = self.translate(addr, 2, Perm::READ)?;
        let mut b = [0u8; 2];
        self.load(r, &mut b);
        Ok(u16::from_be_bytes(b))
    }

    pub fn write_u16(&mut self, addr: u32, v: u16) -> Result<()> {
        Self::check_align(addr, 2)?;
        let r = self.translate(addr, 2, Perm::WRITE)?;
        self.store(r, &v.to_be_bytes());
        Ok(())
    }

    pub fn read_u32(&self, addr: u32) -> Result<u32> {
        Self::check_align(addr, 4)?;
        let r = self.translate(addr, 4, Perm::READ)?;
        let mut b = [0u8; 4];
        self.load(r, &mut b);
        Ok(u32::from_be_bytes(b))
    }

    /// Instruction fetch requires EXEC permission.
    pub fn fetch_u32(&self, addr: u32) -> Result<u32> {
        Self::check_align(addr, 4)?;
        let r = self.translate(addr, 4, Perm::EXEC)?;
        let mut b = [0u8; 4];
        self.load(r, &mut b);
        Ok(u32::from_be_bytes(b))
    }

    pub fn write_u32(&mut self, addr: u32, v: u32) -> Result<()> {
        Self::check_align(addr, 4)?;
        let r = self.translate(addr, 4, Perm::WRITE)?;
        self.store(r, &v.to_be_bytes());
        Ok(())
    }

    pub fn read_u64(&self, addr: u32) -> Result<u64> {
        Self::check_align(addr, 8)?;
        let r = self.translate(addr, 8, Perm::READ)?;
        let mut b = [0u8; 8];
        self.load(r, &mut b);
        Ok(u64::from_be_bytes(b))
    }

    pub fn write_u64(&mut self, addr: u32, v: u64) -> Result<()> {
        Self::check_align(addr, 8)?;
        let r = self.translate(addr, 8, Perm::WRITE)?;
        self.store(r, &v.to_be_bytes());
        Ok(())
    }

    pub fn read_bytes(&self, addr: u32, len: usize) -> Result<Vec<u8>> {
        let r = self.translate(addr, len, Perm::READ)?;
        let mut out = vec![0u8; len];
        self.load(r, &mut out);
        Ok(out)
    }

    pub fn write_bytes(&mut self, addr: u32, src: &[u8]) -> Result<()> {
        let r = self.translate(addr, src.len(), Perm::WRITE)?;
        self.store(r, src);
        Ok(())
    }

    /// Physical address for a KSEG address; physical passes through,
    /// anything else maps to None (never a fabricated pointer).
    pub fn to_physical(&self, addr: u32) -> Option<u32> {
        if (addr as usize) < self.usable {
            return Some(addr);
        }
        if addr >= KSEG0_BASE && (addr - KSEG0_BASE) < self.usable as u32 {
            return Some(addr - KSEG0_BASE);
        }
        if addr >= KSEG1_BASE && (addr - KSEG1_BASE) < self.usable as u32 {
            return Some(addr - KSEG1_BASE);
        }
        None
    }
}

#[derive(Debug, Clone, Copy)]
enum MemRef {
    Rdram(usize),
    Dmem(usize),
    Imem(usize),
}

impl Default for Memory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_be() {
        let mut m = Memory::new();
        m.write_u32(0x1000, 0xDEAD_BEEF).unwrap();
        assert_eq!(m.read_u32(0x1000).unwrap(), 0xDEAD_BEEF);
    }

    #[test]
    fn mirrors_alias() {
        let mut m = Memory::new();
        m.write_u32(0x1000, 0x1234_5678).unwrap();
        assert_eq!(m.read_u32(KSEG0_BASE + 0x1000).unwrap(), 0x1234_5678);
        assert_eq!(m.read_u32(KSEG1_BASE + 0x1000).unwrap(), 0x1234_5678);
        assert_eq!(m.fetch_u32(KSEG0_BASE + 0x1000).unwrap(), 0x1234_5678);
    }

    #[test]
    fn sp_memories_work() {
        let mut m = Memory::new();
        m.write_u32(SP_DMEM_BASE + 0x100, 0xAABB_CCDD).unwrap();
        assert_eq!(m.read_u32(SP_DMEM_BASE + 0x100).unwrap(), 0xAABB_CCDD);
        m.write_u32(SP_IMEM_BASE + 0x200, 0x1122_3344).unwrap();
        assert_eq!(m.fetch_u32(SP_IMEM_BASE + 0x200).unwrap(), 0x1122_3344);
    }

    #[test]
    fn expansion_pak_grows_usable() {
        let m = Memory::with_expansion_pak(false);
        assert!(m.read_u32(0x0060_0000).is_err());
        let mut m2 = Memory::with_expansion_pak(true);
        m2.write_u32(0x0060_0000, 1).unwrap();
        assert_eq!(m2.read_u32(0x0060_0000).unwrap(), 1);
    }

    #[test]
    fn oob_is_error_not_wrap() {
        let m = Memory::new();
        assert!(m.read_u32(0xFFFF_FFFC).is_err());
    }

    #[test]
    fn misaligned_is_error() {
        let m = Memory::new();
        assert!(m.read_u32(0x1001).is_err());
    }

    #[test]
    fn mmio_guard_denies() {
        let m = Memory::new();
        assert!(m.read_u32(MMIO_BASE).is_err());
        let mut m2 = Memory::new();
        assert!(m2.write_u8(MMIO_BASE, 1).is_err());
    }
}
