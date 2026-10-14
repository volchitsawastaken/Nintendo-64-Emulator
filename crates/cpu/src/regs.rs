//! VR4300 thread register file (MIPS III, big-endian).
//!
//! GPRs are 64-bit. 32-bit ops sign-extend into the full width per the
//! architecture; the low 32 bits are what most N64 code observes.

/// COP0 Status bits we track (subset).
pub const STATUS_IE: u32 = 1 << 0;
pub const STATUS_EXL: u32 = 1 << 1;
pub const STATUS_ERL: u32 = 1 << 2;

/// COP0 Cause bits (subset).
pub const CAUSE_BD: u32 = 1 << 31;

/// One CPU thread.
#[derive(Debug, Clone)]
pub struct ThreadState {
    pub gpr: [u64; 32],
    pub fpr: [f64; 32],
    pub fcr0: u32,
    pub fcr31: u32,
    pub pc: u32,
    /// Delay-slot bookkeeping: address the branch will land on, if any.
    pub branch_target: Option<u32>,
    /// True when `pc` is a delay-slot instruction.
    pub in_delay_slot: bool,
    pub hi: u64,
    pub lo: u64,
    /// Load-delay: (reg, value) committed when the delay slot retires.
    pub load_delay: Option<(u32, u64)>,
    pub cp0_status: u32,
    pub cp0_cause: u32,
    pub cp0_epc: u32,
    pub cp0_count: u32,
    pub cp0_compare: u32,
    /// LL/SC reservation.
    pub llbit: bool,
    pub lladdr: u32,
    pub id: usize,
}

impl ThreadState {
    pub fn new(id: usize, entry: u32) -> Self {
        Self {
            gpr: [0; 32],
            fpr: [0.0; 32],
            fcr0: 0,
            fcr31: 0,
            pc: entry,
            branch_target: None,
            in_delay_slot: false,
            hi: 0,
            lo: 0,
            load_delay: None,
            cp0_status: 0,
            cp0_cause: 0,
            cp0_epc: 0,
            cp0_count: 0,
            cp0_compare: 0,
            llbit: false,
            lladdr: 0,
            id,
        }
    }

    /// Read GPR; r0 is hardwired to zero.
    pub fn r(&self, i: u32) -> u64 {
        if i == 0 {
            0
        } else {
            self.gpr[i as usize]
        }
    }

    /// Write GPR; writes to r0 are discarded.
    pub fn w(&mut self, i: u32, v: u64) {
        if i != 0 {
            self.gpr[i as usize] = v;
        }
    }

    /// 32-bit sign-extended write (ADDIU/LW family semantics).
    pub fn w32(&mut self, i: u32, v: u32) {
        self.w(i, (v as i32) as i64 as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r0_hardwired() {
        let mut t = ThreadState::new(0, 0x8000_1000);
        t.w(0, 99);
        assert_eq!(t.r(0), 0);
        t.w32(1, 0xFFFF_FFFF);
        assert_eq!(t.r(1), 0xFFFF_FFFF_FFFF_FFFF);
    }
}
