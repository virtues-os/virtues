//! Yjs WebSocket sync server using yrs crate
//!
//! Implements the y-websocket protocol for compatibility with the y-websocket client library.
//! Uses Y.Text for plain markdown content (via y-codemirror.next on the frontend).
//!
//! Protocol:
//! - Message type 0 (Sync):
//!   - [0, 0, ...stateVector] = sync step 1 (client sends their state vector)
//!   - [0, 1, ...update] = sync step 2 (server sends missing updates)
//!   - [0, 2, ...update] = incremental update
//! - Message type 1 (Awareness): cursor/presence data (optional)
//!
//! Responsibilities:
//! 1. Handle y-websocket protocol (binary messages with type prefixes)
//! 2. Maintain yrs::Doc per page (cached in memory with TTL)
//! 3. Debounced materialization to content column
//! 4. Placeholder hooks for future embedding updates
//! 5. The document contract (`crates/virtues-document`) on the socket: a
//!    client says the contract it reads (`?contract=N`, 0 when absent) and is
//!    bound only at or above the page's (`bind`); an update to a page written
//!    as a Yjs tree is checked against the contract before it is applied or
//!    relayed (`admit_update`), and a refused client is closed with a code
//!    y-websocket does not reconnect on.

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    response::Response,
};
use moka::sync::Cache;
use sqlx::PgPool;
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::{broadcast, watch, RwLock};
use tokio::time::Instant;
use yrs::{
    updates::decoder::Decode, updates::encoder::Encode, Doc, GetString, OffsetKind, Options, ReadTxn,
    StateVector, Text, Transact, WriteTxn,
};

// y-websocket message types
const MSG_SYNC: u8 = 0;
const MSG_AWARENESS: u8 = 1;

// y-websocket sync message subtypes
const MSG_SYNC_STEP1: u8 = 0;
const MSG_SYNC_STEP2: u8 = 1;
const MSG_SYNC_UPDATE: u8 = 2;

// Close codes. y-websocket stops reconnecting on any code from 4400 to 4499
// (it reads them as HTTP 4xx: the server decided, and retrying cannot help),
// so a refused client stays off instead of resending what was refused.
/// The page is written under a newer contract than the client reads, or was
/// raised past it while the client was bound (HTTP 426, Upgrade Required).
const CLOSE_CONTRACT: u16 = 4426;
/// The client sent an update outside the contract (HTTP 422).
const CLOSE_REFUSED: u16 = 4422;

/// Cached document state with broadcast channel for multi-client sync
pub struct PageDoc {
    pub doc: Doc,
    pub broadcast_tx: broadcast::Sender<Vec<u8>>,
    pub last_update: Instant,
    /// `app_pages.updated_at` as read when this doc was built. Every update
    /// to the row moves it (the table's `set_updated_at` trigger), so a
    /// different value means something wrote the page after the doc was
    /// built.
    built_at: chrono::DateTime<chrono::Utc>,
    /// How many changes have been applied to this doc since it was built.
    changes: u64,
    /// The largest `changes` count a state saved to the database included.
    /// Below `changes`, the doc holds edits the database does not have yet.
    saved: u64,
    /// The document contract version the doc is written under: the stamp in
    /// its `meta` (`virtues_document::stamped_version`), or 0 for a page that
    /// is markdown in a Y.Text. Read from the stamp when the doc is built:
    /// only the server writes `meta`, since `admit_update` refuses a
    /// client's update that writes it, on a page of either kind. Kept here,
    /// not read from the doc on each use, so every socket bound to the doc
    /// watches the one value `raise_contract` moves.
    contract: watch::Sender<u32>,
}

impl PageDoc {
    fn new(doc: Doc, built_at: chrono::DateTime<chrono::Utc>) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        let stamped = virtues_document::stamped_version(&doc.transact()).unwrap_or(0);
        Self {
            doc,
            broadcast_tx,
            last_update: Instant::now(),
            built_at,
            changes: 0,
            saved: 0,
            contract: watch::Sender::new(stamped),
        }
    }

    /// The contract version the doc is written under; 0 for a markdown page.
    pub fn contract(&self) -> u32 {
        *self.contract.borrow()
    }

    /// Stamp this server's contract version into the doc, when the doc is
    /// below it, and tell every socket bound to it: one bound below the new
    /// version closes, since its client deletes what its schema cannot read.
    /// The stamp goes to the other clients like any edit. Returns the change
    /// count of the stamped state, for the save, or `None` when the doc was
    /// already at the version.
    fn raise_contract(&mut self) -> Option<u64> {
        let version = virtues_document::contract().version;
        if self.contract() >= version {
            return None;
        }
        let before = self.doc.transact().state_vector();
        virtues_document::stamp_version(virtues_document::contract(), &mut self.doc.transact_mut());
        let update = self.doc.transact().encode_diff_v1(&before);
        self.contract.send_replace(version);
        let _ = self.broadcast_tx.send(encode_sync_update(&update));
        Some(self.record_change())
    }

    /// Repair what a merge of clients' edits left outside the contract
    /// (`virtues_document::repair`: an emptied list, a block id used twice),
    /// in a transaction of the server's own that is relayed like any edit.
    /// Returns whether it changed the doc.
    fn repair(&mut self) -> bool {
        let update = {
            let mut txn = self.doc.transact_mut();
            if virtues_document::repair(&mut txn) == 0 {
                return false;
            }
            txn.encode_update_v1()
        };
        let _ = self.broadcast_tx.send(encode_sync_update(&update));
        true
    }

    /// Count a change just applied, and return the count the doc's state now
    /// includes, for the save that carries it.
    fn record_change(&mut self) -> u64 {
        self.last_update = Instant::now();
        self.changes += 1;
        self.changes
    }

    fn mark_saved(&mut self, generation: u64) {
        self.saved = self.saved.max(generation);
    }

    fn has_unsaved_changes(&self) -> bool {
        self.changes > self.saved
    }

    /// The text and the full state, from one transaction.
    fn written(&self) -> Written {
        let txn = self.doc.transact();
        Written {
            text: content_of(&txn),
            state: virtues_document::encode_state(&txn, &StateVector::default()),
        }
    }
}

/// A page's text and the doc state that says it, taken under one lock: what
/// a write left on the page, or what a read found there. A version cut from
/// it (`pages::cut_version`) stores exactly the text it shows, whatever an
/// open editor typed a moment later.
#[derive(Clone, PartialEq, Eq)]
pub struct Written {
    pub text: String,
    /// `encode_state_as_update_v1` of the whole doc: what
    /// `app_page_versions.yjs_snapshot` holds.
    pub state: Vec<u8>,
}

impl std::fmt::Debug for Written {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Written")
            .field("text", &self.text)
            .field("state", &format_args!("<{} bytes>", self.state.len()))
            .finish()
    }
}

/// What a find/replace (`YjsState::apply_text_edit`) found on the page and
/// what it left there, both read under the lock the edit was made under.
#[derive(Debug)]
pub struct TextEdit {
    /// The page just before the edit, including anything an open editor
    /// typed after the caller last read it.
    pub before: Written,
    /// What the edit left on the page: `Ok`, or `NotSaved` carrying the same
    /// when the save failed. The edit is applied either way.
    pub after: Result<Written, TextWriteError>,
}

/// Document cache with automatic TTL eviction
pub struct DocCache {
    pages: Cache<String, Arc<RwLock<PageDoc>>>,
    /// Every doc handed out that something still holds: an open socket, or
    /// the save queue while a state of the doc waits there (`PendingSave`).
    ///
    /// An editor's socket calls `get_or_create` once and keeps its `Arc` for
    /// as long as the tab is open, so moka's idle eviction can drop a doc that
    /// is still in use. Loading the page again would then make a second doc:
    /// a server-side write would land in it, the open tab would never see that
    /// write, and the tab's next keystroke would save its own doc over it. A
    /// miss in `pages` looks here first, so every writer in this process
    /// reaches the doc an open socket holds, or the doc whose unsaved state
    /// is queued.
    held: std::sync::Mutex<std::collections::HashMap<String, Weak<RwLock<PageDoc>>>>,
}

impl DocCache {
    pub fn new() -> Self {
        Self {
            pages: Cache::builder()
                .time_to_idle(Duration::from_secs(30 * 60)) // 30 min TTL
                .max_capacity(100)
                .build(),
            held: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn held(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, Weak<RwLock<PageDoc>>>> {
        // The map holds only weak pointers and every critical section is a
        // plain insert, lookup or retain, so a panic elsewhere cannot leave it
        // half-written.
        self.held.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The doc this process already has for a page: warm in moka, or evicted
    /// from moka but still held by someone (an open socket, or the save
    /// queue), in which case it goes back into moka so the next reader finds
    /// it without the detour.
    ///
    /// Every insert into moka happens under the `held` lock, beside the held
    /// entry it mirrors, so `retire` can take a doc out of both at once.
    fn in_memory(&self, page_id: &str) -> Option<Arc<RwLock<PageDoc>>> {
        if let Some(doc) = self.pages.get(page_id) {
            return Some(doc);
        }
        let held = self.held();
        let doc = held.get(page_id).and_then(Weak::upgrade)?;
        self.pages.insert(page_id.to_string(), doc.clone());
        Some(doc)
    }

    /// Whether `doc` is the doc registered for the page, in a map read under
    /// the `held` lock.
    fn registered_in(
        held: &std::collections::HashMap<String, Weak<RwLock<PageDoc>>>,
        page_id: &str,
        doc: &Arc<RwLock<PageDoc>>,
    ) -> bool {
        held.get(page_id)
            .and_then(Weak::upgrade)
            .is_some_and(|r| Arc::ptr_eq(&r, doc))
    }

    /// Whether `doc` is still the page's doc. False once another caller has
    /// retired it, for a write outside the CRDT, and loaded the page again:
    /// a caller holding it then has to take the doc that replaced it.
    fn is_registered(&self, page_id: &str, doc: &Arc<RwLock<PageDoc>>) -> bool {
        Self::registered_in(&self.held(), page_id, doc)
    }

    /// Take `doc` out of this process's cache so the next reader loads the
    /// page again. Only while `doc` is still the one registered for the page:
    /// false means another caller already replaced it, and its doc is the one
    /// to use.
    fn retire(&self, page_id: &str, doc: &Arc<RwLock<PageDoc>>) -> bool {
        let mut held = self.held();
        if !Self::registered_in(&held, page_id, doc) {
            return false;
        }
        held.remove(page_id);
        if self.pages.get(page_id).is_some_and(|m| Arc::ptr_eq(&m, doc)) {
            self.pages.invalidate(page_id);
        }
        true
    }

    /// Get or load document from database
    pub async fn get_or_create(
        &self,
        page_id: &str,
        pool: &PgPool,
    ) -> Result<Arc<RwLock<PageDoc>>, anyhow::Error> {
        // Check memory first, but not blindly. A page written through the
        // pool (the nightly narration, a migration, a manual reset) has
        // `yjs_state IS NULL`, and a doc built before that write would put the
        // old prose back over it on its next save. A re-narrated day page was
        // once clobbered back to its old text exactly so. So the database wins
        // when the page was written after this doc was built and the doc holds
        // nothing the database lacks.
        //
        // Both halves matter. A page nobody has saved through the CRDT is NULL
        // too, including while this doc's first save is queued or in flight:
        // that alone is no rewrite. And a doc with unsaved changes is never
        // dropped, because its queued state is saved over the page either
        // way, and a doc rebuilt beside it would hold neither.
        //
        // A doc is handed out only while it is still the page's: another
        // caller can retire it and load the page again while this one awaits
        // (the query below waits on the pool), and the doc it found is then
        // not the one every writer reaches. The loop takes the replacement.
        loop {
            let Some(doc) = self.in_memory(page_id) else { break };
            let (unsaved, built_at) = {
                let d = doc.read().await;
                (d.has_unsaved_changes(), d.built_at)
            };
            if unsaved {
                if self.is_registered(page_id, &doc) {
                    return Ok(doc);
                }
                continue;
            }
            // No row: the page is gone, and the doc in hand is all there is.
            let rewritten: bool = sqlx::query_scalar::<_, bool>(
                "SELECT yjs_state IS NULL AND updated_at IS DISTINCT FROM $2 \
                 FROM app_pages WHERE id = $1",
            )
            .bind(page_id)
            .bind(built_at)
            .fetch_optional(pool)
            .await?
            .unwrap_or(false);
            if !rewritten || doc.read().await.has_unsaved_changes() {
                if self.is_registered(page_id, &doc) {
                    return Ok(doc);
                }
                continue;
            }
            if self.retire(page_id, &doc) {
                tracing::info!(
                    page_id,
                    "page was rewritten outside the CRDT - dropping the cached doc and reseeding"
                );
                break;
            }
        }

        // Content, state and the timestamp in one read, so `built_at` dates
        // exactly the text the doc is built from.
        let row: Option<(String, Option<Vec<u8>>, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
            "SELECT content, yjs_state, updated_at FROM app_pages \
             WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(page_id)
        .fetch_optional(pool)
        .await?;
        let Some((content, yjs_state, built_at)) = row else {
            anyhow::bail!("Page not found: {page_id}");
        };

        let doc = match yjs_state {
            Some(state) => doc_from_state(page_id, &state),
            // No Yjs state yet: the page's markdown is the doc's text.
            None => doc_from_text(&content),
        };

        let page_doc = Arc::new(RwLock::new(PageDoc::new(doc, built_at)));

        // Two callers can miss together and both load (the loads await the
        // database). Whoever registers first wins and the other takes that
        // doc, so a miss never builds a second doc beside a live one. A page
        // has two docs in this process only after a rewrite outside the CRDT
        // (above): the retired doc lives on in any socket still holding it,
        // until that tab reconnects.
        let page_doc = {
            let mut held = self.held();
            let page_doc = match held.get(page_id).and_then(Weak::upgrade) {
                Some(first) => first,
                None => {
                    held.retain(|_, doc| doc.strong_count() > 0);
                    held.insert(page_id.to_string(), Arc::downgrade(&page_doc));
                    page_doc
                }
            };
            self.pages.insert(page_id.to_string(), page_doc.clone());
            page_doc
        };

        Ok(page_doc)
    }
}

impl Default for DocCache {
    fn default() -> Self {
        Self::new()
    }
}

/// A new, empty doc. Every doc the server builds comes from here.
///
/// Text offsets count UTF-8 bytes (`OffsetKind::Bytes`), because every
/// offset the server hands to a `content` text is measured on a Rust `str`:
/// `str::find` and `str::len` in `apply_text_edit`, `text_ops` for the
/// diffing writes. Counted in any other unit, an edit after a non-ASCII
/// character would land in the wrong place. Set here rather than left to
/// yrs's default so the unit is stated where it is relied on.
///
/// Browser editors count UTF-16 code units in their own docs. The unit is a
/// property of each replica's API, not of the updates they exchange, so the
/// two never need to agree.
///
/// The client id is 32 bits (`virtues_document::client_id`, which says why).
pub(crate) fn new_doc() -> Doc {
    Doc::with_options(Options {
        client_id: virtues_document::client_id(),
        offset_kind: OffsetKind::Bytes,
        ..Options::default()
    })
}

/// Why a v1 update did not apply to a doc (`apply_v1`).
#[derive(Debug)]
pub(crate) enum ApplyError {
    /// The bytes are not a v1 update. The doc is untouched.
    Decode(String),
    /// yrs refused the update, or panicked applying it. The doc holds
    /// whatever applied before the failure.
    Apply(String),
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "not a v1 update: {e}"),
            Self::Apply(e) => f.write_str(e),
        }
    }
}

/// Decode a v1 update and apply it to `doc` in one transaction. Returns
/// whether that transaction deleted anything: only what this update
/// deleted, not text it deletes again.
///
/// Decoded through `virtues_document::decode_update`, which refuses values
/// nested deep enough to overflow the stack while yrs decodes them: that
/// would abort the process, where `catch_unwind` catches only a panic.
/// yrs returns an error for some malformed updates and still panics on
/// others; both come back as `ApplyError::Apply`.
fn apply_v1(doc: &Doc, bytes: &[u8]) -> Result<bool, ApplyError> {
    let update = virtues_document::decode_update(bytes)
        .map_err(|e| ApplyError::Decode(e.to_string()))?;
    let applied = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let mut txn = doc.transact_mut();
        txn.apply_update(update)?;
        Ok::<_, yrs::error::UpdateError>(!txn.delete_set().is_empty())
    }));
    match applied {
        Ok(Ok(deleted)) => Ok(deleted),
        Ok(Err(e)) => Err(ApplyError::Apply(e.to_string())),
        Err(_) => Err(ApplyError::Apply("yrs panicked applying the update".into())),
    }
}

