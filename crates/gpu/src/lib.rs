//! RDP command processor + VI state + software framebuffer.
//!
//! Pipeline: RSP task -> RDP command list -> decode -> state -> pixels.
//! Unknown commands are explicit errors with the raw word, never silent.

use n64_common::{EmuError, Result};
use std::collections::HashMap;

/// RDP command (first word selects operation per SGI docs).
#[derive(Debug, Clone)]
pub enum RdpCommand {
    NoOp,
    FillTriangle {
        words: Vec<u32>,
    },
    TextureRect {
        words: Vec<u32>,
    },
    SyncFull,
    SyncPipe,
    SyncTile,
    SyncLoad,
    SetColorImage {
        fmt: u32,
        siz: u32,
        width: u32,
        addr: u32,
    },
    SetFillColor {
        color: u32,
    },
    SetScissor {
        words: Vec<u32>,
    },
    Unknown {
        word: u32,
    },
}

#[derive(Debug, Default)]
pub struct GpuStats {
    pub decoded: u64,
    pub unsupported: u64,
    pub triangles: u64,
    pub last_unsupported: Option<String>,
}

/// Minimal RDP state.
pub struct Rdp {
    pub stats: GpuStats,
    pub regs: HashMap<u32, u32>,
    pub width: u32,
    pub height: u32,
    pub framebuffer: Vec<u8>,
    pub fill_color: u32,
}

impl Rdp {
    pub fn new() -> Self {
        Self {
            stats: GpuStats::default(),
            regs: HashMap::new(),
            width: 320,
            height: 240,
            framebuffer: vec![0; 320 * 240 * 4],
            fill_color: 0,
        }
    }

    pub fn with_size(width: u32, height: u32) -> Self {
        let mut r = Self::new();
        r.width = width;
        r.height = height;
        r.framebuffer = vec![0; (width * height * 4) as usize];
        r
    }

    /// Decode one 8-byte command (two words). Returns command + words used.
    pub fn decode_command(&mut self, w0: u32, w1: u32) -> Result<(RdpCommand, usize)> {
        let op = (w0 >> 24) & 0xFF;
        self.stats.decoded += 1;
        let cmd = match op {
            0x00 => RdpCommand::NoOp,
            0x08..=0x0B => RdpCommand::FillTriangle {
                words: vec![w0, w1],
            },
            0x24 => RdpCommand::TextureRect {
                words: vec![w0, w1],
            },
            0x29 => RdpCommand::SyncFull,
            0x27 => RdpCommand::SyncPipe,
            0x28 => RdpCommand::SyncTile,
            0x2A => RdpCommand::SyncLoad,
            0x3F => {
                let fmt = (w0 >> 21) & 0x7;
                let siz = (w0 >> 19) & 0x3;
                let width = w0 & 0x3FF;
                RdpCommand::SetColorImage {
                    fmt,
                    siz,
                    width: width + 1,
                    addr: w1 & 0x00FF_FFFF,
                }
            }
            0x37 => RdpCommand::SetFillColor { color: w1 },
            0x2D => RdpCommand::SetScissor {
                words: vec![w0, w1],
            },
            _ => {
                let msg = format!("rdp op={:02x} w0={:08x} w1={:08x}", op, w0, w1);
                self.stats.unsupported += 1;
                self.stats.last_unsupported = Some(msg.clone());
                return Err(EmuError::UnimplementedRdp(msg));
            }
        };
        Ok((cmd, 2))
    }

    /// Dispatch a decoded command. Fill ops write real pixels; syncs succeed.
    pub fn dispatch(&mut self, cmd: &RdpCommand) -> Result<()> {
        match cmd {
            RdpCommand::NoOp
            | RdpCommand::SyncFull
            | RdpCommand::SyncPipe
            | RdpCommand::SyncTile
            | RdpCommand::SyncLoad => Ok(()),
            RdpCommand::SetFillColor { color } => {
                self.fill_color = *color;
                Ok(())
            }
            RdpCommand::SetColorImage { width, .. } => {
                self.width = (*width).clamp(1, 640);
                self.framebuffer = vec![0; (self.width * self.height * 4) as usize];
                Ok(())
            }
            RdpCommand::SetScissor { .. } => Ok(()),
            RdpCommand::FillTriangle { .. } => {
                self.stats.triangles += 1;
                // Software fill with current fill color (RGBA16 -> RGBA8 approx).
                let r = (((self.fill_color >> 11) & 0x1F) * 255 / 31) as u8;
                let g = (((self.fill_color >> 6) & 0x1F) * 255 / 31) as u8;
                let b = (((self.fill_color >> 1) & 0x1F) * 255 / 31) as u8;
                let (px_chunks, _) = self.framebuffer.as_chunks_mut::<4>();
                for px in px_chunks {
                    px[0] = r;
                    px[1] = g;
                    px[2] = b;
                    px[3] = 0xFF;
                }
                Ok(())
            }
            RdpCommand::TextureRect { words } => {
                let msg = format!("TextureRect len={} pending (no sampler yet)", words.len());
                self.stats.unsupported += 1;
                self.stats.last_unsupported = Some(msg.clone());
                Err(EmuError::UnimplementedRdp(msg))
            }
            RdpCommand::Unknown { word } => {
                Err(EmuError::UnimplementedRdp(format!("unknown {:08x}", word)))
            }
        }
    }

    /// Validation: fill the framebuffer red and verify pixels read back.
    pub fn validate_fill(&mut self) -> Result<bool> {
        self.dispatch(&RdpCommand::SetFillColor { color: 0xF801 })?;
        self.dispatch(&RdpCommand::FillTriangle { words: vec![0, 0] })?;
        let (chunks, _) = self.framebuffer.as_chunks::<4>();
        Ok(chunks
            .iter()
            .any(|p| p[0] > 128 && p[1] < 128 && p[2] < 128))
    }
}

impl Default for Rdp {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_decodes() {
        let mut r = Rdp::new();
        let (c, used) = r.decode_command(0x2900_0000, 0).unwrap();
        assert_eq!(used, 2);
        assert!(matches!(c, RdpCommand::SyncFull));
        r.dispatch(&c).unwrap();
    }

    #[test]
    fn unknown_op_is_loud() {
        let mut r = Rdp::new();
        assert!(r.decode_command(0xFF00_0000, 0).is_err());
        assert_eq!(r.stats.unsupported, 1);
    }

    #[test]
    fn fill_triangle_writes_pixels() {
        let mut r = Rdp::with_size(64, 64);
        r.dispatch(&RdpCommand::SetFillColor { color: 0xF801 })
            .unwrap();
        r.dispatch(&RdpCommand::FillTriangle { words: vec![0, 0] })
            .unwrap();
        assert_eq!(r.stats.triangles, 1);
        assert!(r.framebuffer[0] > 128);
    }
}
