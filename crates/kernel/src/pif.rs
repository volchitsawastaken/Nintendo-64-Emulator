//! PIF boot: LLE reset vector when a PIF ROM is loaded, HLE IPL3 copy
//! otherwise.
//!
//! Hardware facts (public N64 boot docs, second-sourced against cen64 and
//! mupen boot notes — numbers below are the ones both agree on):
//! - CPU reset vector is `0xBFC00000` (KSEG1 mirror of PIF ROM base).
//! - PIF ROM is 2 KiB at physical `0x1FC00000`; PIF RAM is 64 B of
//!   JoyChannel scratch (commands land with the SI/JoyBus work).
//! - IPL2 (in PIF ROM) DMAs the first 4 KiB of cart ROM into RDRAM and
//!   hands off at `0xA0000400` (uncached IPL3 window).
//!
//! Deliberately NOT shipped: the IPL3 register handoff values (s4/cic
//! seed and friends). Two sources disagree on details and my memory is
//! not a source (see playbook rule 4). HLE zeroes GPRs to the documented
//! reset state and marks the handoff pending; retail boot will pin the
//! exact values with a failing test before they are encoded.

use n64_common::{EmuError, Result};
use n64_cpu::ThreadState;
use n64_memory::Memory;

/// IPL3 window: first 4 KiB of cart lands here for both boot paths.
pub const IPL3_SIZE: usize = 0x1000;
/// Uncached RDRAM address IPL3 executes from.
pub const IPL3_ADDR: u32 = 0xA000_0400;
/// CPU reset vector (PIF ROM base, KSEG1).
pub const RESET_VECTOR: u32 = 0xBFC0_0000;
/// PIF RAM size (JoyChannel scratch).
pub const PIF_RAM_SIZE: usize = 64;

/// CIC lockout kind, detected from the IPL3 checksum. Needed for the LLE
/// path; recorded (not emulated) on the HLE path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CicKind {
    NUS6101,
    NUS6102,
    NUS6103,
    NUS6105,
    NUS6106,
}

