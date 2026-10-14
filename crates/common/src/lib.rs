//! Shared types: errors, config, logging.

use std::path::PathBuf;
use thiserror::Error;

/// Machine-wide result type.
pub type Result<T> = std::result::Result<T, EmuError>;

/// Explicit failure modes. No silent success.
#[derive(Debug, Error)]
pub enum EmuError {
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("unimplemented service: {0}")]
    UnimplementedService(u32),
    #[error("unimplemented RDP command: {0}")]
    UnimplementedRdp(String),
    #[error("unimplemented RSP vector op: {0}")]
    UnimplementedRsp(String),
    #[error("invalid ROM: {0}")]
    InvalidRom(String),
    #[error("invalid executable: {0}")]
    InvalidExecutable(String),
    #[error("memory fault: addr={addr:#010x} size={size} kind={kind}")]
    MemoryFault {
        addr: u32,
        size: usize,
        kind: String,
    },
    #[error("alignment fault: addr={addr:#010x}")]
    AlignmentFault { addr: u32 },
    #[error("io error: {0}")]
    Io(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("other: {0}")]
    Other(String),
}

impl From<std::io::Error> for EmuError {
    fn from(e: std::io::Error) -> Self {
        EmuError::Io(e.to_string())
    }
}

/// Global emulator configuration.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EmuConfig {
    pub content_dir: PathBuf,
    pub save_dir: PathBuf,
    pub expansion_pak: bool,
    pub audio_null: bool,
    pub player_name: String,
}

impl Default for EmuConfig {
    fn default() -> Self {
        Self {
            content_dir: PathBuf::from("content"),
            save_dir: PathBuf::from("saves"),
            expansion_pak: false,
            audio_null: true,
            player_name: String::from("player1"),
        }
    }
}

impl EmuConfig {
    pub fn validate(&self) -> Result<()> {
        if self.player_name.trim().is_empty() {
            return Err(EmuError::Config("player_name must not be empty".into()));
        }
        Ok(())
    }
}

/// Initialize logging once. Safe to call multiple times in tests.
pub fn init_logging() {
    let _ = env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .is_test(true)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_validates() {
        let c = EmuConfig::default();
        c.validate().unwrap();
    }

    #[test]
    fn empty_player_rejected() {
        let c = EmuConfig {
            player_name: "  ".into(),
            ..EmuConfig::default()
        };
        assert!(c.validate().is_err());
    }

    #[test]
    fn error_display_is_explicit() {
        let e = EmuError::Unsupported("mult".into());
        assert!(e.to_string().contains("unsupported"));
    }
}
