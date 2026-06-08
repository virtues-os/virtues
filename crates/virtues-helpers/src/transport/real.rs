//! Production server transport.
//!
//! Binds a plain TCP listener; WireGuard tunneling happens at the OS
//! layer below this. WS-2 will extend this with per-pair CA TLS
//! termination on `virtues.internal`.

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
        TcpListener::bind(format!("{}:{}", self.host, self.port)).await
    }

    fn describe(&self) -> String {
        format!("real://{}:{}", self.host, self.port)
    }
}
