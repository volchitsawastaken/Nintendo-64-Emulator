//! VR4300 execution: one `Decoded` + raw word -> state + memory.
//!
//! Delay slots are real: branches/J/JR/JAL schedule `branch_target` and
//! the next instruction executes before the jump lands. Load-delay is
//! modeled for LW/LB/LH (value commits after the delay slot).

use n64_common::{EmuError, Result};
use n64_memory::Memory;

use crate::decode::{self, Decoded};
use crate::regs::ThreadState;

/// Trap out of normal flow (BREAK / SYSCALL).
#[derive(Debug, Clone, Copy)]
pub struct Trap {
    pub kind: TrapKind,
    pub pc: u32,
    pub code: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrapKind {
    Break,
    Syscall,
}

fn ea(base: u32, off: i16) -> u32 {
    base.wrapping_add(off as i32 as u32)
}

fn commit_load(t: &mut ThreadState) {
    if let Some((reg, val)) = t.load_delay.take() {
        t.w(reg, val);
    }
}

fn schedule_branch(t: &mut ThreadState, target: u32) {
    t.branch_target = Some(target);
}

/// Execute one decoded instruction. Returns a trap for BREAK/SYSCALL,
/// otherwise advances `pc` (landing pending branches after the delay slot).
pub fn execute(
    d: Decoded,
    word: u32,
    t: &mut ThreadState,
    mem: &mut Memory,
) -> Result<Option<Trap>> {
    let pc = t.pc;
    let was_delay_slot = t.in_delay_slot;
    // Entering this instruction clears the flag; branches set it for next.
    t.in_delay_slot = false;

    let trap: Option<Trap> = match d {
        Decoded::Nop => None,
        Decoded::Lui { rt, imm } => {
            commit_load(t);
            t.w32(rt, imm << 16);
            None
        }
        Decoded::Addiu { rt, rs, imm } => {
            commit_load(t);
            let a = t.r(rs) as u32;
            let v = a.wrapping_add(imm as i32 as u32);
            t.w32(rt, v);
            None
        }
        Decoded::Addi { rt, rs, imm } => {
            // M1: no integer-overflow trap yet; flag as future work in docs.
            // Behaves as ADDIU for now but stays a distinct decode.
            commit_load(t);
            let a = t.r(rs) as u32;
            let v = a.wrapping_add(imm as i32 as u32);
            t.w32(rt, v);
            None
        }
        Decoded::Andi { rt, rs, imm } => {
            commit_load(t);
            t.w32(rt, (t.r(rs) as u32) & imm);
            None
        }
        Decoded::Ori { rt, rs, imm } => {
            commit_load(t);
            t.w32(rt, (t.r(rs) as u32) | imm);
            None
        }
        Decoded::Xori { rt, rs, imm } => {
            commit_load(t);
            t.w32(rt, (t.r(rs) as u32) ^ imm);
            None
        }
        Decoded::Slti { rt, rs, imm } => {
            commit_load(t);
            let a = t.r(rs) as i64;
            let b = imm as i64;
            t.w32(rt, if a < b { 1 } else { 0 });
            None
        }
        Decoded::Sltiu { rt, rs, imm } => {
            commit_load(t);
            // Canonical: compare low-32 unsigned after sign-extension of imm.
            let av = t.r(rs) as u32;
            let bv = imm as i32 as u32;
            t.w32(rt, if av < bv { 1 } else { 0 });
            None
        }
        Decoded::Addu { rd, rs, rt } => {
            commit_load(t);
            let v = (t.r(rs) as u32).wrapping_add(t.r(rt) as u32);
            t.w32(rd, v);
            None
        }
        Decoded::Add { rd, rs, rt } => {
            commit_load(t);
            let v = (t.r(rs) as u32).wrapping_add(t.r(rt) as u32);
            t.w32(rd, v);
            None
        }
        Decoded::Subu { rd, rs, rt } => {
            commit_load(t);
            let v = (t.r(rs) as u32).wrapping_sub(t.r(rt) as u32);
            t.w32(rd, v);
            None
        }
        Decoded::And { rd, rs, rt } => {
            commit_load(t);
            t.w32(rd, (t.r(rs) as u32) & (t.r(rt) as u32));
            None
        }
        Decoded::Or { rd, rs, rt } => {
            commit_load(t);
            t.w32(rd, (t.r(rs) as u32) | (t.r(rt) as u32));
            None
        }
        Decoded::Xor { rd, rs, rt } => {
            commit_load(t);
            t.w32(rd, (t.r(rs) as u32) ^ (t.r(rt) as u32));
            None
        }
        Decoded::Nor { rd, rs, rt } => {
            commit_load(t);
            t.w32(rd, !((t.r(rs) as u32) | (t.r(rt) as u32)));
            None
        }
        Decoded::Slt { rd, rs, rt } => {
            commit_load(t);
            let a = t.r(rs) as i64;
            let b = t.r(rt) as i64;
            t.w32(rd, if a < b { 1 } else { 0 });
            None
        }
        Decoded::Sltu { rd, rs, rt } => {
            commit_load(t);
            t.w32(rd, if t.r(rs) < t.r(rt) { 1 } else { 0 });
            None
        }
        Decoded::Sll { rd, rt, sa } => {
            commit_load(t);
            t.w32(rd, (t.r(rt) as u32) << sa);
            None
        }
        Decoded::Srl { rd, rt, sa } => {
            commit_load(t);
            t.w32(rd, (t.r(rt) as u32) >> sa);
            None
        }
        Decoded::Sra { rd, rt, sa } => {
            commit_load(t);
            t.w32(rd, ((t.r(rt) as u32 as i32) >> sa) as u32);
            None
        }
        Decoded::Sllv { rd, rt, rs } => {
            commit_load(t);
            let sh = (t.r(rs) as u32) & 0x1F;
            t.w32(rd, (t.r(rt) as u32) << sh);
            None
        }
        Decoded::Srlv { rd, rt, rs } => {
            commit_load(t);
            let sh = (t.r(rs) as u32) & 0x1F;
            t.w32(rd, (t.r(rt) as u32) >> sh);
            None
        }
        Decoded::Srav { rd, rt, rs } => {
            commit_load(t);
            let sh = (t.r(rs) as u32) & 0x1F;
            t.w32(rd, ((t.r(rt) as u32 as i32) >> sh) as u32);
            None
        }
        Decoded::Mult { rs, rt } => {
            commit_load(t);
            let a = t.r(rs) as i64;
            let b = t.r(rt) as i64;
            let p = (a as i128) * (b as i128);
            t.lo = p as u64;
            t.hi = ((p >> 64) & 0xFFFF_FFFF_FFFF_FFFF) as u64;
            // HI/LO low-32 convention for 32-bit mult.
            let p32 = (a as i32 as i64) * (b as i32 as i64);
            t.lo = p32 as u64;
            t.hi = ((p32 >> 32) & 0xFFFF_FFFF) as u64;
            None
        }
        Decoded::Multu { rs, rt } => {
            commit_load(t);
            let p = (t.r(rs) as u32 as u64) * (t.r(rt) as u32 as u64);
            t.hi = (p >> 32) & 0xFFFF_FFFF;
            // Sign-extend low word like hardware observably does.
            t.lo = ((p & 0xFFFF_FFFF) as u32 as i32) as i64 as u64;
            None
        }
        Decoded::Div { rs, rt } => {
            commit_load(t);
            let a = t.r(rs) as u32 as i32;
            let b = t.r(rt) as u32 as i32;
            if b == 0 {
                return Err(EmuError::Unsupported(format!(
                    "DIV by zero pc={:08x} word={:08x}",
                    pc, word
                )));
            }
            let q = a.wrapping_div(b);
            let r = a.wrapping_rem(b);
            t.lo = (q as i64) as u64;
            t.hi = (r as i64) as u64;
            None
        }
        Decoded::Divu { rs, rt } => {
            commit_load(t);
            let a = t.r(rs) as u32;
            let b = t.r(rt) as u32;
            if b == 0 {
                return Err(EmuError::Unsupported(format!(
                    "DIVU by zero pc={:08x} word={:08x}",
                    pc, word
                )));
            }
            t.lo = ((a / b) as i32 as i64) as u64;
            t.hi = ((a % b) as i32 as i64) as u64;
            None
        }
        Decoded::Mfhi { rd } => {
            commit_load(t);
            t.w(rd, t.hi);
            None
        }
        Decoded::Mflo { rd } => {
            commit_load(t);
            t.w(rd, t.lo);
            None
        }
        Decoded::Jr { rs } => {
            commit_load(t);
            let target = t.r(rs) as u32;
            if target & 3 != 0 {
                return Err(EmuError::AlignmentFault { addr: target });
            }
            schedule_branch(t, target);
            t.in_delay_slot = true;
            None
        }
        Decoded::Jalr { rd, rs } => {
            commit_load(t);
            let target = t.r(rs) as u32;
            if target & 3 != 0 {
                return Err(EmuError::AlignmentFault { addr: target });
            }
            t.w32(rd, pc.wrapping_add(8));
            schedule_branch(t, target);
            t.in_delay_slot = true;
            None
        }
        Decoded::Lw { rt, base, off } => {
            let addr = ea(t.r(base) as u32, off);
            let v = mem.read_u32(addr)?;
            // Load-delay: visible after the next instruction.
            t.load_delay = Some((rt, ((v as i32) as i64) as u64));
            None
        }
        Decoded::Sw { rt, base, off } => {
            commit_load(t);
            let addr = ea(t.r(base) as u32, off);
            mem.write_u32(addr, t.r(rt) as u32)?;
            None
        }
        Decoded::Lb { rt, base, off } => {
            let addr = ea(t.r(base) as u32, off);
            let b = mem.read_u8(addr)?;
            t.load_delay = Some((rt, ((b as i8) as i64) as u64));
            None
        }
        Decoded::Lbu { rt, base, off } => {
            let addr = ea(t.r(base) as u32, off);
            let b = mem.read_u8(addr)?;
            t.load_delay = Some((rt, b as u64));
            None
        }
        Decoded::Lh { rt, base, off } => {
            let addr = ea(t.r(base) as u32, off);
            let h = mem.read_u16(addr)?;
            t.load_delay = Some((rt, ((h as i16) as i64) as u64));
            None
        }
        Decoded::Lhu { rt, base, off } => {
            let addr = ea(t.r(base) as u32, off);
            let h = mem.read_u16(addr)?;
            t.load_delay = Some((rt, h as u64));
            None
        }
        Decoded::Sb { rt, base, off } => {
            commit_load(t);
            let addr = ea(t.r(base) as u32, off);
            mem.write_u8(addr, t.r(rt) as u8)?;
            None
        }
        Decoded::Sh { rt, base, off } => {
            commit_load(t);
            let addr = ea(t.r(base) as u32, off);
            mem.write_u16(addr, t.r(rt) as u16)?;
            None
        }
        Decoded::Lwl { rt, base, off } => {
            // Big-endian unaligned load, left part: bytes [off..4) of the
            // aligned word land in the high bytes of rt. Load-delayed.
            let addr = ea(t.r(base) as u32, off);
            let aligned = addr & !3;
            let n = (addr & 3) as usize;
            let word = mem.read_u32(aligned)?;
            let mb = word.to_be_bytes();
            let mut rb = (t.r(rt) as u32).to_be_bytes();
            rb[..4 - n].copy_from_slice(&mb[n..]);
            let v = u32::from_be_bytes(rb);
            t.load_delay = Some((rt, ((v as i32) as i64) as u64));
            None
        }
        Decoded::Lwr { rt, base, off } => {
            // Big-endian unaligned load, right part: bytes [0..=off] of the
            // aligned word land in the low bytes of rt. Load-delayed.
            let addr = ea(t.r(base) as u32, off);
            let aligned = addr & !3;
            let n = (addr & 3) as usize + 1;
            let word = mem.read_u32(aligned)?;
            let mb = word.to_be_bytes();
            let mut rb = (t.r(rt) as u32).to_be_bytes();
            for i in 0..n {
                rb[4 - n + i] = mb[i];
            }
            let v = u32::from_be_bytes(rb);
            t.load_delay = Some((rt, ((v as i32) as i64) as u64));
            None
        }
        Decoded::Swl { rt, base, off } => {
            commit_load(t);
            let addr = ea(t.r(base) as u32, off);
            let aligned = addr & !3;
            let n = (addr & 3) as usize;
            let word = mem.read_u32(aligned)?;
            let mut mb = word.to_be_bytes();
            let rb = (t.r(rt) as u32).to_be_bytes();
            mb[n..].copy_from_slice(&rb[..4 - n]);
            mem.write_u32(aligned, u32::from_be_bytes(mb))?;
            None
        }
        Decoded::Swr { rt, base, off } => {
            commit_load(t);
            let addr = ea(t.r(base) as u32, off);
            let aligned = addr & !3;
            let n = (addr & 3) as usize + 1;
            let word = mem.read_u32(aligned)?;
            let mut mb = word.to_be_bytes();
            let rb = (t.r(rt) as u32).to_be_bytes();
            for i in 0..n {
                mb[i] = rb[4 - n + i];
            }
            mem.write_u32(aligned, u32::from_be_bytes(mb))?;
            None
        }
        Decoded::Ll { rt, base, off } => {
            let addr = ea(t.r(base) as u32, off);
            let v = mem.read_u32(addr)?;
            t.llbit = true;
            t.lladdr = addr;
            t.load_delay = Some((rt, ((v as i32) as i64) as u64));
            None
        }
        Decoded::Sc { rt, base, off } => {
            commit_load(t);
            // Single-threaded: the reservation always survives unless an
            // intervening store cleared it (none modeled yet beyond SC).
            // SMP invalidation is pending work, noted, not faked.
            let addr = ea(t.r(base) as u32, off);
            if t.llbit {
                mem.write_u32(addr, t.r(rt) as u32)?;
                t.w32(rt, 1);
            } else {
                t.w32(rt, 0);
            }
            t.llbit = false;
            None
        }
        Decoded::Sync => {
            // Memory barrier: no-op for the in-order interpreter.
            commit_load(t);
            None
        }
        Decoded::Beq { rs, rt, off } => {
            commit_load(t);
            if t.r(rs) == t.r(rt) {
                let target = pc.wrapping_add(4).wrapping_add(((off as i32) << 2) as u32);
                schedule_branch(t, target);
            }
            t.in_delay_slot = true;
            // A not-taken branch still executes its delay slot, then falls
            // through; keep the flag so pc advances linearly once.
            if t.branch_target.is_none() {
                t.in_delay_slot = true;
            }
            None
        }
        Decoded::Bne { rs, rt, off } => {
            commit_load(t);
            if t.r(rs) != t.r(rt) {
                let target = pc.wrapping_add(4).wrapping_add(((off as i32) << 2) as u32);
                schedule_branch(t, target);
            }
            t.in_delay_slot = true;
            if t.branch_target.is_none() {
                t.in_delay_slot = true;
            }
            None
        }
        Decoded::Blez { rs, off } => {
            commit_load(t);
            if (t.r(rs) as i64) <= 0 {
                let target = pc.wrapping_add(4).wrapping_add(((off as i32) << 2) as u32);
                schedule_branch(t, target);
            }
            t.in_delay_slot = true;
            if t.branch_target.is_none() {
                t.in_delay_slot = true;
            }
            None
        }
        Decoded::Bgtz { rs, off } => {
            commit_load(t);
            if (t.r(rs) as i64) > 0 {
                let target = pc.wrapping_add(4).wrapping_add(((off as i32) << 2) as u32);
                schedule_branch(t, target);
            }
            t.in_delay_slot = true;
            if t.branch_target.is_none() {
                t.in_delay_slot = true;
            }
            None
        }
        Decoded::J { target } => {
            commit_load(t);
            let dest = (pc & 0xF000_0000) | (target << 2);
            schedule_branch(t, dest);
            t.in_delay_slot = true;
            None
        }
        Decoded::Jal { target } => {
            commit_load(t);
            t.w32(31, pc.wrapping_add(8));
            let dest = (pc & 0xF000_0000) | (target << 2);
            schedule_branch(t, dest);
            t.in_delay_slot = true;
            None
        }
        Decoded::Mfc0 { rt, rd } => {
            commit_load(t);
            let v = match rd {
                12 => t.cp0_status,
                13 => t.cp0_cause,
                14 => t.cp0_epc,
                9 => t.cp0_count,
                11 => t.cp0_compare,
                _ => {
                    return Err(EmuError::Unsupported(format!(
                        "MFC0 rd={} pc={:08x} word={:08x}",
                        rd, pc, word
                    )))
                }
            };
            t.w32(rt, v);
            None
        }
        Decoded::Mtc0 { rt, rd } => {
            commit_load(t);
            let v = t.r(rt) as u32;
            match rd {
                12 => t.cp0_status = v,
                14 => t.cp0_epc = v,
                9 => t.cp0_count = v,
                11 => t.cp0_compare = v,
                _ => {
                    return Err(EmuError::Unsupported(format!(
                        "MTC0 rd={} pc={:08x} word={:08x}",
                        rd, pc, word
                    )))
                }
            }
            None
        }
        Decoded::Break { code } => {
            commit_load(t);
            return Ok(Some(Trap {
                kind: TrapKind::Break,
                pc,
                code,
            }));
        }
        Decoded::Syscall { code } => {
            commit_load(t);
            return Ok(Some(Trap {
                kind: TrapKind::Syscall,
                pc,
                code,
            }));
        }
        Decoded::Cache { .. } => {
            // Cache maintenance is a no-op for the interpreter.
            commit_load(t);
            None
        }
    };

    // Retire pc: if we just executed a delay slot with a pending target,
    // land it; otherwise advance linearly. A branch instruction itself
    // only moves to its delay slot.
    if was_delay_slot {
        if let Some(target) = t.branch_target.take() {
            t.pc = target;
        } else {
            // Not-taken branch delay slot falls through; also commits any
            // pending load.
            commit_load(t);
            t.pc = pc.wrapping_add(4);
        }
    } else if t.in_delay_slot {
        t.pc = pc.wrapping_add(4);
    } else {
        commit_load(t);
        t.pc = pc.wrapping_add(4);
        // No branch pending outside a delay slot.
        t.branch_target = None;
    }
    // Loads scheduled by this instruction stay pending through the delay
    // slot and commit when it retires (handled at the top of next exec,
    // except the fall-through path above which commits immediately).
    let _ = decode::op(word);
    Ok(trap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decode::decode;

    fn step_word(word: u32, t: &mut ThreadState, mem: &mut Memory) {
        let d = decode(word).unwrap();
        execute(d, word, t, mem).unwrap();
    }

    #[test]
    fn addiu_sign_extends() {
        let mut mem = Memory::new();
        let mut t = ThreadState::new(0, 0x8000_0000);
        step_word(0x2401_FFFF, &mut t, &mut mem); // addiu $1,$0,-1
        assert_eq!(t.r(1), 0xFFFF_FFFF_FFFF_FFFF);
    }

    #[test]
    fn branch_takes_delay_slot() {
        let mut mem = Memory::new();
        // beq $0,$0,+2 ; addiu $1,$0,7 (delay) ; addiu $2,$0,9 (skipped) ; nop (target)
        mem.write_u32(0x8000_0000, 0x1000_0002).unwrap();
        mem.write_u32(0x8000_0004, 0x2401_0007).unwrap();
        mem.write_u32(0x8000_0008, 0x2402_0009).unwrap();
        mem.write_u32(0x8000_000C, 0x0000_0000).unwrap();
        let mut t = ThreadState::new(0, 0x8000_0000);
        for _ in 0..3 {
            let w = mem.fetch_u32(t.pc).unwrap();
            let d = decode(w).unwrap();
            execute(d, w, &mut t, &mut mem).unwrap();
        }
        assert_eq!(t.r(1) as u32, 7);
        assert_eq!(t.r(2) as u32, 0);
        assert_eq!(t.pc, 0x8000_0010);
    }

    #[test]
    fn unaligned_store_then_load_roundtrip() {
        // Classic boot-code idiom: swl at addr + swr at addr+3 stores a
        // whole word unaligned; lwl at addr + lwr at addr+3 reassembles it.
        // Base $5=0x2000, value $6=0xAABBCCDD, addr=0x2001.
        let mut mem = Memory::new();
        let mut t = ThreadState::new(0, 0x8000_0000);
        step_word(0x2405_2000, &mut t, &mut mem); // addiu $5,$0,0x2000
        step_word(0x3C06_AABB, &mut t, &mut mem); // lui $6,0xAABB
        step_word(0x34C6_CCDD, &mut t, &mut mem); // ori $6,$6,0xCCDD
        step_word(0xA8A6_0001, &mut t, &mut mem); // swl $6,1($5)
        step_word(0xB8A6_0004, &mut t, &mut mem); // swr $6,4($5)
                                                  // Bytes at 0x2000..0x2005 read [00 AA BB CC][DD 00 00 00].
        assert_eq!(mem.read_u32(0x2000).unwrap(), 0x00AA_BBCC);
        assert_eq!(mem.read_u32(0x2004).unwrap(), 0xDD00_0000);
        // Reassemble with lwl+lwr.
        step_word(0x88A8_0001, &mut t, &mut mem); // lwl $8,1($5)
        step_word(0x98A8_0004, &mut t, &mut mem); // lwr $8,4($5)
        step_word(0x0000_0000, &mut t, &mut mem); // nop (retire)
        assert_eq!(t.r(8) as u32, 0xAABB_CCDD);
    }

    #[test]
    fn ll_sc_succeeds_single_threaded() {
        let mut mem = Memory::new();
        mem.write_u32(0x3000, 5).unwrap();
        let mut t = ThreadState::new(0, 0x8000_0000);
        step_word(0x2405_3000, &mut t, &mut mem); // addiu $5,$0,0x3000
        step_word(0x2406_0009, &mut t, &mut mem); // addiu $6,$0,9
        step_word(0xC0A7_0000, &mut t, &mut mem); // ll $7,0($5)
        step_word(0x0000_0000, &mut t, &mut mem); // nop (retire)
        assert_eq!(t.r(7) as u32, 5);
        step_word(0xE0A6_0000, &mut t, &mut mem); // sc $6,0($5)
        assert_eq!(t.r(6) as u32, 1);
        assert_eq!(mem.read_u32(0x3000).unwrap(), 9);
        // Reservation consumed: a second SC without LL fails loudly-by-value.
        step_word(0x2406_0007, &mut t, &mut mem); // addiu $6,$0,7
        step_word(0xE0A6_0000, &mut t, &mut mem); // sc $6,0($5)
        assert_eq!(t.r(6) as u32, 0);
        assert_eq!(mem.read_u32(0x3000).unwrap(), 9);
    }

    #[test]
    fn sync_is_nop() {
        let mut mem = Memory::new();
        let mut t = ThreadState::new(0, 0x8000_0000);
        step_word(0x0000_000F, &mut t, &mut mem); // sync
        assert_eq!(t.pc, 0x8000_0004);
    }
}
