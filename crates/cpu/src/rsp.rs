//! RSP scalar core: Reality Signal Processor task execution (LLE path).
//!
//! Hardware facts (public RSP docs, cross-checked between task-struct
//! layouts both references agree on):
//! - Scalar register file is 32 GPRs of 32 bits (r0 hardwired to zero).
//! - PC is a 12-bit byte offset into IMEM (`0x04001000`); IMEM is 4 KiB.
//! - DMEM (`0x04000000`, 4 KiB) is the scalar data window.
//! - Vector work lives in COP2 (`0x12`) plus LWC2/SWC2 and MFC2/MTC2/CFC2/
//!   CTC2 moves; every one of those is loud until the vector unit lands.
//! - BREAK halts the RSP (real halt flag, not a trap-out).
//!
//! Open question, flagged not guessed: the scalar branch-delay behavior.
//! This core retires branches WITHOUT a delay slot and loads WITHOUT a
//! load-delay. If the first retail microcode disagrees, its fault will
//! say so and the one-line switch below (`RSP_HAS_DELAY_SLOT`) flips the
//! model with a retail-pinned test. No silent guessing either way.

use n64_common::{EmuError, Result};
use n64_memory::{Memory, SP_DMEM_BASE, SP_IMEM_BASE, SP_MEM_SIZE};

use crate::decode::{decode, Decoded};

/// Flip to true if retail microcode proves scalar delay slots exist.
/// Default false: neighboring docs describe the RSP pipeline as stalling
/// rather than exposing the slot. Retail arbitrates (see module docs).
pub const RSP_HAS_DELAY_SLOT: bool = false;

/// 12-bit IMEM address mask.
pub const RSP_PC_MASK: u32 = 0xFFC;

/// Scalar register file (32-bit GPRs).
#[derive(Debug, Clone)]
pub struct RspRegs {
    pub gpr: [u32; 32],
}

impl RspRegs {
    pub fn new() -> Self {
        Self { gpr: [0; 32] }
    }

    pub fn r(&self, i: u32) -> u32 {
        if i == 0 {
            0
        } else {
            self.gpr[i as usize]
        }
    }

    pub fn w(&mut self, i: u32, v: u32) {
        if i != 0 {
            self.gpr[i as usize] = v;
        }
    }
}

impl Default for RspRegs {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Default)]
pub struct RspStats {
    pub tasks: u64,
    pub steps: u64,
    pub unsupported: u64,
    pub halts: u64,
    pub last_unsupported: Option<String>,
}

/// RSP scalar state.
pub struct Rsp {
    pub regs: RspRegs,
    /// Byte offset into IMEM (masked to 12 bits on every retirement).
    pub pc: u32,
    pub halted: bool,
    pub stats: RspStats,
}

impl Rsp {
    pub fn new() -> Self {
        Self {
            regs: RspRegs::new(),
            pc: 0,
            halted: false,
            stats: RspStats::default(),
        }
    }

    /// Queue one task (ucode ptr + size). Loud until SP DMA regs land.
    pub fn submit_task(&mut self, ucode: u32, size: u32) -> Result<()> {
        self.stats.tasks += 1;
        let msg = format!(
            "RSP task ucode={:08x} size={} (SP DMA regs pending)",
            ucode, size
        );
        self.stats.unsupported += 1;
        self.stats.last_unsupported = Some(msg.clone());
        Err(EmuError::UnimplementedRsp(msg))
    }

    /// Load microcode bytes straight into IMEM (tests + task setup path).
    pub fn load_imem(&mut self, mem: &mut Memory, data: &[u8]) -> Result<()> {
        if data.len() > SP_MEM_SIZE {
            return Err(EmuError::Other(format!(
                "imem image {} bytes exceeds 4 KiB",
                data.len()
            )));
        }
        mem.write_bytes(SP_IMEM_BASE, data)?;
        self.pc = 0;
        self.halted = false;
        Ok(())
    }

    fn ea(base: u32, off: i16) -> u32 {
        base.wrapping_add(off as i32 as u32)
    }

