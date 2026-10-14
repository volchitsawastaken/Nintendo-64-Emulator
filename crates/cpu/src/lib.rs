//! VR4300 interpreter core.

pub mod decode;
pub mod exec;
pub mod regs;
pub mod rsp;

pub use decode::{decode, decode_cop0, disasm, Decoded};
pub use exec::{execute, Trap, TrapKind};
pub use regs::ThreadState;
pub use rsp::Rsp;

use n64_common::{EmuError, Result};
use n64_memory::Memory;

/// Backend trait so a JIT can replace the interpreter later.
pub trait CpuBackend {
    fn step(&mut self, mem: &mut Memory) -> Result<Option<Trap>>;
    fn thread(&self) -> &ThreadState;
    fn thread_mut(&mut self) -> &mut ThreadState;
}

/// Single-threaded interpreter.
pub struct Interpreter {
    pub t: ThreadState,
    pub steps: u64,
    pub unsupported_count: u64,
}

impl Interpreter {
    pub fn new(id: usize, entry: u32) -> Self {
        Self {
            t: ThreadState::new(id, entry),
            steps: 0,
            unsupported_count: 0,
        }
    }

    /// Run up to `quanta` instructions or until trap/error.
    pub fn run_quanta(&mut self, mem: &mut Memory, quanta: usize) -> Result<Option<Trap>> {
        for _ in 0..quanta {
            if let Some(trap) = self.step(mem)? {
                return Ok(Some(trap));
            }
        }
        Ok(None)
    }
}

impl CpuBackend for Interpreter {
    fn step(&mut self, mem: &mut Memory) -> Result<Option<Trap>> {
        let pc = self.t.pc;
        let word = mem.fetch_u32(pc).map_err(|e| match e {
            EmuError::MemoryFault { addr, size, kind } => EmuError::MemoryFault {
                addr,
                size,
                kind: format!("pc-fetch: {}", kind),
            },
            other => other,
        })?;
        // COP0 moves need the secondary decode.
        let d = if (word >> 26) & 0x3F == 0x10 {
            decode_cop0(word)?
        } else {
            decode(word)?
        };
        match execute(d, word, &mut self.t, mem) {
            Ok(trap) => {
                self.steps += 1;
                Ok(trap)
            }
            Err(EmuError::Unsupported(msg)) => {
                self.unsupported_count += 1;
                log::warn!(
                    "cpu{} pc={:08x} word={:08x} regs r1={:016x} r2={:016x} r3={:016x}: {}",
                    self.t.id,
                    pc,
                    word,
                    self.t.gpr[1],
                    self.t.gpr[2],
                    self.t.gpr[3],
                    msg
                );
                Err(EmuError::Unsupported(msg))
            }
            Err(e) => Err(e),
        }
    }

    fn thread(&self) -> &ThreadState {
        &self.t
    }
    fn thread_mut(&mut self) -> &mut ThreadState {
        &mut self.t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpreter_runs_lui_addiu_sw_lw() {
        let mut mem = Memory::new();
        // lui $5,0x8000 noise-free: use low RAM instead.
        // addiu $5,$0,0x4000 ; addiu $3,$0,16 ; sw $3,0($5) ; lw $4,0($5)
        let prog: [u32; 4] = [0x2405_4000, 0x2403_0010, 0xACA3_0000, 0x8CA4_0000];
        for (i, w) in prog.iter().enumerate() {
            mem.write_u32(0x10000 + i as u32 * 4, *w).unwrap();
        }
        let mut cpu = Interpreter::new(0, 0x10000);
        for _ in 0..5 {
            assert!(cpu.step(&mut mem).unwrap().is_none());
        }
        assert_eq!(cpu.t.r(3) as u32, 16);
        assert_eq!(cpu.t.r(4) as u32, 16);
    }

    #[test]
    fn jal_links_and_jumps() {
        let mut mem = Memory::new();
        // jal 0x10008 (target field = addr>>2 & 03ffffff)
        let target = 0x0001_0008u32;
        let jal = 0x0C00_0000 | ((target >> 2) & 0x03FF_FFFF);
        mem.write_u32(0x10000, jal).unwrap();
        mem.write_u32(0x10004, 0x0000_0000).unwrap(); // delay nop
        mem.write_u32(0x10008, 0x2403_0007).unwrap();
        let mut cpu = Interpreter::new(0, 0x10000);
        cpu.step(&mut mem).unwrap();
        cpu.step(&mut mem).unwrap();
        assert_eq!(cpu.t.pc, 0x10008);
        assert_eq!(cpu.t.r(31) as u32, 0x10008);
        cpu.step(&mut mem).unwrap();
        assert_eq!(cpu.t.r(3) as u32, 7);
    }
}
