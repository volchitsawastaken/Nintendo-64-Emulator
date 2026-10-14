//! N64 system: PIF boot, MI interrupts, libultra HLE stubs.
//!
//! The N64 has no kernel syscalls; game code calls libultra routines that
//! touch hardware. HLE intercepts go through this table and every unknown
//! service fails loudly with its exact id.

use n64_common::{EmuError, Result};
use n64_cpu::{CpuBackend, Interpreter};
use n64_memory::Memory;
use std::collections::HashMap;

pub mod boot;

/// One HLE service id (libultra-level).
pub type ServiceId = u32;

/// System state: MI interrupt mask/pending, PIF state, HLE call counts.
pub struct System {
    pub mi_intr_mask: u32,
    pub mi_intr_pending: u32,
    pub boot_done: bool,
    calls: HashMap<ServiceId, u64>,
    pub console: Vec<u8>,
}

impl System {
    pub fn new() -> Self {
        Self {
            mi_intr_mask: 0,
            mi_intr_pending: 0,
            boot_done: false,
            calls: HashMap::new(),
            console: Vec::new(),
        }
    }

    /// Invoke an HLE service. Only service 0 (console putc) and 1 (exit)
    /// are implemented in M1; everything else is an explicit error.
    pub fn service(&mut self, id: ServiceId, args: &[u32]) -> Result<u32> {
        *self.calls.entry(id).or_insert(0) += 1;
        match id {
            0 => {
                let ch = args.first().copied().unwrap_or(b'?' as u32) as u8;
                self.console.push(ch);
                Ok(0)
            }
            1 => Ok(args.first().copied().unwrap_or(0)),
            _ => Err(EmuError::UnimplementedService(id)),
        }
    }

    pub fn call_count(&self, id: ServiceId) -> u64 {
        self.calls.get(&id).copied().unwrap_or(0)
    }

    pub fn raise_mi(&mut self, bit: u32) {
        self.mi_intr_pending |= bit;
    }

    pub fn clear_mi(&mut self, bit: u32) {
        self.mi_intr_pending &= !bit;
    }
}

impl Default for System {
    fn default() -> Self {
        Self::new()
    }
}

/// Run until BREAK/SYSCALL-exit or step limit. Genuine-only: any CPU fault
/// stops with pc + registers + raw word for forensics.
pub struct ExitInfo {
    pub code: u32,
    pub steps: u64,
    pub output: Vec<u8>,
}

pub fn run_until_exit(
    mem: &mut Memory,
    cpu: &mut Interpreter,
    sys: &mut System,
    limit: u64,
) -> Result<ExitInfo> {
    let mut steps = 0u64;
    loop {
        if steps >= limit {
            return Err(EmuError::Other(format!("step limit {} reached", limit)));
        }
        let trap = cpu.step(mem).map_err(|e| {
            let t = cpu.thread();
            EmuError::Other(format!(
                "fault pc={:08x} r1={:016x} r2={:016x} r3={:016x}: {}",
                t.pc, t.gpr[1], t.gpr[2], t.gpr[3], e
            ))
        })?;
        steps += 1;
        if let Some(t) = trap {
            match t.kind {
                n64_cpu::TrapKind::Break => {
                    return Ok(ExitInfo {
                        code: t.code,
                        steps,
                        output: sys.console.clone(),
                    });
                }
                n64_cpu::TrapKind::Syscall => {
                    let code = sys.service(t.code, &[]).map_err(|e| {
                        EmuError::Other(format!("service {} at pc={:08x}: {}", t.code, t.pc, e))
                    })?;
                    if t.code == 1 {
                        return Ok(ExitInfo {
                            code,
                            steps,
                            output: sys.console.clone(),
                        });
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_service_is_loud() {
        let mut s = System::new();
        assert!(s.service(9999, &[]).is_err());
    }

    #[test]
    fn putc_console() {
        let mut s = System::new();
        s.service(0, &[b'H' as u32]).unwrap();
        assert_eq!(s.console, b"H");
    }
}