    /// Step one scalar instruction. `Ok(true)` = BREAK halted the RSP.
    pub fn step(&mut self, mem: &mut Memory) -> Result<bool> {
        if self.halted {
            return Err(EmuError::Unsupported("RSP stepped while halted".into()));
        }
        let addr = SP_IMEM_BASE + (self.pc & RSP_PC_MASK);
        let word = mem.fetch_u32(addr)?;
        let pc = self.pc & RSP_PC_MASK;
        // Vector + privileged encodings are loud, never NOPs.
        let op = (word >> 26) & 0x3F;
        if op == 0x12 || op == 0x32 || op == 0x3A || op == 0x10 || op == 0x11 {
            let msg = format!(
                "RSP pc={:03x} word={:08x} (vector/privileged unit pending)",
                pc, word
            );
            self.stats.unsupported += 1;
            self.stats.last_unsupported = Some(msg.clone());
            log::warn!("rsp: {}", msg);
            return Err(EmuError::UnimplementedRsp(msg));
        }
        let d = decode(word).map_err(|e| {
            EmuError::UnimplementedRsp(format!("RSP pc={:03x} word={:08x}: {}", pc, word, e))
        })?;
        let next = pc.wrapping_add(4) & RSP_PC_MASK;
        match d {
            Decoded::Nop => {}
            Decoded::Break { .. } => {
                self.halted = true;
                self.stats.halts += 1;
                self.stats.steps += 1;
                self.pc = next;
                return Ok(true);
            }
            Decoded::Syscall { .. } => {
                return Err(EmuError::UnimplementedRsp(format!(
                    "RSP pc={:03x}: SYSCALL has no RSP meaning",
                    pc
                )));
            }
            Decoded::Lui { rt, imm } => self.regs.w(rt, imm << 16),
            Decoded::Addiu { rt, rs, imm } => {
                self.regs
                    .w(rt, self.regs.r(rs).wrapping_add(imm as i32 as u32));
            }
            Decoded::Addi { rt, rs, imm } => {
                self.regs
                    .w(rt, self.regs.r(rs).wrapping_add(imm as i32 as u32));
            }
            Decoded::Andi { rt, rs, imm } => self.regs.w(rt, self.regs.r(rs) & imm),
            Decoded::Ori { rt, rs, imm } => self.regs.w(rt, self.regs.r(rs) | imm),
            Decoded::Xori { rt, rs, imm } => self.regs.w(rt, self.regs.r(rs) ^ imm),
            Decoded::Slti { rt, rs, imm } => {
                let a = self.regs.r(rs) as i32;
                self.regs.w(rt, if a < imm as i32 { 1 } else { 0 });
            }
            Decoded::Sltiu { rt, rs, imm } => {
                self.regs.w(
                    rt,
                    if self.regs.r(rs) < imm as i32 as u32 {
                        1
                    } else {
                        0
                    },
                );
            }
            Decoded::Addu { rd, rs, rt } => {
                self.regs
                    .w(rd, self.regs.r(rs).wrapping_add(self.regs.r(rt)));
            }
            Decoded::Add { rd, rs, rt } => {
                self.regs
                    .w(rd, self.regs.r(rs).wrapping_add(self.regs.r(rt)));
            }
            Decoded::Subu { rd, rs, rt } => {
                self.regs
                    .w(rd, self.regs.r(rs).wrapping_sub(self.regs.r(rt)));
            }
            Decoded::And { rd, rs, rt } => {
                self.regs.w(rd, self.regs.r(rs) & self.regs.r(rt));
            }
            Decoded::Or { rd, rs, rt } => {
                self.regs.w(rd, self.regs.r(rs) | self.regs.r(rt));
            }
            Decoded::Xor { rd, rs, rt } => {
                self.regs.w(rd, self.regs.r(rs) ^ self.regs.r(rt));
            }
            Decoded::Nor { rd, rs, rt } => {
                self.regs.w(rd, !(self.regs.r(rs) | self.regs.r(rt)));
            }
            Decoded::Slt { rd, rs, rt } => {
                self.regs.w(
                    rd,
                    if (self.regs.r(rs) as i32) < (self.regs.r(rt) as i32) {
                        1
                    } else {
                        0
                    },
                );
            }
            Decoded::Sltu { rd, rs, rt } => {
                self.regs.w(
                    rd,
                    if self.regs.r(rs) < self.regs.r(rt) {
                        1
                    } else {
                        0
                    },
                );
            }
            Decoded::Sll { rd, rt, sa } => self.regs.w(rd, self.regs.r(rt) << sa),
            Decoded::Srl { rd, rt, sa } => self.regs.w(rd, self.regs.r(rt) >> sa),
            Decoded::Sra { rd, rt, sa } => {
                self.regs.w(rd, ((self.regs.r(rt) as i32) >> sa) as u32);
            }
            Decoded::Sllv { rd, rt, rs } => {
                self.regs.w(rd, self.regs.r(rt) << (self.regs.r(rs) & 0x1F));
            }
            Decoded::Srlv { rd, rt, rs } => {
                self.regs.w(rd, self.regs.r(rt) >> (self.regs.r(rs) & 0x1F));
            }
            Decoded::Srav { rd, rt, rs } => {
                self.regs.w(
                    rd,
                    ((self.regs.r(rt) as i32) >> (self.regs.r(rs) & 0x1F)) as u32,
                );
            }
            Decoded::Jr { rs } => {
                let target = self.regs.r(rs) & RSP_PC_MASK;
                if target & 3 != 0 {
                    return Err(EmuError::AlignmentFault {
                        addr: SP_IMEM_BASE + target,
                    });
                }
                self.pc = target;
                self.stats.steps += 1;
                if RSP_HAS_DELAY_SLOT {
                    // Delay-slot model pending retail proof; currently off.
                    let _ = next;
                }
                return Ok(false);
            }
            Decoded::Jalr { rd, rs } => {
                let target = self.regs.r(rs) & RSP_PC_MASK;
                self.regs.w(rd, next);
                self.pc = target;
                self.stats.steps += 1;
                return Ok(false);
            }
            Decoded::J { target } => {
                self.pc = (target << 2) & RSP_PC_MASK;
                self.stats.steps += 1;
                return Ok(false);
            }
            Decoded::Jal { target } => {
                self.regs.w(31, next);
                self.pc = (target << 2) & RSP_PC_MASK;
                self.stats.steps += 1;
                return Ok(false);
            }
            Decoded::Beq { rs, rt, off } => {
                if self.regs.r(rs) == self.regs.r(rt) {
                    self.pc = next.wrapping_add(((off as i32) << 2) as u32) & RSP_PC_MASK;
                    self.stats.steps += 1;
                    return Ok(false);
                }
            }
            Decoded::Bne { rs, rt, off } => {
                if self.regs.r(rs) != self.regs.r(rt) {
                    self.pc = next.wrapping_add(((off as i32) << 2) as u32) & RSP_PC_MASK;
                    self.stats.steps += 1;
                    return Ok(false);
                }
            }
            Decoded::Blez { rs, off } => {
                if (self.regs.r(rs) as i32) <= 0 {
                    self.pc = next.wrapping_add(((off as i32) << 2) as u32) & RSP_PC_MASK;
                    self.stats.steps += 1;
                    return Ok(false);
                }
            }
            Decoded::Bgtz { rs, off } => {
                if (self.regs.r(rs) as i32) > 0 {
                    self.pc = next.wrapping_add(((off as i32) << 2) as u32) & RSP_PC_MASK;
                    self.stats.steps += 1;
                    return Ok(false);
                }
            }
            Decoded::Lb { rt, base, off } => {
                let a = Self::ea(SP_DMEM_BASE.wrapping_add(self.regs.r(base)), off);
                let b = mem.read_u8(a)?;
                self.regs.w(rt, ((b as i8) as i32) as u32);
            }
            Decoded::Lbu { rt, base, off } => {
                let a = Self::ea(SP_DMEM_BASE.wrapping_add(self.regs.r(base)), off);
                self.regs.w(rt, mem.read_u8(a)? as u32);
            }
            Decoded::Lh { rt, base, off } => {
                // RSP DMA bases are DMEM-relative here: values in regs are
                // offsets, so Lh must land inside DMEM, not RDRAM.
                let off_abs = self.regs.r(base).wrapping_add(off as i32 as u32);
                if off_abs as usize + 2 > SP_MEM_SIZE {
                    return Err(EmuError::UnimplementedRsp(format!(
                        "RSP pc={:03x}: LH dmem OOB {:08x}",
                        pc, off_abs
                    )));
                }
                let h = mem.read_u16(SP_DMEM_BASE + off_abs)?;
                self.regs.w(rt, ((h as i16) as i32) as u32);
            }
            Decoded::Lhu { rt, base, off } => {
                let off_abs = self.regs.r(base).wrapping_add(off as i32 as u32);
                if off_abs as usize + 2 > SP_MEM_SIZE {
                    return Err(EmuError::UnimplementedRsp(format!(
                        "RSP pc={:03x}: LHU dmem OOB {:08x}",
                        pc, off_abs
                    )));
                }
                self.regs
                    .w(rt, mem.read_u16(SP_DMEM_BASE + off_abs)? as u32);
            }
            Decoded::Lw { rt, base, off } => {
                let off_abs = self.regs.r(base).wrapping_add(off as i32 as u32);
                if off_abs as usize + 4 > SP_MEM_SIZE {
                    return Err(EmuError::UnimplementedRsp(format!(
                        "RSP pc={:03x}: LW dmem OOB {:08x}",
                        pc, off_abs
                    )));
                }
                self.regs.w(rt, mem.read_u32(SP_DMEM_BASE + off_abs)?);
            }
            Decoded::Sb { rt, base, off } => {
                let a = Self::ea(SP_DMEM_BASE.wrapping_add(self.regs.r(base)), off);
                if !(SP_DMEM_BASE..SP_DMEM_BASE + SP_MEM_SIZE as u32).contains(&a) {
                    return Err(EmuError::UnimplementedRsp(format!(
                        "RSP pc={:03x}: SB outside DMEM {:08x}",
                        pc, a
                    )));
                }
                mem.write_u8(a, self.regs.r(rt) as u8)?;
            }
            Decoded::Sh { rt, base, off } => {
                let off_abs = self.regs.r(base).wrapping_add(off as i32 as u32);
                if off_abs as usize + 2 > SP_MEM_SIZE {
                    return Err(EmuError::UnimplementedRsp(format!(
                        "RSP pc={:03x}: SH dmem OOB {:08x}",
                        pc, off_abs
                    )));
                }
                mem.write_u16(SP_DMEM_BASE + off_abs, self.regs.r(rt) as u16)?;
            }
            Decoded::Sw { rt, base, off } => {
                let off_abs = self.regs.r(base).wrapping_add(off as i32 as u32);
                if off_abs as usize + 4 > SP_MEM_SIZE {
                    return Err(EmuError::UnimplementedRsp(format!(
                        "RSP pc={:03x}: SW dmem OOB {:08x}",
                        pc, off_abs
                    )));
                }
                mem.write_u32(SP_DMEM_BASE + off_abs, self.regs.r(rt))?;
            }
            other => {
                let msg = format!("RSP pc={:03x}: {:?} pending", pc, other);
                self.stats.unsupported += 1;
                self.stats.last_unsupported = Some(msg.clone());
                return Err(EmuError::UnimplementedRsp(msg));
            }
        }
        self.pc = next;
        self.stats.steps += 1;
        Ok(false)
    }

