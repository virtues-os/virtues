//! Production server transport: a plain TCP listener. Paired devices reach
//! the server over iroh, which is encrypted; this port serves the LAN.

use super::ServerTransport;
use std::io;
use tokio::net::TcpListener;

pub struct RealServerTransport {
    pub host: String,
    pub port: u16,
}

impl RealServerTransport {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self { host: host.into(), port }
    }
}

#[async_trait::async_trait]
impl ServerTransport for RealServerTransport {
    async fn bind(&self) -> io::Result<TcpListener> {
        match TcpListener::bind(format!("{}:{}", self.host, self.port)).await {
            // A kernel booted with IPv6 off (`ipv6.disable=1`) has no `[::]`
            // to bind. Every interface on IPv4 is what the person meant.
            Err(e) if is_ipv6_any(&self.host) && ipv6_unavailable(&e) => {
                tracing::warn!(error = %e, "IPv6 is off on this machine; listening on IPv4 only");
                TcpListener::bind(("0.0.0.0", self.port)).await
            }
            other => other,
        }
    }

    fn describe(&self) -> String {
        format!("real://{}:{}", self.host, self.port)
    }
}

/// `[::]` or `::`, the IPv6 any-address the installer's unit binds.
fn is_ipv6_any(host: &str) -> bool {
    host.trim_matches(|c| c == '[' || c == ']')
        .parse::<std::net::Ipv6Addr>()
        .is_ok_and(|a| a.is_unspecified())
}

/// The bind failed because this machine has no IPv6, not for another reason
/// such as the port being taken.
fn ipv6_unavailable(e: &io::Error) -> bool {
    #[cfg(unix)]
    {
        matches!(e.raw_os_error(), Some(libc_errno::EAFNOSUPPORT) | Some(libc_errno::EADDRNOTAVAIL))
    }
    #[cfg(not(unix))]
    {
        let _ = e;
        false
    }
}

#[cfg(unix)]
mod libc_errno {
    #[cfg(target_os = "linux")]
    pub const EAFNOSUPPORT: i32 = 97;
    #[cfg(target_os = "linux")]
    pub const EADDRNOTAVAIL: i32 = 99;
    #[cfg(not(target_os = "linux"))]
    pub const EAFNOSUPPORT: i32 = 47;
    #[cfg(not(target_os = "linux"))]
    pub const EADDRNOTAVAIL: i32 = 49;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_ipv6_any_address_falls_back() {
        assert!(is_ipv6_any("[::]"));
        assert!(is_ipv6_any("::"));
        assert!(!is_ipv6_any("0.0.0.0"));
        assert!(!is_ipv6_any("[::1]"));
        assert!(!is_ipv6_any("127.0.0.1"));
    }

    #[cfg(unix)]
    #[test]
    fn a_taken_port_is_not_mistaken_for_missing_ipv6() {
        assert!(!ipv6_unavailable(&io::Error::from(io::ErrorKind::AddrInUse)));
        assert!(ipv6_unavailable(&io::Error::from_raw_os_error(libc_errno::EAFNOSUPPORT)));
    }
}
