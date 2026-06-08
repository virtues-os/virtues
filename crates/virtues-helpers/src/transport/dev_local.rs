//! Development server transport — loopback only, no WG, no TLS.
//!
//! Compile-time gated behind the `dev-transport` feature. Never present
//! in release binaries.

use super::ServerTransport;
use std::io;
use tokio::net::TcpListener;

pub struct DevLocalServerTransport {
    pub port: u16,
}

impl DevLocalServerTransport {
    pub fn new(port: u16) -> Self {
        Self { port }
    }
}

#[async_trait::async_trait]
impl ServerTransport for DevLocalServerTransport {
    async fn bind(&self) -> io::Result<TcpListener> {
        TcpListener::bind(format!("127.0.0.1:{}", self.port)).await
    }

    fn describe(&self) -> String {
        format!("dev-local://127.0.0.1:{}", self.port)
    }
}