/// The doc's `content` text as `txn` sees it, or empty when it has none.
fn content_of<T: ReadTxn>(txn: &T) -> String {
    txn.get_text("content")
        .map(|t| t.get_string(txn))
        .unwrap_or_default()
}

/// A page's doc from its saved state (`app_pages.yjs_state`). A state that
/// does not load is logged, and the doc holds what applied of it.
fn doc_from_state(page_id: &str, state: &[u8]) -> Doc {
    let doc = new_doc();
    if let Err(error) = apply_v1(&doc, state) {
        tracing::error!(
            page_id,
            %error,
            "the page's saved state did not load; its doc holds what applied before the failure"
        );
    }
    doc
}

/// A doc whose text is `text`: what a page with no saved CRDT state is
/// loaded as. The one way to build a page's doc from its markdown.
fn doc_from_text(text: &str) -> Doc {
    let doc = new_doc();
    {
        let mut txn = doc.transact_mut();
        let content = txn.get_or_insert_text("content");
        if !text.is_empty() {
            content.insert(&mut txn, 0, text);
        }
    }
    doc
}

/// The full state of a doc built from `text` (`doc_from_text`), as
/// `app_page_versions.yjs_snapshot` holds it: for versioning text the
/// server has as markdown rather than in a live doc, such as an article's
/// first draft.
pub fn state_from_text(text: &str) -> Vec<u8> {
    doc_from_text(text)
        .transact()
        .encode_state_as_update_v1(&StateVector::default())
}

/// Pending save entry for debounced persistence
struct PendingSave {
    yjs_state: Vec<u8>,
    /// The doc the state came from, told once the state is saved. Held
    /// strongly: while its state waits here the doc stays reachable through
    /// `DocCache::held`, so every reader and writer of the page finds it
    /// rather than a doc rebuilt from the text the database still has.
    doc: Arc<RwLock<PageDoc>>,
    /// The doc's change count the state includes.
    generation: u64,
    queued_at: Instant,
    /// How many times saving this page has failed. Drives the backoff, and
    /// escalates the log once a transient blip looks like a real fault.
    attempts: u32,
}

impl PendingSave {
    /// Whether this is a state of `doc` at least as new as its state
    /// `generation`, and so carries everything that one does.
    fn covers(&self, doc: &Arc<RwLock<PageDoc>>, generation: u64) -> bool {
        Arc::ptr_eq(&self.doc, doc) && self.generation >= generation
    }
}

/// Debounced save queue - waits for typing to stop before saving
pub struct SaveQueue {
    pending: RwLock<std::collections::HashMap<String, PendingSave>>,
    /// Held across every write of a state to the database (`write`), so
    /// writes land in the order they start and each one sees what the
    /// writes before it marked saved.
    writing: tokio::sync::Mutex<()>,
}

impl SaveQueue {
    pub fn new() -> Self {
        Self {
            pending: RwLock::new(std::collections::HashMap::new()),
            writing: tokio::sync::Mutex::new(()),
        }
    }

    /// Queue `doc`'s state, which includes its first `generation` changes,
    /// in place of an older state of the same doc: the doc's newest state
    /// carries every older one.
    ///
    /// Callers queue after they let go of the doc's lock, and some await the
    /// database in between (an editor's socket records the human edit
    /// first), so states of one doc can arrive here out of order. One that
    /// is no newer than the state already queued for that doc is dropped:
    /// queued in its place, it would be written instead of the newer state
    /// (a machine edit whose save failed, say), which would then reach the
    /// database only with the doc's next change. A state of a different doc
    /// for the page always replaces what is queued.
    async fn queue_save(
        &self,
        page_id: String,
        doc: &Arc<RwLock<PageDoc>>,
        yjs_state: Vec<u8>,
        generation: u64,
    ) {
        let mut pending = self.pending.write().await;
        if pending.get(&page_id).is_some_and(|queued| queued.covers(doc, generation)) {
            return;
        }
        pending.insert(
            page_id,
            PendingSave {
                yjs_state,
                doc: doc.clone(),
                generation,
                queued_at: Instant::now(),
                attempts: 0,
            },
        );
    }

    /// Take `doc`'s state `generation` out of the queue, once a direct
    /// write has saved it, unless something newer has replaced it there.
    async fn dequeue(&self, page_id: &str, doc: &Arc<RwLock<PageDoc>>, generation: u64) {
        let mut pending = self.pending.write().await;
        if pending
            .get(page_id)
            .is_some_and(|queued| Arc::ptr_eq(&queued.doc, doc) && queued.generation == generation)
        {
            pending.remove(page_id);
        }
    }

    /// Put back a state whose save failed, for the loop to retry.
    ///
    /// Something queued for the page while the save was being attempted is
    /// kept instead: a newer state of the same doc, which already carries
    /// this one, or a state of a doc that replaced it.
    async fn requeue(&self, page_id: String, save: PendingSave) {
        let mut pending = self.pending.write().await;
        let superseded = pending.get(&page_id).is_some_and(|queued| {
            !Arc::ptr_eq(&queued.doc, &save.doc) || queued.covers(&save.doc, save.generation)
        });
        if !superseded {
            pending.insert(page_id, save);
        }
    }

    /// Write one state of `doc` to the page, then tell the doc how much of
    /// it the database has. The one way a state reaches `app_pages`.
    ///
    /// A state older than one of the same doc already written is skipped.
    /// Such a state reached the queue after the newer one had left it (the
    /// socket that sent it awaited the database first), and writing it
    /// would put older text over newer while the doc counts the newer as
    /// saved. Writes are one at a time, so the check sees every write
    /// before it.
    async fn write(
        &self,
        pool: &PgPool,
        page_id: &str,
        doc: &Arc<RwLock<PageDoc>>,
        yjs_state: &[u8],
        generation: u64,
    ) -> Result<(), anyhow::Error> {
        let _one_at_a_time = self.writing.lock().await;
        if generation < doc.read().await.saved {
            return Ok(());
        }
        save_and_materialize(pool, page_id, yjs_state).await?;
        doc.write().await.mark_saved(generation);
        Ok(())
    }

    /// Background task: process saves after 2s of inactivity
    pub async fn process_loop(self: Arc<Self>, pool: PgPool) {
        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;
            self.save_due(&pool, Instant::now()).await;
        }
    }

    /// Save every queued state whose wait is over at `now`, one at a time
    /// and in the order taken, so a page's later state is written after its
    /// earlier one.
    async fn save_due(&self, pool: &PgPool, now: Instant) {
        const DEBOUNCE_DURATION: Duration = Duration::from_secs(2);

        let to_save: Vec<(String, PendingSave)> = {
            let mut pending = self.pending.write().await;
            let due: Vec<String> = pending
                .iter()
                .filter(|(_, save)| {
                    // Back off after a failure, so a database that is briefly
                    // unavailable is not hammered every 500 ms.
                    let wait = DEBOUNCE_DURATION * (1 << save.attempts.min(5));
                    now.saturating_duration_since(save.queued_at) >= wait
                })
                .map(|(page_id, _)| page_id.clone())
                .collect();
            due.into_iter()
                .filter_map(|page_id| pending.remove(&page_id).map(|save| (page_id, save)))
                .collect()
        };

        for (page_id, save) in to_save {
            let written = self
                .write(pool, &page_id, &save.doc, &save.yjs_state, save.generation)
                .await;
            if let Err(e) = written {
                // Put it BACK. This is the owner's only copy.
                //
                // The entry was removed from `pending` before the save was
                // attempted, so a failure used to drop the bytes on the
                // floor with one log line. The editor is a CRDT and keeps
                // showing the text, so nothing looked wrong — until moka
                // evicted the doc ~30 minutes later and the page reverted to
                // its last successful save. A Postgres blip (pool exhausted
                // by the nightly narration, a restart during `virtues
                // upgrade`, an OOM on an SBC) is enough, and the window is
                // exactly when someone is typing.
                let attempts = save.attempts.saturating_add(1);
                if attempts <= 3 {
                    tracing::warn!(page = %page_id, attempts, error = %e,
                        "page save failed; requeued");
                } else {
                    tracing::error!(page = %page_id, attempts, error = %e,
                        "page save still failing - the owner's edits are unsaved");
                }
                let save = PendingSave {
                    queued_at: Instant::now(),
                    attempts,
                    ..save
                };
                self.requeue(page_id, save).await;
            }
        }
    }
}

impl Default for SaveQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply a Yjs update to a document and return the new state for saving
/// This is a synchronous function to avoid Send issues with yrs types
///
/// Returns the full doc state for the debounced save, whether the update
/// actually CHANGED the doc, and the doc's change count that state includes.
/// The distinction matters: a freshly-opened client answers sync step 1 with
/// an empty diff, which applies cleanly but changes nothing — opening a page
/// to read it must not count as editing it.
///
/// A change is new insertions (the state vector moves) or new deletions,
/// which leave the state vector where it was. Both count, so every state
/// that says something new has a higher change count than the states before
/// it, which is what lets the save queue keep the newest (`queue_save`).
fn apply_yjs_update(doc: &mut PageDoc, data: &[u8]) -> Option<(Vec<u8>, bool, u64)> {
    let sv_before = doc.doc.transact().state_vector();
    let deleted = match apply_v1(&doc.doc, data) {
        Ok(deleted) => deleted,
        Err(ApplyError::Decode(_)) => return None,
        Err(ApplyError::Apply(error)) => {
            tracing::error!(%error, "an editor's update did not apply; dropping it");
            return None;
        }
    };
    doc.last_update = Instant::now();

    // Broadcast to other clients (wrapped as y-websocket update message)
    let broadcast_msg = encode_sync_update(data);
    let _ = doc.broadcast_tx.send(broadcast_msg);

    // Edits each inside the contract can merge into a tree page that is
    // not (`admit_update` takes them); the server puts it right at once.
    let repaired = doc.contract() > 0 && doc.repair();

    // Get current state for debounced save
    let (state, changed) = {
        let txn = doc.doc.transact();
        let changed = deleted || repaired || txn.state_vector() != sv_before;
        (virtues_document::encode_state(&txn, &StateVector::default()), changed)
    };
    let generation = if changed { doc.record_change() } else { doc.changes };
    Some((state, changed, generation))
}

/// A doc update arriving over the WebSocket is, by definition, a human edit —
/// the machine's writes go through `YjsState` methods server-side and never
/// traverse a client connection. (Opening a page to read sends an empty diff,
/// which is why the caller checks that the doc actually changed first.)
///
/// This used to CLAIM the article: `auto_update` flipped to false and the
/// record stopped editing it ever again, on the reasoning that an article has
/// exactly one pen so "whose sentence is this" never needs answering. That is
/// the one-pen rule, and it is overruled — almost nobody wants to maintain
/// their own record, and touching one sentence is not a decision to take over
/// a page. Losing the record's maintenance was the price of a typo fix.
///
/// So the same signal now records WHEN rather than deciding WHO: the editor
/// skips an article somebody was in recently, and "whose sentence is this" is
/// answered properly, by diffing the live text against what the editor itself
/// last wrote (`api::wiki_editor::provenance`).
async fn note_human_edit(pool: &PgPool, page_id: &str) {
    if let Err(e) = sqlx::query(
        "UPDATE wiki_articles SET last_human_edit_at = now() WHERE page_id = $1",
    )
    .bind(page_id)
    .execute(pool)
    .await
    {
        tracing::warn!(page_id, error = %e, "could not record a human edit on the article");
    }
}

// ============================================================================
// lib0 VarInt Encoding (used by y-websocket protocol)
// ============================================================================

/// Insert a markdown block at the END of a doc's `content` text.
///
/// Separated from the async append path so the block-separation rules and the
/// merge (no-clobber) property are unit-testable without a pool or doc cache.
/// A blank line is inserted unless the doc is empty or already ends in one.
fn append_block_to_doc(doc: &Doc, markdown: &str) {
    let mut txn = doc.transact_mut();
    let text = txn.get_or_insert_text("content");
    let len = text.len(&txn);
    let current = text.get_string(&txn);

    let prefix = if current.is_empty() {
        ""
    } else if current.ends_with("\n\n") {
        ""
    } else if current.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };

    text.insert(&mut txn, len, &format!("{prefix}{markdown}"));
    // txn commits on drop
}

