//! The box registry: maps an SNI hostname to the live control handle of the box
//! that dialed in for it. Ephemeral, current-state-only routing metadata
//! (overwritten, never appended) — no identity, no content.

use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Live handle to a connected box. Cloneable so the client-accept path can signal
/// the box (request a work connection for a `conn_id`) without locking the map.
#[derive(Clone)]
pub struct BoxHandle {
    /// Send a `conn_id` to ask the box to dial a work connection for it. The
    /// control writer task forwards this as `RelayMsg::OpenConn`.
    pub work_tx: mpsc::Sender<Uuid>,
    /// Registration generation — lets a teardown remove *its own* entry without
    /// clobbering a newer reconnect (the old-vs-new overlap bug class).
    pub gen: u64,
}

/// Thread-safe `sni -> BoxHandle` map shared across all accept loops.
#[derive(Clone, Default)]
pub struct Registry {
    boxes: Arc<DashMap<String, BoxHandle>>,
    gen: Arc<AtomicU64>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (replacing any stale handle — **last-writer-wins**). Returns the
    /// generation assigned to this registration; pass it to [`unregister_if`] on
    /// teardown so a box reconnecting after a restart isn't clobbered.
    pub fn register(&self, sni: String, work_tx: mpsc::Sender<Uuid>) -> u64 {
        let gen = self.gen.fetch_add(1, Ordering::SeqCst);
        self.boxes.insert(sni, BoxHandle { work_tx, gen });
        gen
    }

    /// Look up the box currently serving `sni`.
    pub fn lookup(&self, sni: &str) -> Option<BoxHandle> {
        self.boxes.get(sni).map(|h| h.value().clone())
    }

    /// Remove `sni` **only if** the current entry is still generation `gen` — i.e.
    /// this connection's own registration, not a newer one that replaced it.
    pub fn unregister_if(&self, sni: &str, gen: u64) {
        self.boxes.remove_if(sni, |_, h| h.gen == gen);
    }

    /// Number of boxes currently registered (aggregate, identity-free metric).
    pub fn len(&self) -> usize {
        self.boxes.len()
    }

    #[allow(dead_code)] // pairs with len() for aggregate metrics (P2)
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }
}