/// IPL3 checksum -> CIC kind. Checksums are the public boot-code values
/// both reference emulators agree on (sum over cart bytes `0x40..0x1000`).
pub fn detect_cic(cart: &[u8]) -> Result<CicKind> {
    if cart.len() < IPL3_SIZE {
        return Err(EmuError::InvalidRom(format!(
            "cart {} bytes too small for IPL3 checksum",
            cart.len()
        )));
    }
    let (chunks, _) = cart[0x40..IPL3_SIZE].as_chunks::<4>();
    let sum: u32 = chunks
        .iter()
        .map(|c| u32::from_be_bytes(*c))
        .fold(0u32, |a, w| a.wrapping_add(w));
    match sum {
        0xF2E5_2A5E => Ok(CicKind::NUS6101),
        0xC08E_5BD6 => Ok(CicKind::NUS6102),
        0xA536_6A03 => Ok(CicKind::NUS6103),
        0xB02A_F47E => Ok(CicKind::NUS6105),
        0xACC8_5C30 => Ok(CicKind::NUS6106),
        _ => Err(EmuError::Unsupported(format!(
            "unknown IPL3 checksum {:08x} (unrecognized CIC)",
            sum
        ))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMode {
    /// PIF ROM present: CPU starts at the reset vector, IPL2 runs.
    Lle,
    /// No PIF ROM: IPL3 is pre-copied, CPU starts at `0xA0000400`.
    Hle,
}

#[derive(Debug, Clone)]
pub struct BootInfo {
    pub mode: BootMode,
    pub pc: u32,
    pub cic: Option<CicKind>,
}

/// PIF state: 64 B RAM + detected CIC. JoyChannel commands are loud
/// until the SI work lands.
pub struct Pif {
    pub ram: [u8; PIF_RAM_SIZE],
    pub cic: Option<CicKind>,
}

impl Pif {
    pub fn new() -> Self {
        Self {
            ram: [0; PIF_RAM_SIZE],
            cic: None,
        }
    }

    /// JoyBus command passthrough. Loud until SI/JoyBus emulation lands.
    pub fn command(&mut self, _bytes: &[u8]) -> Result<Vec<u8>> {
        Err(EmuError::Unsupported(
            "PIF JoyBus commands pending (SI work)".into(),
        ))
    }
}

impl Default for Pif {
    fn default() -> Self {
        Self::new()
    }
}

/// Boot a parsed ROM: load the cart, pick LLE vs HLE, place IPL3, set the
/// CPU to the handoff state. Returns how the CPU was started.
pub fn boot(
    mem: &mut Memory,
    cpu: &mut ThreadState,
    rom_data: &[u8],
    expansion_pak: bool,
) -> Result<BootInfo> {
    mem.set_expansion_pak(expansion_pak);
    mem.load_cart(rom_data.to_vec())
        .map_err(|e| EmuError::InvalidRom(format!("cart rejected by memory map: {}", e)))?;
    let cic = detect_cic(rom_data).ok();
    if mem.has_pif_rom() {
        // LLE: reset state, IPL2 runs from the real ROM image.
        *cpu = ThreadState::new(cpu.id, RESET_VECTOR);
        cpu.cp0_status = 0x3400_0000 | (1 << 22) | (1 << 2) | (1 << 1);
        return Ok(BootInfo {
            mode: BootMode::Lle,
            pc: RESET_VECTOR,
            cic,
        });
    }
    // HLE: pre-copy IPL3 to the uncached window, start the CPU there.
    if rom_data.len() < IPL3_SIZE {
        return Err(EmuError::InvalidRom(format!(
            "cart {} bytes too small for HLE IPL3 copy",
            rom_data.len()
        )));
    }
    mem.write_bytes(IPL3_ADDR, &rom_data[..IPL3_SIZE])?;
    *cpu = ThreadState::new(cpu.id, IPL3_ADDR);
    // Documented reset state: GPRs zero, COP0 Status with EXL|ERL set,
    // Compare zeroed. IPL3-derived handoff values stay zero until retail
    // pins them (see module docs).
    cpu.cp0_status = (1 << 2) | (1 << 1);
    log::info!(
        "pif: HLE boot, IPL3 copied to {:08x}, cic={:?} (recorded only)",
        IPL3_ADDR,
        cic
    );
    Ok(BootInfo {
        mode: BootMode::Hle,
        pc: IPL3_ADDR,
        cic,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use n64_cpu::Interpreter;

    fn synth_cart() -> Vec<u8> {
        let mut cart = vec![0u8; IPL3_SIZE];
        cart[0..4].copy_from_slice(&0x8037_1240u32.to_be_bytes());
        cart[8..12].copy_from_slice(&0x8000_0400u32.to_be_bytes());
        // Fill IPL3 body with NOPs so the copy is executable.
        for w in cart[0x40..].chunks_exact_mut(4) {
            w.copy_from_slice(&0u32.to_be_bytes());
        }
        cart
    }

    #[test]
    fn hle_boot_copies_ipl3_and_sets_pc() {
        let mut mem = Memory::new();
        let cart = synth_cart();
        let mut cpu = ThreadState::new(0, 0);
        let info = boot(&mut mem, &mut cpu, &cart, false).unwrap();
        assert_eq!(info.mode, BootMode::Hle);
        assert_eq!(info.pc, IPL3_ADDR);
        assert_eq!(cpu.pc, IPL3_ADDR);
        assert_eq!(mem.read_u32(IPL3_ADDR).unwrap(), 0x8037_1240);
        // Reset state: GPRs zero, status carries EXL|ERL.
        assert_eq!(cpu.gpr[4], 0);
        assert_ne!(cpu.cp0_status & 0x6, 0);
        // KSEG1 cart mirror reads the same header.
        assert_eq!(mem.read_u32(n64_memory::CART_KSEG1).unwrap(), 0x8037_1240);
    }

    #[test]
    fn hle_boot_rejects_tiny_cart() {
        let mut mem = Memory::new();
        let mut cpu = ThreadState::new(0, 0);
        assert!(boot(&mut mem, &mut cpu, &[0u8; 64], false).is_err());
    }

    #[test]
    fn lle_boot_needs_pif_rom() {
        // Without a PIF ROM the LLE vector must fault, never silently HLE.
        let mut mem = Memory::new();
        assert!(mem.fetch_u32(RESET_VECTOR).is_err());
        // With one, boot starts at the reset vector.
        mem.load_pif_rom(vec![0u8; n64_memory::PIF_ROM_SIZE])
            .unwrap();
        let cart = synth_cart();
        let mut cpu = ThreadState::new(0, 0);
        let info = boot(&mut mem, &mut cpu, &cart, false).unwrap();
        assert_eq!(info.mode, BootMode::Lle);
        assert_eq!(info.pc, RESET_VECTOR);
        assert_eq!(cpu.pc, RESET_VECTOR);
    }

    #[test]
    fn unknown_cic_is_loud() {
        let cart = synth_cart();
        // Synthetic NOP-filled IPL3 sums to the header words only, which
        // matches no known CIC: detection must fail loudly, not guess.
        assert!(detect_cic(&cart).is_err());
    }

    #[test]
    fn pif_commands_are_loud() {
        let mut p = Pif::new();
        assert!(p.command(&[0x00]).is_err());
        let _ = Interpreter::new(0, 0);
    }
}