/// Write a variable-length unsigned integer (lib0 format)
fn write_var_uint(buf: &mut Vec<u8>, mut value: usize) {
    while value > 0x7f {
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

/// Read a variable-length unsigned integer, returning (value, bytes_consumed)
fn read_var_uint(data: &[u8]) -> Option<(usize, usize)> {
    let mut value: usize = 0;
    let mut shift = 0;
    let mut pos = 0;

    loop {
        if pos >= data.len() {
            return None;
        }
        let byte = data[pos];
        value |= ((byte & 0x7f) as usize) << shift;
        pos += 1;

        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    Some((value, pos))
}

/// Write a length-prefixed byte array (lib0 VarUint8Array format)
fn write_var_uint8_array(buf: &mut Vec<u8>, data: &[u8]) {
    write_var_uint(buf, data.len());
    buf.extend_from_slice(data);
}

/// Read a length-prefixed byte array, returning the data slice and bytes consumed
fn read_var_uint8_array(data: &[u8]) -> Option<(&[u8], usize)> {
    let (len, header_size) = read_var_uint(data)?;
    let total_size = header_size + len;
    if data.len() < total_size {
        return None;
    }
    Some((&data[header_size..total_size], total_size))
}

// ============================================================================
// y-websocket Protocol Helpers
// ============================================================================

/// Encode a sync step 1 message (state vector request)
/// Format: [MSG_SYNC][MSG_SYNC_STEP1][varint length][state_vector bytes]
fn encode_sync_step1(state_vector: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(2 + 5 + state_vector.len()); // 5 bytes max for varint
    msg.push(MSG_SYNC);
    msg.push(MSG_SYNC_STEP1);
    write_var_uint8_array(&mut msg, state_vector);
    msg
}

/// Encode a sync step 2 message (response with missing updates)
/// Format: [MSG_SYNC][MSG_SYNC_STEP2][varint length][update bytes]
fn encode_sync_step2(update: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(2 + 5 + update.len());
    msg.push(MSG_SYNC);
    msg.push(MSG_SYNC_STEP2);
    write_var_uint8_array(&mut msg, update);
    msg
}

/// Encode an incremental update message
/// Format: [MSG_SYNC][MSG_SYNC_UPDATE][varint length][update bytes]
fn encode_sync_update(update: &[u8]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(2 + 5 + update.len());
    msg.push(MSG_SYNC);
    msg.push(MSG_SYNC_UPDATE);
    write_var_uint8_array(&mut msg, update);
    msg
}

/// Parse a y-websocket message, returning (message_type, payload)
fn parse_message(data: &[u8]) -> Option<(u8, &[u8])> {
    if data.is_empty() {
        return None;
    }
    Some((data[0], &data[1..]))
}

/// Parse a sync message, returning (sync_type, raw_payload_with_length_prefix)
fn parse_sync_message(data: &[u8]) -> Option<(u8, &[u8])> {
    if data.is_empty() {
        return None;
    }
    Some((data[0], &data[1..]))
}

/// Extract the actual data from a length-prefixed sync payload
fn extract_sync_payload(data: &[u8]) -> Option<&[u8]> {
    let (payload, _) = read_var_uint8_array(data)?;
    Some(payload)
}

/// The `content` text of a saved doc state (`app_pages.yjs_state`,
/// `app_page_versions.yjs_snapshot`), or why it could not be read.
pub(crate) fn text_of_state(yjs_state: &[u8]) -> Result<String, ApplyError> {
    let doc = new_doc();
    apply_v1(&doc, yjs_state)?;
    let text = content_of(&doc.transact());
    Ok(text)
}

/// The `content` text of a saved doc state. Empty when the bytes are not a
/// state or the state does not apply.
pub fn extract_text_content(yjs_state: &[u8]) -> String {
    match text_of_state(yjs_state) {
        Ok(text) => text,
        Err(ApplyError::Decode(_)) => String::new(),
        Err(ApplyError::Apply(error)) => {
            tracing::error!(%error, "a saved page state did not apply; reading it as empty");
            String::new()
        }
    }
}

/// Save Yjs state and materialize content for search
async fn save_and_materialize(
    pool: &PgPool,
    page_id: &str,
    yjs_state: &[u8],
) -> Result<(), anyhow::Error> {
    // Extract text content from yrs
    let content = extract_text_content(yjs_state);

    // Save both yjs_state and materialized content
    sqlx::query(
        "UPDATE app_pages SET yjs_state = $1, content = $2, updated_at = now() WHERE id = $3",
    )
    .bind(yjs_state)
    .bind(&content)
    .bind(page_id)
    .execute(pool)
    .await?;

    tracing::debug!("Saved page {} ({} chars)", page_id, content.len());
    Ok(())
}

/// Why a server-side text write (`apply_text_edit`, `apply_text_diff`,
/// `replace_text`) did not simply land. The variants differ in what the page
/// says afterwards, which is the thing a caller has to report honestly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextWriteError {
    /// The page no longer says what the caller read before it started, so
    /// nothing was changed.
    Stale,
    /// The change is on the page and went out to every open editor, but
    /// saving it failed. The state is queued and the save loop retries it;
    /// the queue holds this doc (`PendingSave::doc`), so it stays the page's
    /// until the state lands. An applied edit: a caller versions `written`
    /// and reports the pending save, and never makes the edit again.
    NotSaved { written: Written, error: String },
    /// Nothing was changed, for the reason given: the page's document could
    /// not be loaded, or the text a find/replace looks for is not on it.
    Other(String),
}

impl std::fmt::Display for TextWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stale => f.write_str(
                "the page changed while your edit was in flight; read it again and retry",
            ),
            Self::NotSaved { error, .. } => write!(
                f,
                "the edit is on the page, but your server couldn't save it yet and saves it again on its own: {error}"
            ),
            Self::Other(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for TextWriteError {}

/// The unit a text write diffs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Granularity {
    /// For revising prose in place: a changed word stays a changed word, so
    /// History shows the edit and a concurrent edit elsewhere in the sentence
    /// survives.
    Words,
    /// For replacing a whole page. A word diff between two independent drafts
    /// is one CRDT operation per token (about ten thousand for a day page) and
    /// leaves the stored state several times larger for good; a line diff
    /// gives the same text in a few hundred.
    Lines,
}

/// The edits that turn `old` into `new`, as (byte offset into `old`, bytes
/// to delete, text to insert), in ascending offset order.
///
/// Offsets are BYTES, the unit every server doc counts in (`new_doc`).
/// Applied last-first, every edit lands at an offset that nothing before it
/// in the text has moved.
fn text_ops(old: &str, new: &str, granularity: Granularity) -> Vec<(u32, u32, String)> {
    use similar::{ChangeTag, TextDiff};

    let diff = match granularity {
        Granularity::Words => TextDiff::from_words(old, new),
        Granularity::Lines => TextDiff::from_lines(old, new),
    };
    let mut ops = Vec::new();
    let mut at = 0usize;
    for change in diff.iter_all_changes() {
        let v = change.value();
        match change.tag() {
            ChangeTag::Equal => at += v.len(),
            ChangeTag::Delete => {
                ops.push((at as u32, v.len() as u32, String::new()));
                at += v.len();
            }
            ChangeTag::Insert => ops.push((at as u32, 0, v.to_string())),
        }
    }
    ops
}

/// Apply `text_ops` output to a doc's `content` text, last edit first.
fn apply_text_ops(txn: &mut yrs::TransactionMut, text: &yrs::TextRef, ops: Vec<(u32, u32, String)>) {
    for (start, del, ins) in ops.into_iter().rev() {
        if del > 0 {
            text.remove_range(txn, start, del);
        }
        if !ins.is_empty() {
            text.insert(txn, start, &ins);
        }
    }
}

/// Machine writes to one page take turns, from the copy kept before a write
/// to the version cut after it. Two at once would each read the other's change
/// as typing that landed in between, and version it as the owner's.
#[derive(Default)]
pub struct WriteTurns(std::sync::Mutex<std::collections::HashMap<String, Weak<tokio::sync::Mutex<()>>>>);

impl WriteTurns {
    async fn take(&self, page_id: &str) -> tokio::sync::OwnedMutexGuard<()> {
        let turn = {
            let mut turns = self.0.lock().unwrap_or_else(|e| e.into_inner());
            turns.retain(|_, t| t.strong_count() > 0);
            match turns.get(page_id).and_then(Weak::upgrade) {
                Some(turn) => turn,
                None => {
                    let turn = Arc::new(tokio::sync::Mutex::new(()));
                    turns.insert(page_id.to_string(), Arc::downgrade(&turn));
                    turn
                }
            }
        };
        turn.lock_owned().await
    }
}

/// Shared state for Yjs WebSocket connections
#[derive(Clone)]
pub struct YjsState {
    pub doc_cache: Arc<DocCache>,
    pub save_queue: Arc<SaveQueue>,
    pub write_turns: Arc<WriteTurns>,
    pub pool: PgPool,
}

impl YjsState {
    pub fn new(pool: PgPool) -> Self {
        Self {
            doc_cache: Arc::new(DocCache::new()),
            save_queue: Arc::new(SaveQueue::new()),
            write_turns: Arc::new(WriteTurns::default()),
            pool,
        }
    }

    /// This page's turn for a machine write; held until the write's version is cut.
    pub async fn write_turn(&self, page_id: &str) -> tokio::sync::OwnedMutexGuard<()> {
        self.write_turns.take(page_id).await
    }

    /// Start the background save queue processor
    pub fn start_save_processor(&self) {
        let save_queue = self.save_queue.clone();
        let pool = self.pool.clone();
        tokio::spawn(async move {
            save_queue.process_loop(pool).await;
        });
    }

    /// Write every queued edit now, ignoring the debounce.
    ///
    /// Called on shutdown. Without it, SIGTERM discards whatever is inside the
    /// debounce window — which means every `systemctl restart virtues` and every
    /// self-update silently drops the last couple of seconds of the owner's
    /// typing. `server/mod.rs` used to carry a comment saying no flush was
    /// needed on shutdown; that was true of the old StreamWriter and was never
    /// true of this queue.
    ///
    /// Best-effort by design: a failure here has nowhere left to go, since the
    /// process is exiting. It is logged at error so the next boot's journal
    /// says what was lost.
    pub async fn flush_pending_saves(&self) {
        let drained: Vec<(String, PendingSave)> = {
            let mut pending = self.save_queue.pending.write().await;
            pending.drain().collect()
        };
        if drained.is_empty() {
            return;
        }
        tracing::info!(pages = drained.len(), "flushing unsaved page edits before shutdown");
        for (page_id, save) in drained {
            let written = self
                .save_queue
                .write(&self.pool, &page_id, &save.doc, &save.yjs_state, save.generation)
                .await;
            if let Err(e) = written {
                tracing::error!(page = %page_id, error = %e,
                    "could not flush page on shutdown - these edits are lost");
            }
        }
    }

    /// The document contract the page is written under, as its socket binds
    /// it (`PageDoc::contract`): 0 for a markdown page. The editor reads it
    /// before it shows the copy of the page it kept on the device, which may
    /// be from before the page was raised.
    pub async fn page_contract(&self, page_id: &str) -> anyhow::Result<u32> {
        let page_doc = self.doc_cache.get_or_create(page_id, &self.pool).await?;
        let contract = page_doc.read().await.contract();
        Ok(contract)
    }

    /// Raise the page's document to this server's contract version
    /// (`PageDoc::raise_contract`): stamped, saved at once, and every socket
    /// bound below the new version closed. Nothing happens when the page is
    /// already at it.
    pub async fn raise_contract(&self, page_id: &str) -> Result<(), TextWriteError> {
        let page_doc = self
            .doc_cache
            .get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| TextWriteError::Other(format!("Failed to get page document: {e}")))?;
        let raised = {
            let mut doc = page_doc.write().await;
            doc.raise_contract().map(|generation| (doc.written(), generation))
        };
        if let Some((written, generation)) = raised {
            self.persist_now(page_id, &page_doc, written, generation).await?;
        }
        Ok(())
    }

    /// Save a machine edit now, ignoring the debounce, and hand back what was
    /// written: as `Ok`, or inside `NotSaved` when the save failed.
    ///
    /// The debounce is right for a person typing and wrong for a machine edit
    /// the caller is about to record elsewhere (a version, an edition), and
    /// for a reader of the database such as `virtues page get` straight after
    /// `virtues page edit`.
    ///
    /// No older state of this doc is saved over this one. The state is
    /// queued before the direct write, as any edit is, which supersedes an
    /// older one still waiting (an open editor's keystrokes from just
    /// before) and drops one that arrives later while this one waits
    /// (`queue_save`). Writes are one at a time and skip a state older than
    /// one already written (`SaveQueue::write`), which covers an older state
    /// the loop had in flight and one queued after this one has landed.
    ///
    /// Once the direct write lands, the state leaves the queue unless a
    /// newer one has replaced it. On failure it stays, for the loop to retry.
    async fn persist_now(
        &self,
        page_id: &str,
        page_doc: &Arc<RwLock<PageDoc>>,
        written: Written,
        generation: u64,
    ) -> Result<Written, TextWriteError> {
        self.save_queue
            .queue_save(page_id.to_string(), page_doc, written.state.clone(), generation)
            .await;
        let landed = self
            .save_queue
            .write(&self.pool, page_id, page_doc, &written.state, generation)
            .await;
        match landed {
            Ok(()) => {
                self.save_queue.dequeue(page_id, page_doc, generation).await;
                Ok(written)
            }
            Err(e) => {
                tracing::warn!(page = %page_id, error = %e, "machine edit not saved; queued for retry");
                Err(TextWriteError::NotSaved {
                    written,
                    error: e.to_string(),
                })
            }
        }
    }

    /// Apply a markdown text edit to a page through Yjs (Y.Text).
    ///
    /// The document IS markdown, so find/replace operates directly on the text.
    /// No XML tree walking, no markdown↔XML conversion needed.
    ///
    /// - `find`: markdown text to locate (empty = full document replacement)
    /// - `replace`: markdown replacement text
    ///
    /// Saved immediately (`persist_now`). Returns what the page said just
    /// before the edit and what the edit left on it, both read under the
    /// lock it was made under, for the caller's versions (`TextEdit`). Text
    /// that is not on the page is `Other`, with nothing changed.
    pub async fn apply_text_edit(
        &self,
        page_id: &str,
        find: &str,
        replace: &str,
    ) -> Result<TextEdit, TextWriteError> {
        let page_doc = self
            .doc_cache
            .get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| TextWriteError::Other(format!("Failed to get page document: {e}")))?;

        let (before, written, generation) = {
            let mut doc = page_doc.write().await;
            let before = doc.written();

            {
                let mut txn = doc.doc.transact_mut();
                let text = txn.get_or_insert_text("content");

                if find.is_empty() {
                    // Full document replacement
                    let len = text.len(&txn);
                    if len > 0 {
                        text.remove_range(&mut txn, 0, len);
                    }
                    text.insert(&mut txn, 0, replace);
                } else {
                    // Find/replace within the markdown text
                    let current = text.get_string(&txn);

                    if let Some(byte_offset) = current.find(find) {
                        // Bytes, the unit the doc counts in (`new_doc`).
                        let start = byte_offset as u32;
                        let len = find.len() as u32;

                        text.remove_range(&mut txn, start, len);
                        text.insert(&mut txn, start, replace);
                    } else {
                        return Err(TextWriteError::Other(format!(
                            "Text not found in page: '{}'",
                            if find.chars().count() > 50 {
                                format!("{}...", find.chars().take(50).collect::<String>())
                            } else {
                                find.to_string()
                            }
                        )));
                    }
                }
                // txn commits on drop
            }

            let generation = doc.record_change();
            let written = doc.written();
            let _ = doc.broadcast_tx.send(encode_sync_update(&written.state));
            (before, written, generation)
        };

        let after = self.persist_now(page_id, &page_doc, written, generation).await;
        Ok(TextEdit { before, after })
    }

    /// The page's current markdown, straight from the authoritative doc.
    ///
    /// The editor reads this to build its prompt and hands the same string
    /// back as `expected_old`, which is what makes the staleness guard in
    /// `apply_text_diff` meaningful.
    pub async fn read_text(&self, page_id: &str) -> Result<String, String> {
        let page_doc = self
            .doc_cache
            .get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| format!("Failed to get page document: {e}"))?;
        let doc = page_doc.read().await;
        let text = content_of(&doc.doc.transact());
        Ok(text)
    }

    /// The page's markdown and the doc's full state, read under one lock so
    /// the two agree: a version cut from them says exactly what it stores.
    ///
    /// The state is a self-contained update rather than a yrs Snapshot, which
    /// is what lets a version (`app_page_versions.yjs_snapshot`) be decoded
    /// on its own without `skip_gc`.
    pub async fn text_and_state(&self, page_id: &str) -> Result<Written, String> {
        let page_doc = self
            .doc_cache
            .get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| format!("Failed to get page document: {e}"))?;
        let written = page_doc.read().await.written();
        Ok(written)
    }

    /// Revise a page's prose by applying only what actually changed, word by
    /// word.
    ///
    /// The editor returns a whole article — a batch job has no turn in which
    /// to retry a failed find/replace, and asking a model for exact anchor
    /// strings in a two-thousand-token document is the known way to lose an
    /// edit. So the model writes the document and the SERVER works out the
    /// edit, which is the opposite of `apply_text_edit` and deliberately so.
    ///
    /// Why not simply replace the text: in a CRDT a full replace is delete-all
    /// plus insert-all, which discards any concurrent human edit by
    /// construction and makes every revision diff at 100%, so History shows
    /// "everything changed" every time — the same as showing nothing.
    ///
    /// `expected_old` is the text the caller read before it started. If the
    /// document has moved since (somebody typed while the model was thinking),
    /// the edit is refused with `Stale` rather than applied to text it was not
    /// written against. The next run picks it up.
    ///
    /// Returns what the edit left on the page, read under the same lock as
    /// the edit; `NotSaved` carries the same.
    pub async fn apply_text_diff(
        &self,
        page_id: &str,
        expected_old: &str,
        new_text: &str,
    ) -> Result<Written, TextWriteError> {
        self.write_text(page_id, expected_old, new_text, Granularity::Words)
            .await
    }

    /// Replace a whole page: a new draft, or an earlier version put back.
    ///
    /// The same write as `apply_text_diff` (only what changed, through the
    /// CRDT, refused with `Stale` when the page has moved off `expected_old`),
    /// diffed by LINE. Two independent drafts share common words all the way
    /// through, so a word diff between them interleaves thousands of tiny
    /// operations and leaves a much larger stored state for the same text.
    ///
    /// Returns what the edit left on the page, read under the same lock as
    /// the edit; `NotSaved` carries the same.
    pub async fn replace_text(
        &self,
        page_id: &str,
        expected_old: &str,
        new_text: &str,
    ) -> Result<Written, TextWriteError> {
        self.write_text(page_id, expected_old, new_text, Granularity::Lines)
            .await
    }

    async fn write_text(
        &self,
        page_id: &str,
        expected_old: &str,
        new_text: &str,
        granularity: Granularity,
    ) -> Result<Written, TextWriteError> {
        // Worked out before the lock: open editors wait on it, and the diff is
        // the expensive part. It is only valid against `expected_old`, which
        // the check under the lock confirms.
        let ops = text_ops(expected_old, new_text, granularity);

        let page_doc = self
            .doc_cache
            .get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| TextWriteError::Other(format!("Failed to get page document: {e}")))?;

        let (written, generation) = {
            let mut doc = page_doc.write().await;
            {
                let mut txn = doc.doc.transact_mut();
                let text = txn.get_or_insert_text("content");
                if text.get_string(&txn) != expected_old {
                    return Err(TextWriteError::Stale);
                }
                apply_text_ops(&mut txn, &text, ops);
                // txn commits on drop
            }

            let generation = doc.record_change();
            let written = doc.written();
            let _ = doc.broadcast_tx.send(encode_sync_update(&written.state));
            (written, generation)
        };

        self.persist_now(page_id, &page_doc, written, generation).await
    }

    /// Append a markdown block to the end of a page, through Yjs.
    ///
    /// This is the safe way to send content into a page that may be OPEN in an
    /// editor: the insert happens inside a yrs transaction on the authoritative
    /// doc and is broadcast to every connected client, so an open editor merges
    /// it live instead of being clobbered by a blind content replace.
    ///
    /// Block separation is handled here — a blank line is inserted when the
    /// document is non-empty so appended markdown blocks never run together.
    pub async fn append_markdown(&self, page_id: &str, markdown: &str) -> Result<String, String> {
        let page_doc = self
            .doc_cache
            .get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| format!("Failed to get page document: {}", e))?;

        let (written, generation) = {
            let mut doc = page_doc.write().await;

            append_block_to_doc(&doc.doc, markdown);
            let generation = doc.record_change();
            let written = doc.written();
            let _ = doc.broadcast_tx.send(encode_sync_update(&written.state));
            (written, generation)
        };

        self.save_queue
            .queue_save(page_id.to_string(), &page_doc, written.state, generation)
            .await;

        Ok(written.text)
    }

    /// Get the current content of a page from Yjs as markdown.
    ///
    /// The document IS markdown (Y.Text), so this is a simple text read.
    pub async fn get_page_content(&self, page_id: &str) -> Result<String, String> {
        let page_doc = self.doc_cache.get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| format!("Failed to get page document: {}", e))?;

        let doc = page_doc.read().await;
        let text = content_of(&doc.doc.transact());
        Ok(text)
    }
}

