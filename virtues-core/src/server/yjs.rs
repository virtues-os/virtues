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

use axum::{
    extract::{
        ws::{Message, WebSocket},
        Path, State, WebSocketUpgrade,
    },
    response::Response,
};
use moka::sync::Cache;
use sqlx::PgPool;
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tokio::time::Instant;
use yrs::{updates::decoder::Decode, updates::encoder::Encode, Doc, GetString, ReadTxn, StateVector, Text, Transact, Update, WriteTxn};

// y-websocket message types
const MSG_SYNC: u8 = 0;
const MSG_AWARENESS: u8 = 1;

// y-websocket sync message subtypes
const MSG_SYNC_STEP1: u8 = 0;
const MSG_SYNC_STEP2: u8 = 1;
const MSG_SYNC_UPDATE: u8 = 2;

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
}

impl PageDoc {
    fn new(doc: Doc, built_at: chrono::DateTime<chrono::Utc>) -> Self {
        let (broadcast_tx, _) = broadcast::channel(256);
        Self {
            doc,
            broadcast_tx,
            last_update: Instant::now(),
            built_at,
            changes: 0,
            saved: 0,
        }
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
            text: txn
                .get_text("content")
                .map(|t| t.get_string(&txn))
                .unwrap_or_default(),
            state: txn.encode_state_as_update_v1(&StateVector::default()),
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
            Some(state) => {
                let doc = Doc::new();
                // Apply existing Yjs state (catch_unwind: yrs can panic on corrupt data)
                if let Ok(update) = Update::decode_v1(&state) {
                    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
                        let mut txn = doc.transact_mut();
                        txn.apply_update(update);
                    }));
                    if result.is_err() {
                        tracing::error!("yrs panic in get_or_create for page {}, starting with empty doc", page_id);
                    }
                }
                doc
            }
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

/// A doc whose text is `text`: what a page with no saved CRDT state is
/// loaded as. The one way to build a page's doc from its markdown.
fn doc_from_text(text: &str) -> Doc {
    let doc = Doc::new();
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
    if let Ok(update) = Update::decode_v1(data) {
        let sv_before = doc.doc.transact().state_vector();
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut txn = doc.doc.transact_mut();
            txn.apply_update(update);
            // Only what this update deleted; text it deletes again is not here.
            !txn.delete_set().is_empty()
        }));
        let Ok(deleted) = result else {
            tracing::error!("yrs panic in apply_yjs_update, dropping update");
            return None;
        };
        doc.last_update = Instant::now();

        // Broadcast to other clients (wrapped as y-websocket update message)
        let broadcast_msg = encode_sync_update(data);
        let _ = doc.broadcast_tx.send(broadcast_msg);

        // Get current state for debounced save
        let (state, changed) = {
            let txn = doc.doc.transact();
            let changed = deleted || txn.state_vector() != sv_before;
            (txn.encode_state_as_update_v1(&yrs::StateVector::default()), changed)
        };
        let generation = if changed { doc.record_change() } else { doc.changes };
        Some((state, changed, generation))
    } else {
        None
    }
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

/// Write a variable-length unsigned integer (lib0 format)
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

/// Extract text content from Yjs state bytes (Y.Text)
pub fn extract_text_content(yjs_state: &[u8]) -> String {
    let doc = Doc::new();
    if let Ok(update) = Update::decode_v1(yjs_state) {
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut txn = doc.transact_mut();
            txn.apply_update(update);
        }));
        if result.is_err() {
            tracing::error!("yrs panic in extract_text_content, returning empty string");
            return String::new();
        }
    }

    let txn = doc.transact();
    if let Some(text) = txn.get_text("content") {
        text.get_string(&txn)
    } else {
        String::new()
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
/// Offsets are BYTES because yrs 0.18 is `OffsetKind::Bytes`. Applied
/// last-first, every edit lands at an offset that nothing before it in the
/// text has moved.
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
                        // yrs 0.18 defaults to OffsetKind::Bytes
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
        let txn = doc.doc.transact();
        Ok(txn
            .get_text("content")
            .map(|t| t.get_string(&txn))
            .unwrap_or_default())
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
        let txn = doc.doc.transact();

        if let Some(text) = txn.get_text("content") {
            Ok(text.get_string(&txn))
        } else {
            Ok(String::new())
        }
    }
}