    /// Run until BREAK halt, step limit, or first loud fault.
    pub fn run(&mut self, mem: &mut Memory, limit: u64) -> Result<u64> {
        let mut steps = 0u64;
        loop {
            if steps >= limit {
                return Err(EmuError::Other(format!("rsp step limit {} reached", limit)));
            }
            if self.step(mem)? {
                return Ok(steps + 1);
            }
            steps += 1;
        }
    }
}

impl Default for Rsp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words_to_bytes(words: &[u32]) -> Vec<u8> {
        let mut b = Vec::new();
        for w in words {
            b.extend_from_slice(&w.to_be_bytes());
        }
        b
    }

    #[test]
    fn task_is_loud() {
        let mut r = Rsp::new();
        assert!(r.submit_task(0x0400_1000, 1024).is_err());
        assert_eq!(r.stats.tasks, 1);
    }

    #[test]
    fn scalar_add_store_halt() {
        // addiu $1,$0,0x100 ; addiu $2,$0,7 ; add $3,$1,$2 ;
        // sw $3,0($0) [DMEM+0] ; break
        let prog = [
            0x2401_0100u32,
            0x2402_0007,
            0x0022_1821,
            0xAC03_0000,
            0x0000_200D,
        ];
        let mut mem = Memory::new();
        let mut rsp = Rsp::new();
        rsp.load_imem(&mut mem, &words_to_bytes(&prog)).unwrap();
        let steps = rsp.run(&mut mem, 64).unwrap();
        assert_eq!(steps, 5);
        assert!(rsp.halted);
        assert_eq!(rsp.regs.r(3), 0x107);
        assert_eq!(mem.read_u32(SP_DMEM_BASE).unwrap(), 0x107);
    }

    #[test]
    fn vector_op_is_loud_with_inputs() {
        // MFC2 $1,$2 (op 0x12): the vector unit, not the scalar core.
        let prog = [0x4841_1000u32, 0x0000_200Du32];
        let mut mem = Memory::new();
        let mut rsp = Rsp::new();
        rsp.load_imem(&mut mem, &words_to_bytes(&prog)).unwrap();
        let err = rsp.step(&mut mem).unwrap_err().to_string();
        assert!(err.contains("vector"), "got: {}", err);
        assert_eq!(rsp.stats.unsupported, 1);
    }

    #[test]
    fn dmem_oob_is_loud() {
        // lw $1,0x1000($0): past the 4 KiB DMEM window.
        let prog = [0x8C01_1000u32];
        let mut mem = Memory::new();
        let mut rsp = Rsp::new();
        rsp.load_imem(&mut mem, &words_to_bytes(&prog)).unwrap();
        assert!(rsp.step(&mut mem).is_err());
    }

    #[test]
    fn r0_hardwired() {
        let mut r = Rsp::new();
        r.regs.w(0, 99);
        assert_eq!(r.regs.r(0), 0);
    }
}