/// How a socket is bound to a page's doc, from the client's contract version
/// (`?contract=N`; a client that sends none is 0, which is every client that
/// binds a page's Y.Text) and the document's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Binding {
    /// The client reads everything the doc can hold, and the server can check
    /// everything the client writes.
    ReadWrite,
    /// The client is newer than this server: it reads the doc, but what it
    /// writes could hold what this server cannot check, so it is not applied.
    ReadOnly,
}

/// Bind a client at `client` to a doc at `document`, or why not. A client
/// below the document's version is refused, because Tiptap's binding deletes
/// from the shared document every node its schema cannot read
/// (`apps/web/src/lib/document/skew.test.ts`).
fn bind(client: u32, document: u32) -> Result<Binding, String> {
    if client < document {
        return Err(format!(
            "this page needs a newer version of the app (contract {document}; the app reads {client})"
        ));
    }
    if client > virtues_document::contract().version {
        return Ok(Binding::ReadOnly);
    }
    Ok(Binding::ReadWrite)
}

/// The client's contract version from the socket's query (`?contract=N`).
/// Absent or unreadable is 0: the oldest client, refused by every tree page.
fn client_contract(params: &HashMap<String, String>) -> u32 {
    params
        .get("contract")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// Whether an update from a client may be applied to `doc`, and if not, why.
/// A tree page's update is checked against the contract on a copy of the doc
/// first (`virtues_document::check_update`): once applied it would be
/// broadcast, and a stale peer would act on it before any later check.
///
/// A markdown page (contract 0) has no contract to check its text against,
/// but its `meta` is the server's all the same: the stamp there decides the
/// page's contract when it is next loaded (`PageDoc::new`), so a client that
/// wrote one would shut every other client out. An update that does not
/// decode or apply is left to `apply_yjs_update`, which drops it, as it
/// always has on a markdown page.
fn admit_update(doc: &PageDoc, update: &[u8]) -> Result<(), String> {
    if doc.contract() == 0 {
        // The check only reads the doc, so a panic inside it leaves nothing
        // half done.
        let writes = std::panic::catch_unwind(AssertUnwindSafe(|| {
            virtues_document::changes_meta(&doc.doc, update)
        }));
        return match writes {
            Ok(Ok(true)) => Err("the update writes `meta`, which only the server writes".into()),
            _ => Ok(()),
        };
    }
    let problems = virtues_document::check_update(&doc.doc, update)
        .map_err(|e| format!("the update does not apply: {e}"))?;
    match problems.first() {
        None => Ok(()),
        Some(first) => Err(format!(
            "the update is outside the document contract: {} ({}){}",
            first.message,
            first.at,
            match problems.len() {
                1 => String::new(),
                n => format!(", and {} more", n - 1),
            }
        )),
    }
}

/// Apply an editor's update to the page's doc, record the human edit, and
/// queue the save. An update a tree page's contract refuses is not applied
/// or relayed, and comes back as the reason, for the socket to close on.
async fn take_client_update(
    state: &YjsState,
    page_id: &str,
    page_doc: &Arc<RwLock<PageDoc>>,
    update: &[u8],
) -> Result<(), String> {
    let mut doc = page_doc.write().await;
    // Checked under the same lock it is applied under: nothing lands between.
    admit_update(&doc, update)?;
    let Some((full_state, changed, generation)) = apply_yjs_update(&mut doc, update) else {
        return Ok(());
    };
    drop(doc);
    // Every changing update, not once per cached doc.
    // The stamp is "when were they last in here", which
    // the editor reads to stay out of a page somebody is
    // working in — so a memo that fired once per cached
    // doc would report a session's first keystroke as its
    // last, and let the editor in while they typed.
    if changed {
        note_human_edit(&state.pool, page_id).await;
    }
    state
        .save_queue
        .queue_save(page_id.to_string(), page_doc, full_state, generation)
        .await;
    Ok(())
}

/// Close a socket with a code and a reason. A close frame's reason holds at
/// most 123 bytes.
async fn close_with(socket: &mut WebSocket, code: u16, reason: &str) {
    let mut end = reason.len().min(123);
    while !reason.is_char_boundary(end) {
        end -= 1;
    }
    let frame = CloseFrame {
        code,
        reason: reason[..end].to_string().into(),
    };
    let _ = socket.send(Message::Close(Some(frame))).await;
}

/// WebSocket upgrade handler for Yjs sync. The client says which document
/// contract it reads as `?contract=N` (`apps/web/src/lib/yjs/document.ts`).
pub async fn yjs_websocket_handler(
    ws: WebSocketUpgrade,
    Path(page_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<YjsState>,
) -> Response {
    let contract = client_contract(&params);
    ws.on_upgrade(move |socket| handle_yjs_connection(socket, page_id, contract, state))
}

/// Handle a single WebSocket connection for Yjs sync
/// Implements the y-websocket protocol
async fn handle_yjs_connection(
    mut socket: WebSocket,
    page_id: String,
    client: u32,
    state: YjsState,
) {
    tracing::debug!("WebSocket connection opened for page {}", page_id);

    // Get or create the document
    let page_doc = match state.doc_cache.get_or_create(&page_id, &state.pool).await {
        Ok(doc) => doc,
        Err(e) => {
            tracing::error!("Failed to load page {}: {}", page_id, e);
            let _ = socket.close().await;
            return;
        }
    };

    // Subscribe to broadcasts from other clients, and to the doc's contract,
    // both before binding: a raise between the two is still seen.
    let (mut broadcast_rx, mut contract_rx) = {
        let doc = page_doc.read().await;
        (doc.broadcast_tx.subscribe(), doc.contract.subscribe())
    };
    let document = *contract_rx.borrow_and_update();
    let binding = match bind(client, document) {
        Ok(binding) => binding,
        Err(reason) => {
            tracing::info!(page_id, client, %reason, "refusing a client below the page's contract");
            close_with(&mut socket, CLOSE_CONTRACT, &reason).await;
            return;
        }
    };

    loop {
        tokio::select! {
            // Client sent message
            Some(msg) = socket.recv() => {
                match msg {
                    Ok(Message::Binary(data)) => {
                        // Parse y-websocket message type
                        let Some((msg_type, payload)) = parse_message(&data) else {
                            tracing::warn!("Received empty message from client");
                            continue;
                        };

                        match msg_type {
                            MSG_SYNC => {
                                // Parse sync subtype
                                let Some((sync_type, sync_payload)) = parse_sync_message(payload) else {
                                    tracing::warn!("Received empty sync message");
                                    continue;
                                };

                                match sync_type {
                                    MSG_SYNC_STEP1 => {
                                        // Client is sending their state vector, requesting missing updates
                                        // Extract the actual state vector from VarUint8Array format
                                        let sv_bytes = match extract_sync_payload(sync_payload) {
                                            Some(bytes) => bytes,
                                            None => {
                                                tracing::warn!("Failed to extract state vector from sync step 1");
                                                continue;
                                            }
                                        };

                                        // Parse client's state vector and send back missing updates
                                        let response = {
                                            let doc = page_doc.read().await;
                                            let txn = doc.doc.transact();
                                            
                                            // Decode client's state vector
                                            let client_sv = match StateVector::decode_v1(sv_bytes) {
                                                Ok(sv) => sv,
                                                Err(e) => {
                                                    tracing::warn!("Failed to decode state vector: {}", e);
                                                    StateVector::default()
                                                }
                                            };
                                            
                                            // Encode updates the client is missing
                                            let update = virtues_document::encode_state(&txn, &client_sv);
                                            encode_sync_step2(&update)
                                        };
                                        
                                        if socket.send(Message::Binary(response)).await.is_err() {
                                            break;
                                        }

                                        // Also send our state vector so client can send us their updates
                                        let sv_msg = {
                                            let doc = page_doc.read().await;
                                            let txn = doc.doc.transact();
                                            let sv = txn.state_vector().encode_v1();
                                            encode_sync_step1(&sv)
                                        };
                                        
                                        if socket.send(Message::Binary(sv_msg)).await.is_err() {
                                            break;
                                        }
                                    }
                                    MSG_SYNC_STEP2 => {
                                        // Client is responding to our state vector request with their updates
                                        // Extract the actual update from VarUint8Array format
                                        let update_bytes = match extract_sync_payload(sync_payload) {
                                            Some(bytes) => bytes,
                                            None => {
                                                tracing::warn!("Failed to extract update from sync step 2");
                                                continue;
                                            }
                                        };
                                        if let Err(reason) = take_update(binding, &state, &page_id, &page_doc, update_bytes).await {
                                            close_with(&mut socket, CLOSE_REFUSED, &reason).await;
                                            break;
                                        }
                                    }
                                    MSG_SYNC_UPDATE => {
                                        // Client is sending an incremental update
                                        // Extract the actual update from VarUint8Array format
                                        let update_bytes = match extract_sync_payload(sync_payload) {
                                            Some(bytes) => bytes,
                                            None => {
                                                tracing::warn!("Failed to extract update from sync update");
                                                continue;
                                            }
                                        };
                                        if let Err(reason) = take_update(binding, &state, &page_id, &page_doc, update_bytes).await {
                                            close_with(&mut socket, CLOSE_REFUSED, &reason).await;
                                            break;
                                        }
                                    }
                                    _ => {
                                        tracing::warn!("Unknown sync type: {}", sync_type);
                                    }
                                }
                            }
                            MSG_AWARENESS => {
                                // Awareness updates (cursor positions, etc.)
                                // For now, just broadcast to other clients
                                let doc = page_doc.read().await;
                                let _ = doc.broadcast_tx.send(data.clone());
                            }
                            _ => {
                                tracing::warn!("Unknown message type: {}", msg_type);
                            }
                        }
                    }
                    Ok(Message::Close(_)) => break,
                    Err(_) => break,
                    _ => {} // Ignore text/ping/pong
                }
            }
            // Broadcast from another client (already wrapped in y-websocket format)
            Ok(update) = broadcast_rx.recv() => {
                if socket.send(Message::Binary(update)).await.is_err() {
                    break;
                }
            }
            // The server raised the doc's contract.
            Ok(()) = contract_rx.changed() => {
                let document = *contract_rx.borrow_and_update();
                if let Err(reason) = bind(client, document) {
                    tracing::info!(page_id, client, document, "closing a client below the page's raised contract");
                    close_with(&mut socket, CLOSE_CONTRACT, &reason).await;
                    break;
                }
            }
            else => break,
        }
    }

    tracing::debug!("WebSocket connection closed for page {}", page_id);
}

/// A client's update, as its binding allows: a read-only client's is not
/// applied. Refused comes back as the reason.
async fn take_update(
    binding: Binding,
    state: &YjsState,
    page_id: &str,
    page_doc: &Arc<RwLock<PageDoc>>,
    update: &[u8],
) -> Result<(), String> {
    match binding {
        Binding::ReadWrite => {
            let taken = take_client_update(state, page_id, page_doc, update).await;
            if let Err(reason) = &taken {
                tracing::warn!(page_id, %reason, "refused an editor's update");
            }
            taken
        }
        Binding::ReadOnly => {
            tracing::debug!(page_id, "dropped an update from a client newer than this server");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::pages;
    use yrs::Update;

    /// Helper: create a Y.Text doc with markdown content
    fn setup_doc(content: &str) -> Doc {
        doc_from_text(content)
    }

    /// Helper: read Y.Text content from a doc
    fn read_content(doc: &Doc) -> String {
        content_of(&doc.transact())
    }

    // ── the claim signal: only a CHANGING update counts as an edit ──────────

    /// Helper: a PageDoc around an existing Doc, for apply_yjs_update.
    fn page_doc(doc: Doc) -> PageDoc {
        PageDoc::new(doc, chrono::Utc::now())
    }

    /// Opening a page must never claim it. A freshly-synced client answers
    /// the server's state vector with a diff of everything the server lacks —
    /// which, on open, is NOTHING, so the update applies cleanly and changes
    /// nothing. `changed` is the entire basis of the article-claim flip, so
    /// this pins both directions: a no-op replay is not an edit, a real
    /// insertion is.
    #[test]
    fn a_noop_update_is_not_an_edit_and_a_real_one_is() {
        let server = setup_doc("The record's prose.");
        let mut page = page_doc(server);

        // A second client that has synced the same state.
        let client = new_doc();
        {
            let state = page.doc.transact().encode_state_as_update_v1(&yrs::StateVector::default());
            let mut txn = client.transact_mut();
            txn.apply_update(Update::decode_v1(&state).unwrap()).unwrap();
        }

        // Replaying the shared state back at the server: applies, changes nothing.
        let replay = client.transact().encode_state_as_update_v1(&yrs::StateVector::default());
        let (_, changed, generation) = apply_yjs_update(&mut page, &replay).expect("decodable");
        assert!(!changed, "an update carrying nothing new must not read as an edit");
        assert_eq!(generation, 0, "and is no change to save");

        // A real edit on the client → the diff the server lacks → an edit.
        {
            let mut txn = client.transact_mut();
            let text = txn.get_or_insert_text("content");
            let len = text.len(&txn);
            text.insert(&mut txn, len, " Your line.");
        }
        let sv = page.doc.transact().state_vector();
        let diff = client.transact().encode_state_as_update_v1(&sv);
        let (_, changed, generation) = apply_yjs_update(&mut page, &diff).expect("decodable");
        assert!(changed, "a real insertion must read as an edit");
        assert_eq!(generation, 1);
        assert!(page.has_unsaved_changes());
        assert_eq!(read_content(&page.doc), "The record's prose. Your line.");
    }

    /// Deleting text moves no state vector, and is a change all the same:
    /// its state gets a new count, so the save queue cannot keep an older
    /// state of the doc in its place.
    #[test]
    fn a_deletion_alone_is_a_change() {
        let mut page = page_doc(setup_doc("Keep this. Cut this."));
        let client = new_doc();
        {
            let state = page.doc.transact().encode_state_as_update_v1(&yrs::StateVector::default());
            let mut txn = client.transact_mut();
            txn.apply_update(Update::decode_v1(&state).unwrap()).unwrap();
        }
        let sv = client.transact().state_vector();
        {
            let mut txn = client.transact_mut();
            let text = txn.get_or_insert_text("content");
            text.remove_range(&mut txn, "Keep this.".len() as u32, " Cut this.".len() as u32);
        }
        let diff = client.transact().encode_state_as_update_v1(&sv);

        let (_, changed, generation) = apply_yjs_update(&mut page, &diff).expect("decodable");
        assert!(changed, "a deletion is an edit");
        assert_eq!(generation, 1);
        assert_eq!(read_content(&page.doc), "Keep this.");

        // The same deletion again deletes nothing new.
        let (_, changed, generation) = apply_yjs_update(&mut page, &diff).expect("decodable");
        assert!(!changed);
        assert_eq!(generation, 1);
    }

    // ── append (researcher-plan D4.1) ───────────────────────────────────────

    #[test]
    fn append_separates_blocks_with_a_blank_line() {
        let doc = setup_doc("Existing note.");
        append_block_to_doc(&doc, "> a quote");
        assert_eq!(read_content(&doc), "Existing note.\n\n> a quote");

        // A second append separates again rather than running together.
        append_block_to_doc(&doc, "> another");
        assert_eq!(
            read_content(&doc),
            "Existing note.\n\n> a quote\n\n> another"
        );
    }

    #[test]
    fn append_does_not_add_leading_blank_line_to_empty_doc() {
        let doc = setup_doc("");
        append_block_to_doc(&doc, "> first");
        assert_eq!(read_content(&doc), "> first");
    }

    #[test]
    fn append_respects_existing_trailing_newlines() {
        let one = setup_doc("Note.\n");
        append_block_to_doc(&one, "> q");
        assert_eq!(read_content(&one), "Note.\n\n> q");

        let two = setup_doc("Note.\n\n");
        append_block_to_doc(&two, "> q");
        assert_eq!(read_content(&two), "Note.\n\n> q");
    }

    /// THE property D4.1 exists to guarantee: appending while someone has the
    /// page open in an editor must MERGE, not clobber their work.
    ///
    /// Simulates a connected editor (its own Y.Doc) typing concurrently with a
    /// server-side append, then syncs both ways — both edits must survive.
    #[test]
    fn append_merges_with_a_concurrent_editor_instead_of_clobbering() {
        let server = setup_doc("Existing note.");

        // An editor connects and syncs the current state.
        let client = new_doc();
        {
            let sv = client.transact().state_vector();
            let update = server.transact().encode_state_as_update_v1(&sv);
            let mut txn = client.transact_mut();
            txn.apply_update(Update::decode_v1(&update).unwrap()).unwrap();
        }
        assert_eq!(read_content(&client), "Existing note.");

        // The user types in the open editor...
        {
            let mut txn = client.transact_mut();
            let text = txn.get_or_insert_text("content");
            text.insert(&mut txn, 0, "TYPED ");
        }
        // ...while the server appends a highlight block.
        append_block_to_doc(&server, "> a quote");

        // Exchange updates both directions (what the broadcast + client sync do).
        {
            let sv = client.transact().state_vector();
            let update = server.transact().encode_state_as_update_v1(&sv);
            let mut txn = client.transact_mut();
            txn.apply_update(Update::decode_v1(&update).unwrap()).unwrap();
        }
        {
            let sv = server.transact().state_vector();
            let update = client.transact().encode_state_as_update_v1(&sv);
            let mut txn = server.transact_mut();
            txn.apply_update(Update::decode_v1(&update).unwrap()).unwrap();
        }

        let merged = read_content(&server);
        // The editor's typing survived — this is what a blind content replace
        // would have destroyed.
        assert!(merged.contains("TYPED "), "editor's edit was lost: {merged}");
        assert!(merged.contains("> a quote"), "append was lost: {merged}");
        // Both docs converged.
        assert_eq!(merged, read_content(&client));
    }

    #[test]
    fn test_text_find_replace_simple() {
        let doc = setup_doc("Hello world, this is a test.");
        let mut txn = doc.transact_mut();
        let text = txn.get_or_insert_text("content");
        let content = text.get_string(&txn);

        let byte_offset = content.find("world").unwrap();
        let start = byte_offset as u32;
        let len = "world".len() as u32;

        text.remove_range(&mut txn, start, len);
        text.insert(&mut txn, start, "universe");
        drop(txn);

        assert_eq!(read_content(&doc), "Hello universe, this is a test.");
    }

    #[test]
    fn test_text_find_replace_with_markdown() {
        let doc = setup_doc("This is **bold text** here.");
        let mut txn = doc.transact_mut();
        let text = txn.get_or_insert_text("content");
        let content = text.get_string(&txn);

        // With Y.Text, markdown syntax IS the content — find/replace works directly
        let byte_offset = content.find("**bold text**").unwrap();
        let start = byte_offset as u32;
        let len = "**bold text**".len() as u32;

        text.remove_range(&mut txn, start, len);
        text.insert(&mut txn, start, "**strong words**");
        drop(txn);

        assert_eq!(read_content(&doc), "This is **strong words** here.");
    }

    #[test]
    fn test_text_find_replace_not_found() {
        let doc = setup_doc("Hello world.");
        let txn = doc.transact();
        let text = txn.get_text("content").unwrap();
        let content = text.get_string(&txn);

        assert!(content.find("nonexistent text").is_none());
    }

    #[test]
    fn test_text_full_replacement() {
        let doc = setup_doc("Old content here.");
        let mut txn = doc.transact_mut();
        let text = txn.get_or_insert_text("content");
        let len = text.len(&txn);
        text.remove_range(&mut txn, 0, len);
        text.insert(&mut txn, 0, "# New Content\n\nBrand new page.");
        drop(txn);

        assert_eq!(read_content(&doc), "# New Content\n\nBrand new page.");
    }

    #[test]
    fn test_text_empty_document() {
        let doc = setup_doc("");
        let mut txn = doc.transact_mut();
        let text = txn.get_or_insert_text("content");
        text.insert(&mut txn, 0, "First content");
        drop(txn);

        assert_eq!(read_content(&doc), "First content");
    }

    #[test]
    fn test_text_unicode_handling() {
        let doc = setup_doc("Hello 🌍 world with émojis.");
        let mut txn = doc.transact_mut();
        let text = txn.get_or_insert_text("content");
        let content = text.get_string(&txn);

        let byte_offset = content.find("world").unwrap();
        let start = byte_offset as u32;
        let len = "world".len() as u32;

        text.remove_range(&mut txn, start, len);
        text.insert(&mut txn, start, "planet");
        drop(txn);

        assert_eq!(read_content(&doc), "Hello 🌍 planet with émojis.");
    }

    #[test]
    fn test_text_entity_link_find_replace() {
        // Entity links are just markdown text now — find/replace works naturally
        let doc = setup_doc("Hello [@John](/person/123) how are you?");
        let mut txn = doc.transact_mut();
        let text = txn.get_or_insert_text("content");
        let content = text.get_string(&txn);

        let byte_offset = content.find("how are you?").unwrap();
        let start = byte_offset as u32;
        let len = "how are you?".len() as u32;

        text.remove_range(&mut txn, start, len);
        text.insert(&mut txn, start, "what's up?");
        drop(txn);

        let result = read_content(&doc);
        assert!(result.contains("[@John](/person/123)"), "Entity link should survive. Got: {}", result);
        assert!(result.contains("what's up?"), "Replacement should be present. Got: {}", result);
    }

    // ── text writes: the diff is applied exactly, at either granularity ─────

    /// Apply `text_ops` to a doc holding `old` and read the result back.
    fn written(old: &str, new: &str, granularity: Granularity) -> (String, usize) {
        let doc = setup_doc(old);
        let ops = text_ops(old, new, granularity);
        let n = ops.len();
        {
            let mut txn = doc.transact_mut();
            let text = txn.get_or_insert_text("content");
            apply_text_ops(&mut txn, &text, ops);
        }
        (read_content(&doc), n)
    }

    #[test]
    fn a_text_write_produces_exactly_the_new_text() {
        let cases = [
            ("", "A whole new page.\n"),
            ("Old page.\n", ""),
            ("You met David Okafor.\n", "You met David Okafor at the café.\n"),
            (
                "## Abstract\nA quiet day.\n\n## Morning\nCoffee 🌍 first.\n",
                "## Abstract\nA long day.\n\n## Morning\nCoffee 🌍 first, then the train.\n\n## Evening\nHome.\n",
            ),
            ("one\ntwo\nthree\n", "three\ntwo\none"),
            (MIXED, MIXED_REVISED),
            ("\u{1F44B}\u{1F3FD}", "\u{1F44B}\u{1F3FF}"),
            ("東京の朝", "東京の朝、大阪の夜"),
        ];
        for (old, new) in cases {
            for g in [Granularity::Words, Granularity::Lines] {
                let (got, _) = written(old, new, g);
                assert_eq!(got, new, "{g:?}: {old:?} -> {new:?}");
            }
        }
    }

    /// The reason whole-page replacement diffs by line: two independent drafts
    /// share lines far more often than they share long runs of words, so the
    /// line diff is a small fraction of the operations for the same text.
    #[test]
    fn replacing_a_page_by_line_takes_far_fewer_operations() {
        let old: String = (0..200)
            .map(|i| format!("Sentence {i} of the first draft says something about the day.\n"))
            .collect();
        let new: String = (0..200)
            .map(|i| format!("Line {i} in the second draft puts it another way entirely.\n"))
            .collect();
        let (by_word, word_ops) = written(&old, &new, Granularity::Words);
        let (by_line, line_ops) = written(&old, &new, Granularity::Lines);
        assert_eq!(by_word, new);
        assert_eq!(by_line, new);
        assert!(
            line_ops * 4 < word_ops,
            "lines {line_ops} vs words {word_ops}"
        );
    }

    /// A page with saved CRDT state, the way a page looks once an editor has
    /// had it open.
    async fn saved_page(pool: &PgPool, content: &str) -> String {
        let page = pages::create_page(
            pool,
            pages::CreatePageRequest {
                title: "Notes".into(),
                content: content.into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
            },
        )
        .await
        .unwrap();
        let state = {
            let doc = setup_doc(content);
            let txn = doc.transact();
            txn.encode_state_as_update_v1(&StateVector::default())
        };
        save_and_materialize(pool, &page.id, &state).await.unwrap();
        page.id
    }

    fn held_text(doc: &PageDoc) -> String {
        content_of(&doc.doc.transact())
    }

    /// An open editor's socket takes its doc once and holds it. When moka
    /// drops that doc for being idle, a server-side write must still reach
    /// the held doc (and the editor through its broadcast), not a second doc
    /// loaded from the database that the editor never sees and later saves
    /// over.
    #[sqlx::test]
    async fn a_write_reaches_the_doc_an_open_editor_holds_after_eviction(pool: PgPool) {
        let page_id = saved_page(&pool, "First line.\n").await;
        let yjs = YjsState::new(pool.clone());

        let held = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        let mut editor = held.read().await.broadcast_tx.subscribe();

        yjs.doc_cache.pages.invalidate(&page_id);
        assert!(yjs.doc_cache.pages.get(&page_id).is_none(), "evicted from moka");

        let after = yjs
            .replace_text(&page_id, "First line.\n", "First line.\nSecond line.\n")
            .await
            .unwrap()
            .text;
        assert_eq!(after, "First line.\nSecond line.\n");
        assert_eq!(held_text(&*held.read().await), after, "the held doc has the write");
        assert!(editor.try_recv().is_ok(), "and the open editor was sent it");

        let again = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(Arc::ptr_eq(&held, &again), "one doc per page in this process");

        let stored: String = sqlx::query_scalar("SELECT content FROM app_pages WHERE id = $1")
            .bind(&page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored, after, "a machine write is saved at once");
    }

    /// The external-rewrite rule still holds for a doc found through the held
    /// map: `yjs_state IS NULL` means something wrote the page through the
    /// pool, and the database wins.
    #[sqlx::test]
    async fn a_held_doc_still_yields_to_a_rewrite_outside_the_crdt(pool: PgPool) {
        let page_id = saved_page(&pool, "Old prose.\n").await;
        let yjs = YjsState::new(pool.clone());
        let held = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        yjs.doc_cache.pages.invalidate(&page_id);

        sqlx::query("UPDATE app_pages SET content = 'New prose.\n', yjs_state = NULL WHERE id = $1")
            .bind(&page_id)
            .execute(&pool)
            .await
            .unwrap();

        let fresh = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(!Arc::ptr_eq(&held, &fresh));
        assert_eq!(held_text(&*fresh.read().await), "New prose.\n");
    }

    /// A write against text the page no longer says changes nothing.
    #[sqlx::test]
    async fn a_stale_write_is_refused_and_changes_nothing(pool: PgPool) {
        let page_id = saved_page(&pool, "What the page says.\n").await;
        let yjs = YjsState::new(pool.clone());

        let err = yjs
            .replace_text(&page_id, "What the writer read.\n", "A new draft.\n")
            .await
            .unwrap_err();
        assert_eq!(err, TextWriteError::Stale);
        assert_eq!(yjs.read_text(&page_id).await.unwrap(), "What the page says.\n");
    }

    /// A page nobody has saved through the CRDT, the way a page written
    /// through the pool looks (`yjs_state` NULL).
    async fn unsaved_page(pool: &PgPool, content: &str) -> String {
        pages::create_page(
            pool,
            pages::CreatePageRequest {
                title: "Notes".into(),
                content: content.into(),
                project_id: None,
                icon: None,
                icon_color: None,
                cover_url: None,
                tags: None,
            },
        )
        .await
        .unwrap()
        .id
    }

    async fn stored(pool: &PgPool, page_id: &str) -> (String, bool) {
        sqlx::query_as("SELECT content, yjs_state IS NOT NULL FROM app_pages WHERE id = $1")
            .bind(page_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// What an open editor sends after its owner types `typed` at the end.
    async fn typed_in_an_editor(doc: &Arc<RwLock<PageDoc>>, typed: &str) -> Vec<u8> {
        let client = new_doc();
        {
            let state = doc.read().await.written().state;
            let mut txn = client.transact_mut();
            txn.apply_update(Update::decode_v1(&state).unwrap()).unwrap();
        }
        let sv = client.transact().state_vector();
        {
            let mut txn = client.transact_mut();
            let text = txn.get_or_insert_text("content");
            let len = text.len(&txn);
            text.insert(&mut txn, len, typed);
        }
        let diff = client.transact().encode_state_as_update_v1(&sv);
        diff
    }

    /// Run the save loop as if every debounce and backoff had run out.
    async fn run_the_save_loop(yjs: &YjsState) {
        yjs.save_queue
            .save_due(&yjs.pool, Instant::now() + Duration::from_secs(3600))
            .await;
    }

    /// Every save of a page fails until `allow_page_saves`: the database
    /// refuses the write while it still answers reads.
    async fn refuse_page_saves(pool: &PgPool) {
        for stmt in [
            "CREATE FUNCTION refuse_page_saves() RETURNS trigger LANGUAGE plpgsql \
             AS $$ BEGIN RAISE EXCEPTION 'disk full'; END $$",
            "CREATE TRIGGER refuse_page_saves BEFORE UPDATE ON app_pages \
             FOR EACH ROW WHEN (NEW.yjs_state IS NOT NULL) EXECUTE FUNCTION refuse_page_saves()",
        ] {
            sqlx::query(stmt).execute(pool).await.unwrap();
        }
    }

    async fn allow_page_saves(pool: &PgPool) {
        sqlx::query("DROP TRIGGER refuse_page_saves ON app_pages")
            .execute(pool)
            .await
            .unwrap();
    }

    /// The owner types, and a machine edit lands inside the debounce. The
    /// machine's state carries the typing, so it replaces the queued one:
    /// when the debounce runs out, the older state is not saved over it.
    #[sqlx::test]
    async fn a_machine_edit_is_not_saved_over_by_the_typing_queued_before_it(pool: PgPool) {
        let page_id = saved_page(&pool, "Your line.\n").await;
        let yjs = YjsState::new(pool.clone());
        let editor = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();

        let typed = typed_in_an_editor(&editor, "More.\n").await;
        take_client_update(&yjs, &page_id, &editor, &typed).await.unwrap();

        let edit = yjs
            .apply_text_edit(&page_id, "Your line.", "Your own line.")
            .await
            .unwrap();
        assert_eq!(edit.before.text, "Your line.\nMore.\n", "what the edit found");
        let written = edit.after.unwrap();
        assert_eq!(written.text, "Your own line.\nMore.\n");
        assert_eq!(stored(&pool, &page_id).await.0, written.text, "saved at once");

        run_the_save_loop(&yjs).await;
        assert_eq!(stored(&pool, &page_id).await.0, written.text, "and still after the debounce");
    }

    /// A write whose save fails is on the page all the same: the doc that
    /// holds it stays the page's (even on a page the database still has no
    /// CRDT state for, and even after a write through the pool), the version
    /// the caller cuts holds it, and the save loop lands it once the database
    /// takes writes again.
    #[sqlx::test]
    async fn a_write_that_could_not_be_saved_stays_on_the_page_until_it_is(pool: PgPool) {
        let page_id = unsaved_page(&pool, "The old page.\n").await;
        let yjs = YjsState::new(pool.clone());
        let doc = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();

        refuse_page_saves(&pool).await;
        let err = yjs
            .replace_text(&page_id, "The old page.\n", "The new page.\n")
            .await
            .unwrap_err();
        let TextWriteError::NotSaved { written, .. } = err else {
            panic!("expected NotSaved, got {err:?}");
        };
        assert_eq!(written.text, "The new page.\n");
        assert_eq!(extract_text_content(&written.state), "The new page.\n");
        assert_eq!(stored(&pool, &page_id).await, ("The old page.\n".to_string(), false));

        // A reader, and a write through the pool, while the save is pending.
        assert_eq!(yjs.read_text(&page_id).await.unwrap(), "The new page.\n");
        sqlx::query("UPDATE app_pages SET content = 'Through the pool.\n' WHERE id = $1")
            .bind(&page_id)
            .execute(&pool)
            .await
            .unwrap();
        let again = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(Arc::ptr_eq(&doc, &again), "the doc holding the write is the page's");

        // Still refused: the loop keeps the state queued.
        run_the_save_loop(&yjs).await;
        assert_eq!(yjs.read_text(&page_id).await.unwrap(), "The new page.\n");

        allow_page_saves(&pool).await;
        run_the_save_loop(&yjs).await;
        assert_eq!(stored(&pool, &page_id).await, ("The new page.\n".to_string(), true));
        assert!(!doc.read().await.has_unsaved_changes());
    }

    /// A page nobody has saved through the CRDT reads `yjs_state IS NULL`
    /// while an editor's first save waits out the debounce. Another reader in
    /// that window keeps the editor's doc, whether the editor typed or only
    /// opened the page.
    #[sqlx::test]
    async fn an_editors_doc_is_kept_while_its_first_save_waits(pool: PgPool) {
        let page_id = unsaved_page(&pool, "Old prose.\n").await;
        let yjs = YjsState::new(pool.clone());
        let editor = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();

        // Opening the page: the client answers with an update that changes
        // nothing, and the socket queues the state anyway.
        let nothing = editor.read().await.written().state;
        take_client_update(&yjs, &page_id, &editor, &nothing).await.unwrap();
        let other = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(Arc::ptr_eq(&editor, &other), "an open page is not a rewrite");

        let typed = typed_in_an_editor(&editor, "Your line.\n").await;
        take_client_update(&yjs, &page_id, &editor, &typed).await.unwrap();
        assert_eq!(yjs.read_text(&page_id).await.unwrap(), "Old prose.\nYour line.\n");
        let other = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(Arc::ptr_eq(&editor, &other), "nor is typing that has not saved yet");

        run_the_save_loop(&yjs).await;
        assert_eq!(stored(&pool, &page_id).await, ("Old prose.\nYour line.\n".to_string(), true));
        let other = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(Arc::ptr_eq(&editor, &other));
    }

    /// A doc built from `content`, holding nothing unsaved, still yields to a
    /// later write through the pool: that is a rewrite outside the CRDT.
    #[sqlx::test]
    async fn a_page_rewritten_through_the_pool_is_read_again(pool: PgPool) {
        let page_id = unsaved_page(&pool, "Old prose.\n").await;
        let yjs = YjsState::new(pool.clone());
        let before = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();

        sqlx::query("UPDATE app_pages SET content = 'New prose.\n' WHERE id = $1 AND yjs_state IS NULL")
            .bind(&page_id)
            .execute(&pool)
            .await
            .unwrap();

        let after = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(!Arc::ptr_eq(&before, &after));
        assert_eq!(held_text(&*after.read().await), "New prose.\n");
    }

    /// A caller that finds its doc already replaced takes the replacement
    /// rather than retiring it, so two readers of a rewritten page end on one
    /// doc.
    #[sqlx::test]
    async fn a_doc_already_replaced_is_not_retired_again(pool: PgPool) {
        let page_id = unsaved_page(&pool, "Prose.\n").await;
        let yjs = YjsState::new(pool.clone());
        let cache = &yjs.doc_cache;
        let first = cache.get_or_create(&page_id, &pool).await.unwrap();

        assert!(cache.retire(&page_id, &first));
        let second = cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(!Arc::ptr_eq(&first, &second));

        assert!(!cache.retire(&page_id, &first), "the first doc is no longer the page's");
        let now = cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(Arc::ptr_eq(&second, &now), "the replacement is still registered");
    }

    /// A caller that found the page's doc and then waited, while another
    /// caller retired that doc (a rewrite outside the CRDT) and loaded the
    /// page again, takes the replacement rather than the retired doc that no
    /// other writer reaches.
    #[sqlx::test]
    async fn a_caller_whose_doc_was_replaced_meanwhile_takes_the_replacement(pool: PgPool) {
        let page_id = saved_page(&pool, "Prose.\n").await;
        let yjs = YjsState::new(pool.clone());
        let cache = yjs.doc_cache.clone();
        let first = cache.get_or_create(&page_id, &pool).await.unwrap();

        // The waiting caller finds `first` and waits on its lock.
        let busy = first.write().await;
        let waiting = tokio::spawn({
            let (cache, pool, page_id) = (cache.clone(), pool.clone(), page_id.clone());
            async move { cache.get_or_create(&page_id, &pool).await.unwrap() }
        });
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }

        assert!(cache.retire(&page_id, &first));
        let second = cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        drop(busy);

        let got = waiting.await.unwrap();
        assert!(Arc::ptr_eq(&got, &second), "the replacement, not the retired doc");
        assert!(!cache.is_registered(&page_id, &first));
        assert!(cache.is_registered(&page_id, &second));
    }

    /// An editor's socket applies a keystroke and records the human edit, a
    /// database round trip, before it queues the state. A machine edit that
    /// lands and is saved in between is not saved over by that older state,
    /// whether the machine edit's state already left the queue...
    #[sqlx::test]
    async fn a_keystroke_queued_late_is_not_saved_over_a_machine_edit(pool: PgPool) {
        let page_id = saved_page(&pool, "Your line.\n").await;
        let yjs = YjsState::new(pool.clone());
        let editor = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();

        let typed = typed_in_an_editor(&editor, "More.\n").await;
        let (late, changed, generation) =
            apply_yjs_update(&mut *editor.write().await, &typed).expect("decodable");
        assert!(changed);

        let written = yjs
            .apply_text_edit(&page_id, "Your line.", "Your own line.")
            .await
            .unwrap()
            .after
            .unwrap();
        assert_eq!(written.text, "Your own line.\nMore.\n");

        yjs.save_queue.queue_save(page_id.clone(), &editor, late, generation).await;
        run_the_save_loop(&yjs).await;
        assert_eq!(stored(&pool, &page_id).await.0, written.text);
        assert!(!editor.read().await.has_unsaved_changes());
    }

    /// ...or is still waiting there because the database refused it.
    #[sqlx::test]
    async fn a_keystroke_queued_late_does_not_replace_a_machine_edit_waiting_to_save(
        pool: PgPool,
    ) {
        let page_id = saved_page(&pool, "Your line.\n").await;
        let yjs = YjsState::new(pool.clone());
        let editor = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();

        let typed = typed_in_an_editor(&editor, "More.\n").await;
        let (late, _, generation) =
            apply_yjs_update(&mut *editor.write().await, &typed).expect("decodable");

        refuse_page_saves(&pool).await;
        let edit = yjs
            .apply_text_edit(&page_id, "Your line.", "Your own line.")
            .await
            .unwrap();
        let Err(TextWriteError::NotSaved { written, .. }) = edit.after else {
            panic!("expected NotSaved, got {:?}", edit.after);
        };

        yjs.save_queue.queue_save(page_id.clone(), &editor, late, generation).await;
        allow_page_saves(&pool).await;
        run_the_save_loop(&yjs).await;
        assert_eq!(stored(&pool, &page_id).await.0, written.text);
        assert!(!editor.read().await.has_unsaved_changes());
    }

    /// A doc whose state waits in the save queue stays the page's with
    /// nothing else holding it (moka dropped it, no editor is open): a
    /// reader finds that doc and its text, not one rebuilt from what the
    /// database still says.
    #[sqlx::test]
    async fn a_doc_whose_save_is_pending_outlives_eviction(pool: PgPool) {
        let page_id = saved_page(&pool, "The old page.\n").await;
        let yjs = YjsState::new(pool.clone());
        let doc = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();

        refuse_page_saves(&pool).await;
        let err = yjs
            .replace_text(&page_id, "The old page.\n", "The draft.\n")
            .await
            .unwrap_err();
        assert!(matches!(err, TextWriteError::NotSaved { .. }), "{err:?}");

        let original = Arc::downgrade(&doc);
        drop(doc);
        yjs.doc_cache.pages.invalidate(&page_id);
        yjs.doc_cache.pages.run_pending_tasks();
        assert_eq!(original.strong_count(), 1, "nothing holds it but the save queue");

        let again = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(
            original.upgrade().is_some_and(|o| Arc::ptr_eq(&o, &again)),
            "the doc holding the draft"
        );
        assert_eq!(held_text(&*again.read().await), "The draft.\n");

        allow_page_saves(&pool).await;
        run_the_save_loop(&yjs).await;
        assert_eq!(stored(&pool, &page_id).await.0, "The draft.\n");
    }

    // ── offsets: server writes count bytes, through non-ASCII text ──────────

    /// Characters of two, three and four bytes, an emoji with a skin tone
    /// (two chars, one glyph) and ZWJ sequences (several chars joined by
    /// U+200D): the family, the rainbow flag.
    const MIXED: &str = "Café crème at São Paulo.\n東京の朝は静かだ。\n\
        Wave \u{1F44B}\u{1F3FD} then \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466} \
        and \u{1F3F3}\u{FE0F}\u{200D}\u{1F308} end.\n";

    /// `MIXED` revised word by word on every line, around and inside the
    /// multi-byte runs.
    const MIXED_REVISED: &str = "Café crème brûlée at São Paulo, with Zoë.\n東京の朝は静かだ。 大阪も。\n\
        Wave \u{1F44B}\u{1F3FF} then \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F466} \
        and \u{1F3F3}\u{FE0F}\u{200D}\u{1F308} end, \u{1F9D1}\u{1F3FD}\u{200D}\u{1F4BB}.\n";

    /// Every doc the server writes into writes under a 32-bit client id, so
    /// a box rolled back to yrs 0.18 reads its edits as the right writer's.
    #[test]
    fn every_server_doc_writes_under_a_32_bit_client_id() {
        let docs = [
            new_doc(),
            doc_from_text(MIXED),
            doc_from_state("p", &state_from_text(MIXED)),
        ];
        for doc in docs {
            assert!(doc.client_id().get() <= u64::from(u32::MAX));
        }
        for _ in 0..100 {
            assert!(new_doc().client_id().get() <= u64::from(u32::MAX));
        }
    }

    #[test]
    fn every_server_doc_counts_text_in_bytes() {
        assert_eq!(new_doc().offset_kind(), OffsetKind::Bytes);
        for doc in [doc_from_text(MIXED), doc_from_state("p", &state_from_text(MIXED))] {
            assert_eq!(doc.offset_kind(), OffsetKind::Bytes);
            let txn = doc.transact();
            assert_eq!(txn.get_text("content").unwrap().len(&txn), MIXED.len() as u32);
        }
    }

    /// `apply_text_edit` finds by `str::find` and edits at that byte offset,
    /// so a match after, or inside, multi-byte text lands exactly.
    #[sqlx::test]
    async fn a_find_replace_lands_exactly_in_non_ascii_text(pool: PgPool) {
        let page_id = saved_page(&pool, MIXED).await;
        let yjs = YjsState::new(pool.clone());
        let cases = [
            ("crème", "brûlée"),
            ("静か", "賑やか"),
            ("\u{1F44B}\u{1F3FD}", "\u{1F44B}\u{1F3FF}"),
            // Inside the family: the last two people and their joiner.
            ("\u{1F467}\u{200D}\u{1F466}", "\u{1F466}"),
            ("end", "fin"),
        ];
        let mut expected = MIXED.to_string();
        for (find, replace) in cases {
            expected = expected.replacen(find, replace, 1);
            let edit = yjs.apply_text_edit(&page_id, find, replace).await.unwrap();
            assert_eq!(edit.after.unwrap().text, expected, "{find:?} -> {replace:?}");
        }
        assert_eq!(
            expected,
            "Café brûlée at São Paulo.\n東京の朝は賑やかだ。\n\
             Wave \u{1F44B}\u{1F3FF} then \u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F466} \
             and \u{1F3F3}\u{FE0F}\u{200D}\u{1F308} fin.\n"
        );
        assert_eq!(yjs.read_text(&page_id).await.unwrap(), expected);
        assert_eq!(stored(&pool, &page_id).await.0, expected);

        // An empty find replaces the whole page, by the doc's byte length.
        let whole = "全部 新しい \u{1F469}\u{1F3FE}\u{200D}\u{1F680}\n";
        let edit = yjs.apply_text_edit(&page_id, "", whole).await.unwrap();
        assert_eq!(edit.before.text, expected);
        assert_eq!(edit.after.unwrap().text, whole);
        assert_eq!(stored(&pool, &page_id).await.0, whole);
    }

    #[sqlx::test]
    async fn a_word_diff_lands_exactly_in_non_ascii_text(pool: PgPool) {
        let page_id = saved_page(&pool, MIXED).await;
        let yjs = YjsState::new(pool.clone());
        let written = yjs.apply_text_diff(&page_id, MIXED, MIXED_REVISED).await.unwrap();
        assert_eq!(written.text, MIXED_REVISED);
        assert_eq!(extract_text_content(&written.state), MIXED_REVISED);
        assert_eq!(stored(&pool, &page_id).await.0, MIXED_REVISED);
    }

    #[sqlx::test]
    async fn a_line_replace_lands_exactly_in_non_ascii_text(pool: PgPool) {
        let page_id = saved_page(&pool, MIXED_REVISED).await;
        let yjs = YjsState::new(pool.clone());
        let written = yjs.replace_text(&page_id, MIXED_REVISED, MIXED).await.unwrap();
        assert_eq!(written.text, MIXED);
        assert_eq!(extract_text_content(&written.state), MIXED);
        assert_eq!(stored(&pool, &page_id).await.0, MIXED);
    }

    // ── saved states the server did not write on 0.28 ───────────────────────

    /// A saved page state, and the text it holds, from a fixture directory:
    /// `<name>.bin` and `<name>.txt`.
    macro_rules! fixture {
        ($dir:literal, $name:literal) => {
            (
                $name,
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/",
                    $dir,
                    "/",
                    $name,
                    ".bin"
                )) as &[u8],
                include_str!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/",
                    $dir,
                    "/",
                    $name,
                    ".txt"
                )),
            )
        };
    }

    /// A state the server saved on yrs 0.18. Written once by a throwaway
    /// program against yrs =0.18.8 making the server's own calls from two
    /// yrs replicas (client ids 1 and 2): a Y.Text named "content", edited by
    /// byte offset, saved as `encode_state_as_update_v1` of the whole doc.
    /// That is every `app_pages.yjs_state` a box saved on 0.18, and every
    /// `app_page_versions.yjs_snapshot` the server cut there.
    macro_rules! legacy {
        ($name:literal) => {
            fixture!("yjs-0-18", $name)
        };
    }

    /// A state the browser wrote, with apps/web's Yjs 13.6
    /// (`tests/fixtures/yjs-13/generate.mjs`): typed key by key at UTF-16
    /// positions, which is where Yjs splits its items. A version the browser
    /// saves (`lib/yjs/versions.ts`) is stored in
    /// `app_page_versions.yjs_snapshot` exactly so, and the items an editor
    /// writes reach `app_pages.yjs_state` through the server's doc.
    macro_rules! browser {
        ($name:literal) => {
            fixture!("yjs-13", $name)
        };
    }

    /// From yrs 0.18 (both replicas yrs): `ascii`, a page as first saved;
    /// `non-ascii`, `MIXED`'s kinds of text plus Korean, edited by both
    /// replicas around and inside them; `history`, 300 edits from the two
    /// synced every few steps, with deletions, then an insert in the middle
    /// from one and a deletion at the start from the other; `empty`, a page
    /// saved with nothing on it; `emptied`, one whose text was all deleted.
    ///
    /// From the browser: `typed`, a version snapshot of a page typed key by
    /// key around emoji, ZWJ sequences and CJK, then a skin tone deleted and
    /// a word inserted inside a CJK run; `split-surrogate`, half an emoji's
    /// surrogate pair deleted, which Yjs writes as U+FFFD; `concurrent`, two
    /// editors' 400 edits around the same text, synced every few steps.
    const SAVED_STATES: [(&str, &[u8], &str); 8] = [
        legacy!("ascii"),
        legacy!("non-ascii"),
        legacy!("history"),
        legacy!("empty"),
        legacy!("emptied"),
        browser!("typed"),
        browser!("split-surrogate"),
        browser!("concurrent"),
    ];

    #[test]
    fn saved_states_read_the_same_text() {
        for (name, state, text) in SAVED_STATES {
            assert_eq!(text_of_state(state).unwrap(), text, "{name}: read");
            assert_eq!(extract_text_content(state), text, "{name}: materialized");
            assert_eq!(pages::yjs_state_to_markdown(state), text, "{name}: shared");
            let doc = doc_from_state(name, state);
            assert_eq!(read_content(&doc), text, "{name}: loaded");
            let saved_again = doc.transact().encode_state_as_update_v1(&StateVector::default());
            assert_eq!(extract_text_content(&saved_again), text, "{name}: saved again");
        }
    }

    /// The server's byte-offset writes land exactly on text another writer
    /// put there: yrs 0.18 by bytes, the browser by UTF-16 units.
    #[test]
    fn saved_states_take_byte_offset_writes() {
        let revisions = [
            (
                legacy!("non-ascii"),
                vec![("賑やか", "静か"), ("\u{1F389} ", ""), ("世の界", "世界")],
            ),
            (
                browser!("typed"),
                vec![
                    ("東京駅", "東京"),
                    ("\u{1F44B} then", "\u{1F44B}\u{1F3FF} then"),
                    ("\u{200D}\u{1F467}\u{200D}\u{1F466}", "\u{200D}\u{1F466}"),
                ],
            ),
            (browser!("split-surrogate"), vec![("\u{FFFD}", "\u{1F44B}")]),
        ];
        for ((name, state, text), edits) in revisions {
            let mut new = text.to_string();
            for (find, replace) in edits {
                assert!(new.contains(find), "{name}: {find:?}");
                new = new.replacen(find, replace, 1);
            }
            new.push_str("Fin \u{1F44B}\u{1F3FD}.\n");
            for granularity in [Granularity::Words, Granularity::Lines] {
                let doc = doc_from_state(name, state);
                {
                    let mut txn = doc.transact_mut();
                    let content = txn.get_or_insert_text("content");
                    apply_text_ops(&mut txn, &content, text_ops(text, &new, granularity));
                }
                assert_eq!(read_content(&doc), new, "{name}: {granularity:?}");
            }
        }
    }

    /// An update another writer encoded applies to a doc loaded here, as an
    /// update over the socket does: one yrs 0.18 encoded (an insert in the
    /// middle, a deletion, a non-ASCII append), and one an editor in the
    /// browser sends after typing on a page (the same three).
    #[test]
    fn updates_other_writers_encoded_apply_here() {
        for ((_, base, _), (name, update, after)) in [
            (legacy!("ascii"), legacy!("incremental")),
            (browser!("typed"), browser!("update")),
        ] {
            let mut page = page_doc(doc_from_state(name, base));

            let (state, changed, generation) =
                apply_yjs_update(&mut page, update).expect("applies");
            assert!(changed, "{name}");
            assert_eq!(generation, 1, "{name}");
            assert_eq!(read_content(&page.doc), after, "{name}");
            assert_eq!(extract_text_content(&state), after, "{name}");

            let (_, changed, _) = apply_yjs_update(&mut page, update).expect("applies");
            assert!(!changed, "{name}: the same update again changes nothing");
        }
    }

    /// A page whose saved state yrs 0.18 wrote loads through the cache, takes
    /// a server edit, and saves a state and text that agree.
    #[sqlx::test]
    async fn a_page_saved_by_yrs_0_18_loads_edits_and_saves(pool: PgPool) {
        let (_, state, text) = legacy!("non-ascii");
        let page_id = unsaved_page(&pool, "").await;
        sqlx::query("UPDATE app_pages SET yjs_state = $1, content = $2 WHERE id = $3")
            .bind(state)
            .bind(text)
            .bind(&page_id)
            .execute(&pool)
            .await
            .unwrap();
        let yjs = YjsState::new(pool.clone());
        assert_eq!(yjs.read_text(&page_id).await.unwrap(), text);

        let edit = yjs.apply_text_edit(&page_id, "\u{1F389} ", "").await.unwrap();
        let expected = text.replacen("\u{1F389} ", "", 1);
        assert_eq!(edit.before.text, text);
        assert_eq!(edit.after.unwrap().text, expected);
        assert_eq!(stored(&pool, &page_id).await, (expected.clone(), true));
        let saved: Vec<u8> = sqlx::query_scalar("SELECT yjs_state FROM app_pages WHERE id = $1")
            .bind(&page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(extract_text_content(&saved), expected);
    }

    // ── the contract guard on the page socket ──────────────────────────────

    #[test]
    fn a_client_binds_at_or_above_the_documents_contract() {
        let server = virtues_document::contract().version;
        assert_eq!(bind(0, 0), Ok(Binding::ReadWrite));
        assert_eq!(bind(server, 0), Ok(Binding::ReadWrite));
        assert_eq!(bind(server, server), Ok(Binding::ReadWrite));
        assert!(bind(0, server).is_err(), "every shipped client, on a tree page");
        assert_eq!(bind(server + 1, server), Ok(Binding::ReadOnly));

        let params = |q: &[(&str, &str)]| -> HashMap<String, String> {
            q.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
        };
        assert_eq!(client_contract(&params(&[])), 0);
        assert_eq!(client_contract(&params(&[("contract", "1")])), 1);
        assert_eq!(client_contract(&params(&[("contract", "one")])), 0);
        assert_eq!(client_contract(&params(&[("contract", "-1")])), 0);
    }

    /// A page stored as a Yjs tree under the current contract, as the
    /// converter will write one.
    async fn tree_page(pool: &PgPool, html: &str) -> String {
        let page_id = unsaved_page(pool, "").await;
        let o = virtues_document::parse_html(html, "doc");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let doc = virtues_document::doc_from_nodes(o.nodes);
        let state = doc.transact().encode_state_as_update_v1(&StateVector::default());
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(&state)
            .bind(&page_id)
            .execute(pool)
            .await
            .unwrap();
        page_id
    }

    /// The page socket alone, served on a free port.
    async fn serve(yjs: YjsState) -> std::net::SocketAddr {
        let app = axum::Router::new()
            .route("/ws/yjs/:page_id", axum::routing::get(yjs_websocket_handler))
            .with_state(yjs);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        addr
    }

    type Socket = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    /// A client, its own doc, and the socket it syncs that doc over, as
    /// y-websocket does.
    struct Client {
        socket: Socket,
        doc: Doc,
    }

    /// What a socket said next: the server's state for the client, or the
    /// code it closed with.
    #[derive(Debug, PartialEq)]
    enum Heard {
        Synced,
        Update,
        Closed(u16),
    }

    impl Client {
        async fn connect(addr: std::net::SocketAddr, page_id: &str, contract: Option<u32>) -> Self {
            let query = contract.map(|c| format!("?contract={c}")).unwrap_or_default();
            let (socket, _) =
                tokio_tungstenite::connect_async(format!("ws://{addr}/ws/yjs/{page_id}{query}"))
                    .await
                    .unwrap();
            let mut client = Client {
                socket,
                doc: new_doc(),
            };
            let sv = client.doc.transact().state_vector().encode_v1();
            client.send(encode_sync_step1(&sv)).await;
            client
        }

        async fn send(&mut self, message: Vec<u8>) {
            use futures::SinkExt;
            // A socket the server already closed refuses the send; what it
            // said is read next.
            let _ = self
                .socket
                .send(tokio_tungstenite::tungstenite::Message::Binary(message))
                .await;
        }

        /// The next thing the server says, applying any state it sends.
        async fn hear(&mut self) -> Heard {
            use futures::StreamExt;
            use tokio_tungstenite::tungstenite::Message as Ws;
            loop {
                let next = tokio::time::timeout(Duration::from_secs(5), self.socket.next())
                    .await
                    .expect("the server answers");
                match next {
                    Some(Ok(Ws::Binary(data))) => {
                        let Some((MSG_SYNC, payload)) = parse_message(&data) else {
                            continue;
                        };
                        let Some((kind, rest)) = parse_sync_message(payload) else {
                            continue;
                        };
                        if kind == MSG_SYNC_STEP1 {
                            continue;
                        }
                        let update = extract_sync_payload(rest).unwrap();
                        apply_v1(&self.doc, update).unwrap();
                        return if kind == MSG_SYNC_STEP2 {
                            Heard::Synced
                        } else {
                            Heard::Update
                        };
                    }
                    Some(Ok(Ws::Close(frame))) => {
                        return Heard::Closed(frame.map(|f| u16::from(f.code)).unwrap_or(0));
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(_)) | None => return Heard::Closed(0),
                }
            }
        }

        /// How the socket ends, past the updates still on their way to it:
        /// the server relays every edit to every socket on the page, its
        /// sender's included.
        async fn close(&mut self) -> Heard {
            loop {
                match self.hear().await {
                    Heard::Update => continue,
                    heard => return heard,
                }
            }
        }

        /// Make an edit in this client's doc and send it, as an editor does.
        async fn edit(&mut self, edit: impl FnOnce(&mut yrs::TransactionMut)) {
            let before = self.doc.transact().state_vector();
            edit(&mut self.doc.transact_mut());
            let update = self.doc.transact().encode_diff_v1(&before);
            self.send(encode_sync_update(&update)).await;
        }
    }

    /// The text of the first paragraph of a tree doc.
    fn first_paragraph(doc: &Doc) -> String {
        virtues_document::read_doc(&doc.transact())
            .first()
            .map(|n| n.text_content())
            .unwrap_or_default()
    }

    /// Type at the start of a tree doc's first paragraph.
    fn type_into_first_paragraph(txn: &mut yrs::TransactionMut, typed: &str) {
        use yrs::{XmlFragment, XmlOut};
        let frag = txn.get_or_insert_xml_fragment("doc");
        let Some(XmlOut::Element(p)) = frag.get(txn, 0) else {
            panic!("a paragraph")
        };
        let Some(XmlOut::Text(t)) = p.get(txn, 0) else {
            panic!("its text")
        };
        t.insert(txn, 0, typed);
    }

    #[sqlx::test]
    async fn a_client_below_a_tree_pages_contract_is_refused(pool: PgPool) {
        let page_id = tree_page(&pool, "<p>Hello</p>").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;

        // Every app that binds the Y.Text, which sends no version.
        let mut old = Client::connect(addr, &page_id, None).await;
        assert_eq!(old.hear().await, Heard::Closed(CLOSE_CONTRACT));
        let mut zero = Client::connect(addr, &page_id, Some(0)).await;
        assert_eq!(zero.hear().await, Heard::Closed(CLOSE_CONTRACT));

        let version = virtues_document::contract().version;
        let mut current = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(current.hear().await, Heard::Synced);
        assert_eq!(first_paragraph(&current.doc), "Hello");
    }

    #[sqlx::test]
    async fn an_update_outside_the_contract_is_refused_before_anyone_sees_it(pool: PgPool) {
        let page_id = tree_page(&pool, "<p>Hello</p>").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;

        let mut writer = Client::connect(addr, &page_id, Some(version)).await;
        let mut watcher = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(writer.hear().await, Heard::Synced);
        assert_eq!(watcher.hear().await, Heard::Synced);

        // A keystroke is taken and relayed.
        writer.edit(|txn| type_into_first_paragraph(txn, "Oh, ")).await;
        assert_eq!(watcher.hear().await, Heard::Update);
        assert_eq!(first_paragraph(&watcher.doc), "Oh, Hello");

        // An old copy's Y.Text pushed into the tree page is not.
        writer
            .edit(|txn| {
                let text = txn.get_or_insert_text("content");
                text.insert(txn, 0, "# typed into the void");
            })
            .await;
        assert_eq!(writer.close().await, Heard::Closed(CLOSE_REFUSED));
        // Nor is the stamp lowered, which would let old clients back in.
        let mut lowerer = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(lowerer.hear().await, Heard::Synced);
        lowerer
            .edit(|txn| {
                let meta = txn.get_or_insert_map("meta");
                yrs::Map::insert(&meta, txn, "contract", yrs::Any::from(0i64));
            })
            .await;
        assert_eq!(lowerer.close().await, Heard::Closed(CLOSE_REFUSED));

        let held = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        let held = held.read().await;
        assert_eq!(first_paragraph(&held.doc), "Oh, Hello");
        assert_eq!(content_of(&held.doc.transact()), "", "no Y.Text reached the doc");
        assert_eq!(held.contract(), version);
        assert_eq!(virtues_document::validate_doc(&held.doc.transact()), []);
        drop(held);
        // The other client heard neither.
        let quiet = tokio::time::timeout(Duration::from_millis(200), watcher.hear()).await;
        assert!(quiet.is_err(), "nothing relayed: {quiet:?}");
    }

    #[sqlx::test]
    async fn a_client_newer_than_the_server_reads_but_does_not_write(pool: PgPool) {
        let page_id = tree_page(&pool, "<p>Hello</p>").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let newer = virtues_document::contract().version + 1;

        let mut client = Client::connect(addr, &page_id, Some(newer)).await;
        assert_eq!(client.hear().await, Heard::Synced);
        assert_eq!(first_paragraph(&client.doc), "Hello");
        client.edit(|txn| type_into_first_paragraph(txn, "Oh, ")).await;
        // Give the server the time to have applied it, had it.
        let _ = tokio::time::timeout(Duration::from_millis(200), client.hear()).await;

        let held = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert_eq!(first_paragraph(&held.read().await.doc), "Hello");
    }

    #[sqlx::test]
    async fn raising_a_pages_contract_closes_the_clients_below_it(pool: PgPool) {
        let page_id = saved_page(&pool, "Some notes.\n").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;

        // A markdown page binds the Y.Text editors, and takes their edits.
        let mut old = Client::connect(addr, &page_id, None).await;
        let mut current = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(old.hear().await, Heard::Synced);
        assert_eq!(current.hear().await, Heard::Synced);
        old.edit(|txn| {
            let text = txn.get_or_insert_text("content");
            text.insert(txn, 0, "More. ");
        })
        .await;
        assert_eq!(current.hear().await, Heard::Update);
        assert_eq!(content_of(&current.doc.transact()), "More. Some notes.\n");

        yjs.raise_contract(&page_id).await.unwrap();
        // The old editor is closed...
        assert_eq!(old.close().await, Heard::Closed(CLOSE_CONTRACT));
        // ...the current one stays, and is sent the stamp.
        assert_eq!(current.hear().await, Heard::Update);
        assert_eq!(
            virtues_document::stamped_version(&current.doc.transact()),
            Some(version)
        );
        // The stamp is saved with the page, and binds the next load.
        let saved: Vec<u8> = sqlx::query_scalar("SELECT yjs_state FROM app_pages WHERE id = $1")
            .bind(&page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            virtues_document::stamped_version(&doc_from_state(&page_id, &saved).transact()),
            Some(version)
        );
        let mut late = Client::connect(addr, &page_id, Some(0)).await;
        assert_eq!(late.hear().await, Heard::Closed(CLOSE_CONTRACT));
    }
    /// Hear what the server sends until it goes quiet. Returns how the socket
    /// closed, if it did.
    async fn settle(client: &mut Client) -> Option<Heard> {
        loop {
            match tokio::time::timeout(Duration::from_millis(300), client.hear()).await {
                Err(_) => return None,
                Ok(Heard::Closed(code)) => return Some(Heard::Closed(code)),
                Ok(_) => continue,
            }
        }
    }

    /// Delete the `item`th child of the page's `block`th block.
    fn delete_child(txn: &mut yrs::TransactionMut, block: u32, item: u32) {
        use yrs::{XmlFragment, XmlOut};
        let frag = txn.get_or_insert_xml_fragment("doc");
        let Some(XmlOut::Element(el)) = frag.get(txn, block) else {
            panic!("a block")
        };
        el.remove_range(txn, item, 1);
    }

    fn page_html(doc: &Doc) -> String {
        virtues_document::to_html(&virtues_document::read_doc(&doc.transact()), false)
    }

    /// A laptop and a phone each delete one of a list's two items, each a
    /// valid edit on its own device. Their merge leaves the list empty; the
    /// device whose edit arrives second can never send another, so it is
    /// not refused. The server repairs the list away and every device
    /// follows.
    #[sqlx::test]
    async fn edits_that_merge_into_an_empty_list_are_taken_and_repaired(pool: PgPool) {
        let page_id = tree_page(
            &pool,
            r#"<p>Groceries</p><ul data-type="taskList"><li data-type="taskItem"><p>milk</p></li><li data-type="taskItem"><p>eggs</p></li></ul>"#,
        )
        .await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;

        let mut laptop = Client::connect(addr, &page_id, Some(version)).await;
        let mut phone = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(laptop.hear().await, Heard::Synced);
        assert_eq!(phone.hear().await, Heard::Synced);

        // Both edit before hearing the other.
        laptop.edit(|txn| delete_child(txn, 1, 0)).await;
        phone.edit(|txn| delete_child(txn, 1, 1)).await;
        assert_eq!(settle(&mut laptop).await, None, "the laptop stays connected");
        assert_eq!(settle(&mut phone).await, None, "the phone stays connected");

        let held = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        let held = held.read().await;
        assert_eq!(virtues_document::validate_doc(&held.doc.transact()), []);
        assert_eq!(page_html(&held.doc), "<p>Groceries</p>");
        drop(held);
        assert_eq!(page_html(&laptop.doc), "<p>Groceries</p>");
        assert_eq!(page_html(&phone.doc), "<p>Groceries</p>");
    }

    /// A markdown page has no `meta`, and its clients may not write one: the
    /// stamp would decide, when the page is next loaded, that every shipped
    /// client is too old for it.
    #[sqlx::test]
    async fn a_markdown_page_refuses_a_stamp_a_client_writes(pool: PgPool) {
        let page_id = saved_page(&pool, "Some notes.\n").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;

        let mut client = Client::connect(addr, &page_id, None).await;
        assert_eq!(client.hear().await, Heard::Synced);
        client
            .edit(|txn| {
                let text = txn.get_or_insert_text("content");
                text.insert(txn, 0, "More. ");
            })
            .await;
        assert_eq!(settle(&mut client).await, None, "a text edit is taken");
        client
            .edit(|txn| {
                let meta = txn.get_or_insert_map("meta");
                yrs::Map::insert(&meta, txn, "contract", yrs::Any::from(1i64));
            })
            .await;
        assert_eq!(client.close().await, Heard::Closed(CLOSE_REFUSED));

        // Typing and the stamp in one transaction, as one update: refused
        // all the same, though yrs could not place the stamp in a document
        // without the text the typing went into.
        let mut client = Client::connect(addr, &page_id, None).await;
        assert_eq!(client.hear().await, Heard::Synced);
        client
            .edit(|txn| {
                let text = txn.get_or_insert_text("content");
                text.insert(txn, 4, "x");
                let meta = txn.get_or_insert_map("meta");
                yrs::Map::insert(&meta, txn, "contract", yrs::Any::from(2i64));
            })
            .await;
        assert_eq!(client.close().await, Heard::Closed(CLOSE_REFUSED));

        let held = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        let held = held.read().await;
        assert_eq!(content_of(&held.doc.transact()), "More. Some notes.\n");
        assert_eq!(virtues_document::stamped_version(&held.doc.transact()), None);
        // What a load of this doc's state would bind the page as.
        let state = held.doc.transact().encode_state_as_update_v1(&StateVector::default());
        assert_eq!(page_doc(doc_from_state(&page_id, &state)).contract(), 0);
        assert_eq!(yjs.page_contract(&page_id).await.unwrap(), 0);
    }

    /// An item of JSON content, which Yjs does not write: `pending` puts it
    /// after an item no page has (client 9, clock 5), so yrs would hold it
    /// back; otherwise it goes straight under the root `content`.
    fn json_item_update(pending: bool) -> Vec<u8> {
        let mut u = vec![1, 1, 7, 0];
        if pending {
            u.extend([0x80 | 2, 9, 5]);
        } else {
            u.push(2);
            u.push(1);
            u.push(7);
            u.extend_from_slice(b"content");
        }
        u.extend([0, 1, b'1']); // a count of 0, then one string, as yrs reads it
        u.push(0); // no deletions
        u
    }

    /// yrs reads JSON content and cannot encode it again: held back, it
    /// makes every later encode of the doc panic, every save and every sync;
    /// in place, the saved state does not load. Such an update is dropped,
    /// or refused on a tree page, and the page keeps taking edits and saving.
    #[sqlx::test]
    async fn an_update_holding_what_yrs_cannot_encode_again_is_turned_away(pool: PgPool) {
        let page_id = saved_page(&pool, "Some notes.\n").await;
        let yjs = YjsState::new(pool.clone());
        let doc = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        for pending in [true, false] {
            let update = json_item_update(pending);
            assert!(Update::decode_v1(&update).is_ok(), "yrs reads it");
            assert_eq!(take_client_update(&yjs, &page_id, &doc, &update).await, Ok(()));
            assert_eq!(doc.read().await.written().text, "Some notes.\n");
        }
        let typed = typed_in_an_editor(&doc, " More.").await;
        take_client_update(&yjs, &page_id, &doc, &typed).await.unwrap();
        let written = doc.read().await.written();
        assert_eq!(written.text, "Some notes.\n More.");
        assert_eq!(read_content(&doc_from_state(&page_id, &written.state)), written.text);

        let tree_id = tree_page(&pool, "<p>Hello</p>").await;
        let tree = yjs.doc_cache.get_or_create(&tree_id, &pool).await.unwrap();
        for pending in [true, false] {
            let refused = take_client_update(&yjs, &tree_id, &tree, &json_item_update(pending))
                .await
                .unwrap_err();
            assert!(refused.contains("JSON content"), "{refused}");
        }
        let state = tree.read().await.written().state;
        let back = doc_from_state(&tree_id, &state);
        assert_eq!(first_paragraph(&back), "Hello");
    }

    /// An update setting `key` in the root map `root` to a number inside
    /// `depth` arrays, built by hand: yrs's own encoder recurses once per
    /// level too.
    fn nested_value_update(root: &str, key: &str, depth: usize) -> Vec<u8> {
        fn var(out: &mut Vec<u8>, mut n: u64) {
            while n >= 0x80 {
                out.push((n as u8 & 0x7f) | 0x80);
                n >>= 7;
            }
            out.push(n as u8);
        }
        fn string(out: &mut Vec<u8>, s: &str) {
            var(out, s.len() as u64);
            out.extend_from_slice(s.as_bytes());
        }
        let mut u = vec![];
        var(&mut u, 1); // one client
        var(&mut u, 1); // one item
        var(&mut u, 1_234_567); // its client
        var(&mut u, 0); // its clock
        u.push(0x20 | 8); // a keyed entry holding values
        var(&mut u, 1); // under a root, by name
        string(&mut u, root);
        string(&mut u, key);
        var(&mut u, 1); // one value
        for _ in 0..depth {
            u.extend([117, 1]); // an array of one
        }
        u.extend([125, 0]); // the number 0
        var(&mut u, 0); // no deletions
        u
    }

    /// yrs decodes a value by recursing once per level, so a few hundred
    /// kilobytes of nesting would overflow a tokio worker's stack, which
    /// aborts the process. Such an update is dropped on a markdown page and
    /// refused on a tree page, on a worker-sized stack.
    #[test]
    fn an_update_nesting_values_past_the_limit_is_dropped_not_a_crash() {
        std::thread::Builder::new()
            .stack_size(2 << 20)
            .spawn(|| {
                let shallow = nested_value_update("notes", "k", 3);
                let deep = nested_value_update("notes", "k", 100_000);

                let mut markdown = page_doc(doc_from_text("x"));
                assert!(apply_yjs_update(&mut markdown, &shallow).is_some());
                assert_eq!(admit_update(&markdown, &deep), Ok(()));
                assert!(apply_yjs_update(&mut markdown, &deep).is_none());
                assert_eq!(read_content(&markdown.doc), "x");

                let o = virtues_document::parse_html("<p>x</p>", "doc");
                let tree = page_doc(virtues_document::doc_from_nodes(o.nodes));
                let refused = admit_update(&tree, &deep).unwrap_err();
                assert!(refused.contains("nests more than"), "{refused}");
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
