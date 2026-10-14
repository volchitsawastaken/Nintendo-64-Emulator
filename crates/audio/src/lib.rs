//! AI audio: null sink works, host backend is explicit until wired.

use n64_common::{EmuError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Null,
    Host,
}

pub struct AudioOut {
    pub backend: Backend,
    pub frames_pushed: u64,
    pub samples: Vec<i16>,
}

impl AudioOut {
    pub fn new(backend: Backend) -> Self {
        Self {
            backend,
            frames_pushed: 0,
            samples: Vec::new(),
        }
    }

    /// Push interleaved stereo PCM. Null counts; Host reports Unsupported.
    pub fn push_pcm(&mut self, _pcm: &[i16]) -> Result<()> {
        match self.backend {
            Backend::Null => {
                self.frames_pushed += 1;
                Ok(())
            }
            Backend::Host => Err(EmuError::Unsupported("host audio not wired yet".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_sink_counts() {
        let mut a = AudioOut::new(Backend::Null);
        a.push_pcm(&[0, 0, 0, 0]).unwrap();
        assert_eq!(a.frames_pushed, 1);
    }

    #[test]
    fn host_is_loud() {
        let mut a = AudioOut::new(Backend::Host);
        assert!(a.push_pcm(&[0]).is_err());
    }
}
