//! Transport-agnostic reach client for a Virtues box.
//!
//! Pairs a device to a box (`pair::consume`) and serves the box over iroh to a
//! local HTTP client (`proxy::serve_loopback`) or sends raw HTTP requests to it
//! (`build_client` → `VirtuesIrohClient::request`). Credential storage is
//! injected via [`BoxStore`] so the desktop sidecar and the mobile in-process
//! plugin share one implementation while keeping their own keystores.

mod model;
mod store;

pub mod outbox;
pub mod pair;
pub mod provision;
pub mod proxy;
pub mod scan;
pub mod session;

pub use model::PairedBox;
pub use virtues_iroh::install_crypto_provider;
pub use proxy::{build_client, resolve_box_lan, serve_loopback, serve_on, serve_on_provider};
pub use scan::{local_private_ipv4s, scan_subnet, DiscoveredBox};
pub use session::{probe_session, SessionState};
pub use store::BoxStore;

// Re-export the iroh client so consumers (upload coordinator) can name it
// without a direct virtues-iroh dep.
pub use virtues_iroh::{PathKind, VirtuesIrohClient};
