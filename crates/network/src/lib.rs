//! Network stub: the N64 has no network hardware.
//!
//! This crate exists to mirror the workspace layout and to hold the
//! loopback harness used by frontend smoke tests. No official-service
//! claims, no hardcoded endpoints.

use n64_common::{EmuError, Result};
use std::net::UdpSocket;

pub struct UdpBridge {
    sock: UdpSocket,
}

impl UdpBridge {
    pub fn bind(addr: &str) -> Result<Self> {
        let sock = UdpSocket::bind(addr).map_err(|e| EmuError::Network(e.to_string()))?;
        sock.set_read_timeout(Some(std::time::Duration::from_millis(200)))
            .map_err(|e| EmuError::Network(e.to_string()))?;
        Ok(Self { sock })
    }

    pub fn local_addr(&self) -> Result<String> {
        self.sock
            .local_addr()
            .map(|a| a.to_string())
            .map_err(|e| EmuError::Network(e.to_string()))
    }

    pub fn send_to(&self, data: &[u8], target: &str) -> Result<()> {
        self.sock
            .send_to(data, target)
            .map(|_| ())
            .map_err(|e| EmuError::Network(e.to_string()))
    }

    pub fn recv(&self, buf: &mut [u8]) -> Result<(usize, String)> {
        self.sock
            .recv_from(buf)
            .map(|(n, a)| (n, a.to_string()))
            .map_err(|e| EmuError::Network(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_send_recv() {
        let rx = UdpBridge::bind("127.0.0.1:0").unwrap();
        let tx = UdpBridge::bind("127.0.0.1:0").unwrap();
        let target = rx.local_addr().unwrap();
        tx.send_to(b"n64-beacon", &target).unwrap();
        let mut buf = [0u8; 64];
        let (n, _) = rx.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], b"n64-beacon");
    }
}
