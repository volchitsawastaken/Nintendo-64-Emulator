//! Debugger backed by real emulator state.

use n64_common::Result;
use n64_cpu::{decode::disasm, regs::ThreadState};
use n64_memory::Memory;
use std::collections::{HashSet, VecDeque};

pub const DEFAULT_TRACE_CAPACITY: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEvent {
    pub pc: u32,
    pub word: u32,
    pub disasm: String,
}

#[derive(Debug, Clone)]
pub struct TraceBuffer {
    events: VecDeque<TraceEvent>,
    capacity: usize,
}

impl TraceBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn push(&mut self, pc: u32, word: u32) {
        if self.capacity == 0 {
            return;
        }
        if self.events.len() >= self.capacity {
            self.events.pop_front();
        }
        let disasm = disasm(word);
        self.events.push_back(TraceEvent { pc, word, disasm });
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, TraceEvent> {
        self.events.iter()
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }
}

impl Default for TraceBuffer {
    fn default() -> Self {
        Self::new(DEFAULT_TRACE_CAPACITY)
    }
}

pub struct Debugger {
    breakpoints: HashSet<u32>,
    pub trace: TraceBuffer,
}

impl Debugger {
    pub fn new() -> Self {
        Self {
            breakpoints: HashSet::new(),
            trace: TraceBuffer::default(),
        }
    }

    pub fn add_breakpoint(&mut self, addr: u32) {
        self.breakpoints.insert(addr & !3);
    }
    pub fn remove_breakpoint(&mut self, addr: u32) {
        self.breakpoints.remove(&(addr & !3));
    }
    pub fn is_breakpoint(&self, addr: u32) -> bool {
        self.breakpoints.contains(&(addr & !3))
    }

    pub fn disasm_at(&self, mem: &Memory, addr: u32, count: usize) -> Vec<String> {
        let mut out = Vec::new();
        for i in 0..count {
            let a = addr.wrapping_add(i as u32 * 4);
            match mem.read_u32(a) {
                Ok(w) => out.push(format!("{:08x}: {}", a, disasm(w))),
                Err(e) => out.push(format!("{:08x}: <fault {}>", a, e)),
            }
        }
        out
    }

    pub fn regs(&self, t: &ThreadState) -> Vec<String> {
        let mut v = Vec::new();
        for i in 0..32 {
            v.push(format!("r{:02}={:016x}", i, t.gpr[i]));
        }
        v.push(format!(
            "pc={:08x} hi={:016x} lo={:016x} status={:08x} cause={:08x}",
            t.pc, t.hi, t.lo, t.cp0_status, t.cp0_cause
        ));
        v
    }

    pub fn dump_mem(&self, mem: &Memory, addr: u32, len: usize) -> Result<Vec<u8>> {
        mem.read_bytes(addr, len)
    }
}

impl Default for Debugger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn breakpoints_and_disasm() {
        let mut dbg = Debugger::new();
        dbg.add_breakpoint(0x1001);
        assert!(dbg.is_breakpoint(0x1000));
        let mem = Memory::new();
        let lines = dbg.disasm_at(&mem, 0x1000, 2);
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn trace_respects_capacity() {
        let mut buf = TraceBuffer::new(4);
        for i in 0..10u32 {
            buf.push(0x8000_0000 + i * 4, 0x0000_0000);
        }
        assert_eq!(buf.len(), 4);
    }

    #[test]
    fn trace_push_records_real_disasm() {
        let mut buf = TraceBuffer::new(8);
        buf.push(0x8000_0000, 0x0000_0000);
        let ev = buf.iter().next().expect("one event");
        assert!(ev.disasm.contains("Nop") || ev.disasm.contains("nop"));
    }
}
