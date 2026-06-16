//! The userspace TCP/IP stack (smoltcp) and the virtual device that bridges it
//! to the WireGuard data plane.
//!
//! smoltcp normally drives a real NIC. Here the "NIC" is [`VirtualDevice`]: its
//! *receive* side is a queue of plaintext IP packets decapsulated from WG, and
//! its *transmit* side is a queue of plaintext IP packets to be encapsulated to
//! WG. The event loop in `tunnel.rs` shuttles packets between these queues and
//! [`WgTunnel`](crate::wg::WgTunnel).

use std::collections::VecDeque;

use smoltcp::phy::{Checksum, ChecksumCapabilities, Device, DeviceCapabilities, Medium};
use smoltcp::time::Instant;

use crate::wg::TUNNEL_MTU;

/// In-memory L3 device. Medium is IP (no Ethernet framing) — point-to-point WG.
pub(crate) struct VirtualDevice {
    /// IP packets from WG → smoltcp (inbound).
    pub(crate) inbound: VecDeque<Vec<u8>>,
    /// IP packets smoltcp → WG (outbound).
    pub(crate) outbound: VecDeque<Vec<u8>>,
}

impl VirtualDevice {
    pub(crate) fn new() -> Self {
        Self {
            inbound: VecDeque::new(),
            outbound: VecDeque::new(),
        }
    }
}

impl Device for VirtualDevice {
    type RxToken<'a> = RxToken;
    type TxToken<'a> = TxToken<'a>;

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ip;
        caps.max_transmission_unit = TUNNEL_MTU;
        // The box validates checksums; smoltcp should compute ours fully.
        let mut cksum = ChecksumCapabilities::default();
        cksum.ipv4 = Checksum::Both;
        cksum.tcp = Checksum::Both;
        cksum.udp = Checksum::Both;
        caps.checksum = cksum;
        caps
    }

    fn receive(&mut self, _now: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let buf = self.inbound.pop_front()?;
        Some((
            RxToken { buf },
            TxToken {
                queue: &mut self.outbound,
            },
        ))
    }

    fn transmit(&mut self, _now: Instant) -> Option<Self::TxToken<'_>> {
        Some(TxToken {
            queue: &mut self.outbound,
        })
    }
}

pub(crate) struct RxToken {
    buf: Vec<u8>,
}

impl smoltcp::phy::RxToken for RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.buf)
    }
}

pub(crate) struct TxToken<'a> {
    queue: &'a mut VecDeque<Vec<u8>>,
}

impl smoltcp::phy::TxToken for TxToken<'_> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut buf = vec![0u8; len];
        let result = f(&mut buf);
        self.queue.push_back(buf);
        result
    }
}
