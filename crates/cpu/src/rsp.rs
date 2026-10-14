//! RSP stub: Reality Signal Processor task tracking.
//!
//! Full RSP vector emulation lands after the VR4300 integer core is green.
//! Until then every vector op fails loudly with its exact inputs.

use n64_common::{EmuError, Result};

#[derive(Debug, Default)]
pub struct RspStats {
    pub tasks: u64,
    pub unsupported: u64,
    pub last_unsupported: Option<String>,
}

pub struct Rsp {
    pub stats: RspStats,
}

impl Rsp {
    pub fn new() -> Self {
        Self {
            stats: RspStats::default(),
        }
    }

    /// Queue one task (ucode ptr + size). Always loud until vector core lands.
    pub fn submit_task(&mut self, ucode: u32, size: u32) -> Result<()> {
        self.stats.tasks += 1;
        let msg = format!(
            "RSP task ucode={:08x} size={} (vector core pending)",
            ucode, size
        );
        self.stats.unsupported += 1;
        self.stats.last_unsupported = Some(msg.clone());
        Err(EmuError::UnimplementedRsp(msg))
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
    #[test]
    fn task_is_loud() {
        let mut r = Rsp::new();
        assert!(r.submit_task(0x0400_1000, 1024).is_err());
        assert_eq!(r.stats.tasks, 1);
    }
}