/// Apply an editor's update to the page's doc, record the human edit, and
/// queue the save.
async fn take_client_update(
    state: &YjsState,
    page_id: &str,
    page_doc: &Arc<RwLock<PageDoc>>,
    update: &[u8],
) {
    let mut doc = page_doc.write().await;
    let Some((full_state, changed, generation)) = apply_yjs_update(&mut doc, update) else {
        return;
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
}

/// WebSocket upgrade handler for Yjs sync
pub async fn yjs_websocket_handler(
    ws: WebSocketUpgrade,
    Path(page_id): Path<String>,
    State(state): State<YjsState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_yjs_connection(socket, page_id, state))
}

/// Handle a single WebSocket connection for Yjs sync
/// Implements the y-websocket protocol
async fn handle_yjs_connection(mut socket: WebSocket, page_id: String, state: YjsState) {
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

    // Subscribe to broadcasts from other clients
    let mut broadcast_rx = {
        let doc = page_doc.read().await;
        doc.broadcast_tx.subscribe()
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
                                            let update = txn.encode_state_as_update_v1(&client_sv);
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
                                        take_client_update(&state, &page_id, &page_doc, update_bytes).await;
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
                                        take_client_update(&state, &page_id, &page_doc, update_bytes).await;
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
            else => break,
        }
    }

    tracing::debug!("WebSocket connection closed for page {}", page_id);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::pages;
    use yrs::{Doc, Transact};

    /// Helper: create a Y.Text doc with markdown content
    fn setup_doc(content: &str) -> Doc {
        doc_from_text(content)
    }

    /// Helper: read Y.Text content from a doc
    fn read_content(doc: &Doc) -> String {
        let txn = doc.transact();
        txn.get_text("content").unwrap().get_string(&txn)
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
        let client = Doc::new();
        {
            let state = page.doc.transact().encode_state_as_update_v1(&yrs::StateVector::default());
            let mut txn = client.transact_mut();
            txn.apply_update(Update::decode_v1(&state).unwrap());
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
        let client = Doc::new();
        {
            let state = page.doc.transact().encode_state_as_update_v1(&yrs::StateVector::default());
            let mut txn = client.transact_mut();
            txn.apply_update(Update::decode_v1(&state).unwrap());
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
        let client = Doc::new();
        {
            let sv = client.transact().state_vector();
            let update = server.transact().encode_state_as_update_v1(&sv);
            let mut txn = client.transact_mut();
            txn.apply_update(Update::decode_v1(&update).unwrap());
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
            txn.apply_update(Update::decode_v1(&update).unwrap());
        }
        {
            let sv = server.transact().state_vector();
            let update = client.transact().encode_state_as_update_v1(&sv);
            let mut txn = server.transact_mut();
            txn.apply_update(Update::decode_v1(&update).unwrap());
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
        let txn = doc.doc.transact();
        txn.get_text("content").unwrap().get_string(&txn)
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
        let client = Doc::new();
        {
            let state = doc.read().await.written().state;
            let mut txn = client.transact_mut();
            txn.apply_update(Update::decode_v1(&state).unwrap());
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
        take_client_update(&yjs, &page_id, &editor, &typed).await;

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
        take_client_update(&yjs, &page_id, &editor, &nothing).await;
        let other = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(Arc::ptr_eq(&editor, &other), "an open page is not a rewrite");

        let typed = typed_in_an_editor(&editor, "Your line.\n").await;
        take_client_update(&yjs, &page_id, &editor, &typed).await;
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
}
