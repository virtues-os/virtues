//! Yjs WebSocket sync server using yrs crate
//!
//! Implements the y-websocket protocol for compatibility with the y-websocket client library.
//! A page is one of two formats (`app_pages.format`, `PageFormat`): markdown
//! in a Y.Text named `content` (CodeMirror's binding), or a Yjs XML tree under
//! the document contract (Tiptap's). Every text path here routes on the
//! format the doc was loaded with (`PageDoc::format`); `content` in the
//! database is markdown either way, the tree's export for a tree page.
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
//! 6. Carets: the last awareness state each client sent is kept with the doc
//!    (`Presence`), so a client that joins late sees the others at once and a
//!    closed socket's caret goes with it.

use axum::{
    extract::{
        ws::{CloseFrame, Message, WebSocket},
        Path, Query, State, WebSocketUpgrade,
    },
    response::Response,
};
use moka::sync::Cache;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Weak};
use std::time::Duration;
use tokio::sync::{broadcast, watch, RwLock};
use tokio::time::Instant;
use virtues_document::validate::Admission;
use virtues_document::{Node, Problem};
use yrs::block::ClientID;
use yrs::sync::awareness::{AwarenessUpdate, AwarenessUpdateEntry};
use yrs::{
    updates::decoder::Decode, updates::encoder::Encode, Doc, GetString, OffsetKind, Options, ReadTxn,
    StateVector, Text, Transact, WriteTxn, XmlFragment,
};

use crate::api::pages::PageFormat;

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
/// The page moved to the other format while the client was bound (HTTP 409,
/// Conflict): the doc it edits is not the page's any more, and nothing it
/// writes there would be saved.
const CLOSE_REFORMATTED: u16 = 4409;

/// Whether a doc is still its page's doc, and if not, why
/// (`DocCache::retire`). Ordered: a doc only moves down the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Retired {
    /// It is the page's doc.
    No,
    /// The page was written outside the CRDT and loaded again. A socket
    /// still holding this doc keeps editing it until its tab reconnects.
    Rewritten,
    /// The page's format changed. No state of this doc is saved again
    /// (`SaveError::FormatChanged`), so every socket bound to it closes.
    Reformatted,
}

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
    /// The page's format (`app_pages.format`) as read when the doc was built.
    /// Every text path routes on it, and the save writes only while the row
    /// still says it (`save_and_materialize`).
    format: PageFormat,
    /// What each client on the page last said about itself: its caret, name
    /// and colour. A mutex, so a read lock on the doc is enough for cursor
    /// traffic.
    awareness: std::sync::Mutex<Presence>,
    /// Whether this is still the page's doc. Every socket bound to the doc
    /// watches it, as it watches `contract`, and the save queue reads it so
    /// a retired doc's state never takes the place of the page's own.
    retired: watch::Sender<Retired>,
}

impl PageDoc {
    fn new(doc: Doc, built_at: chrono::DateTime<chrono::Utc>, format: PageFormat) -> Self {
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
            format,
            awareness: std::sync::Mutex::new(Presence::default()),
            retired: watch::Sender::new(Retired::No),
        }
    }

    /// Mark the doc retired, and tell every socket bound to it. A doc
    /// retired for both reasons is `Reformatted`.
    fn retire(&self, why: Retired) {
        self.retired.send_if_modified(|now| {
            let moved = why > *now;
            if moved {
                *now = why;
            }
            moved
        });
    }

    fn retired(&self) -> Retired {
        *self.retired.borrow()
    }

    /// The contract version the doc is written under; 0 for a markdown page.
    pub fn contract(&self) -> u32 {
        *self.contract.borrow()
    }

    /// The page's format, as the doc was loaded.
    pub fn format(&self) -> PageFormat {
        self.format
    }

    fn awareness(&self) -> std::sync::MutexGuard<'_, Presence> {
        // Every critical section is a lookup or an insert into plain maps, so
        // a panic elsewhere cannot leave it half-written.
        self.awareness.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
    /// in a transaction of the server's own. Returns whether it changed the
    /// doc; the caller relays the repair with the edit that called for it
    /// (`apply_yjs_update`).
    fn repair(&mut self) -> bool {
        virtues_document::repair(&mut self.doc.transact_mut()) > 0
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

    /// The text, the tree for a tree page, and the full state, from one
    /// transaction. A tree is read once and its export is the text.
    fn written(&self) -> Written {
        let txn = self.doc.transact();
        let state = virtues_document::encode_state(&txn, &StateVector::default());
        match self.format {
            PageFormat::Markdown => Written {
                text: content_of(&txn),
                state,
                tree: None,
            },
            PageFormat::Tree => {
                let nodes = virtues_document::read_doc(&txn);
                Written {
                    text: virtues_document::to_markdown(&nodes),
                    state,
                    tree: Some(nodes),
                }
            }
        }
    }
}

/// A page's text and the doc state that says it, taken under one lock: what
/// a write left on the page, or what a read found there. A version cut from
/// it (`pages::cut_version`) stores exactly the text it shows, whatever an
/// open editor typed a moment later.
#[derive(Clone, PartialEq)]
pub struct Written {
    /// The page's markdown: the Y.Text, or a tree page's export.
    pub text: String,
    /// `encode_state_as_update_v1` of the whole doc: what
    /// `app_page_versions.yjs_snapshot` holds.
    pub state: Vec<u8>,
    /// A tree page's tree, block ids included. `None` for a markdown page.
    pub tree: Option<Vec<Node>>,
}

impl std::fmt::Debug for Written {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Written")
            .field("text", &self.text)
            .field("state", &format_args!("<{} bytes>", self.state.len()))
            .field("tree", &self.tree.as_ref().map(|t| format!("<{} blocks>", t.len())))
            .finish()
    }
}

/// A saved state's text: its markdown, and for a tree its tree
/// (`page_text_of_state`).
#[derive(Debug, Clone, PartialEq)]
pub struct PageText {
    /// The Y.Text, or a tree's export.
    pub markdown: String,
    /// The tree, when the state is one; `None` for markdown.
    pub tree: Option<Vec<Node>>,
}

/// What `YjsState::append_markdown` left on the page.
#[derive(Debug, Clone)]
pub struct Appended {
    /// The page's markdown after the append: the Y.Text, or the export.
    pub text: String,
    /// What converting the markdown into blocks changed on the way in, for
    /// a tree page; empty for a markdown page.
    pub notes: Vec<Problem>,
}

/// What a block edit (`YjsState::edit_tree`) found and left.
#[derive(Debug)]
pub struct TreeEdit {
    /// The page just before the edit, read under the lock it was made under.
    pub before: Written,
    /// How the ops landed: the blocks written, merge notes.
    pub applied: virtues_document::Applied,
    /// What the edit left on the page: `Ok`, or `NotSaved` carrying the same
    /// when the save failed. The edit is applied either way.
    pub after: Result<Written, TextWriteError>,
}

/// Why a block read or edit did not happen.
#[derive(Debug, Clone)]
pub enum TreeError {
    /// The page is markdown; it is edited as text.
    NotTree,
    /// The batch was refused whole and nothing was written or relayed. The
    /// tree is the page as it is now, for the caller's reply.
    Refused { problems: Vec<Problem>, tree: Vec<Node> },
    /// The page's document could not be loaded.
    Load(String),
}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotTree => f.write_str("this page holds markdown text, not blocks; edit it as text"),
            Self::Refused { problems, .. } => {
                f.write_str("your server wrote nothing")?;
                for p in problems {
                    write!(f, "; {}: {}", p.at, p.message)?;
                }
                Ok(())
            }
            Self::Load(e) => write!(f, "couldn't load the page's document: {e}"),
        }
    }
}

impl std::error::Error for TreeError {}

/// The refusal of a write to a tree page through a Y.Text path. A Y.Text
/// written into a tree page would be a stray root no editor shows and the
/// contract refuses (`check_update`), and server writes skip `admit_update`.
const TREE_TEXT_WRITE: &str = "this page holds blocks, not markdown text; edit it by block";

/// The awareness state each client on a page last sent: its caret, name and
/// colour, as JSON the server only stores and relays. Kept so a client that
/// joins late is sent everyone at once, and a closed socket's caret can be
/// taken down for the clients that stay.
///
/// The y-protocols rule: an entry replaces what is kept for its client when
/// its clock is newer, and a null state at the kept clock removes it.
#[derive(Debug, Default)]
struct Presence {
    states: HashMap<ClientID, (u32, Arc<str>)>,
}

impl Presence {
    /// The largest state kept for one client. A caret, a name and a colour
    /// are a few hundred bytes.
    const MAX_STATE_BYTES: usize = 4 * 1024;
    /// The most clients kept for one page. Past it a new client's state is
    /// still relayed, only not kept for late joiners.
    const MAX_CLIENTS: usize = 64;

    /// Take an update a client sent. Returns the clients it gave a state, for
    /// the socket that sent it to take down when it closes.
    fn apply(&mut self, update: AwarenessUpdate) -> Vec<ClientID> {
        let mut announced = vec![];
        for (client, entry) in update.clients {
            let removing = entry.json.as_ref() == "null";
            match self.states.get(&client) {
                Some((clock, _)) if entry.clock < *clock => continue,
                Some((clock, _)) if entry.clock == *clock && !removing => continue,
                None if removing => continue,
                None if self.states.len() >= Self::MAX_CLIENTS => continue,
                _ => {}
            }
            if removing {
                self.states.remove(&client);
            } else if entry.json.len() <= Self::MAX_STATE_BYTES {
                self.states.insert(client, (entry.clock, entry.json));
                announced.push(client);
            }
        }
        announced
    }

    /// Every state kept, as one update, or `None` when nobody is here.
    fn snapshot(&self) -> Option<AwarenessUpdate> {
        if self.states.is_empty() {
            return None;
        }
        let clients = self
            .states
            .iter()
            .map(|(client, (clock, json))| {
                (
                    *client,
                    AwarenessUpdateEntry {
                        clock: *clock,
                        json: json.clone(),
                    },
                )
            })
            .collect();
        Some(AwarenessUpdate { clients })
    }

    /// Take down `clients`' states: a null state for each, one clock past the
    /// one kept, which every other client applies over the state it holds.
    /// `None` when none of them is kept.
    fn remove(&mut self, clients: &HashSet<ClientID>) -> Option<AwarenessUpdate> {
        let clients: HashMap<ClientID, AwarenessUpdateEntry> = clients
            .iter()
            .filter_map(|client| {
                let (clock, _) = self.states.remove(client)?;
                Some((
                    *client,
                    AwarenessUpdateEntry {
                        clock: clock.saturating_add(1),
                        json: Arc::from("null"),
                    },
                ))
            })
            .collect();
        (!clients.is_empty()).then_some(AwarenessUpdate { clients })
    }
}

/// An awareness message as y-websocket frames one.
fn encode_awareness(update: &AwarenessUpdate) -> Vec<u8> {
    let payload = update.encode_v1();
    let mut msg = Vec::with_capacity(1 + 5 + payload.len());
    msg.push(MSG_AWARENESS);
    write_var_uint8_array(&mut msg, &payload);
    msg
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
        //
        // A doc whose format is no longer the row's is retired too, unsaved
        // changes or not: every text path routes on the format the doc was
        // built with, and its saves no longer land
        // (`SaveError::FormatChanged`). Handed out, it would take every new
        // socket and tool call onto the old format for as long as anyone
        // typed in it. The sockets still bound to it are closed
        // (`Retired::Reformatted`).
        loop {
            let Some(doc) = self.in_memory(page_id) else { break };
            let (built_at, format) = {
                let d = doc.read().await;
                (d.built_at, d.format)
            };
            // No row: the page is gone, and the doc in hand is all there is.
            let row: Option<(bool, String)> = sqlx::query_as(
                "SELECT yjs_state IS NULL AND updated_at IS DISTINCT FROM $2, format \
                 FROM app_pages WHERE id = $1",
            )
            .bind(page_id)
            .bind(built_at)
            .fetch_optional(pool)
            .await?;
            let (rewritten, reformatted) = match row {
                Some((rewritten, now)) => (rewritten, now != format.as_str()),
                None => (false, false),
            };
            if reformatted {
                doc.read().await.retire(Retired::Reformatted);
                if self.retire(page_id, &doc) {
                    tracing::error!(
                        page_id,
                        was = format.as_str(),
                        "the page's format changed under its cached doc - dropping the doc and loading the page again"
                    );
                    break;
                }
                continue;
            }
            if !rewritten || doc.read().await.has_unsaved_changes() {
                if self.is_registered(page_id, &doc) {
                    return Ok(doc);
                }
                continue;
            }
            doc.read().await.retire(Retired::Rewritten);
            if self.retire(page_id, &doc) {
                tracing::info!(
                    page_id,
                    "page was rewritten outside the CRDT - dropping the cached doc and reseeding"
                );
                break;
            }
        }

        // A put-back is saved before the doc is handed out; when the page was
        // written meanwhile, it is read again, and the write it read is the
        // one put back. A page written on every read is not opened.
        let mut reads = 0;
        let (doc, built_at, format) = loop {
            reads += 1;
            if reads > 3 {
                anyhow::bail!("page {page_id} kept changing while your server opened it; try again");
            }
            // Content, state, timestamp and format in one read, so `built_at`
            // dates exactly the text the doc is built from.
            #[derive(sqlx::FromRow)]
            struct Row {
                content: String,
                yjs_state: Option<Vec<u8>>,
                updated_at: chrono::DateTime<chrono::Utc>,
                format: String,
            }
            let row: Option<Row> = sqlx::query_as(
                "SELECT content, yjs_state, updated_at, format FROM app_pages \
                 WHERE id = $1 AND deleted_at IS NULL",
            )
            .bind(page_id)
            .fetch_optional(pool)
            .await?;
            let Some(Row {
                content,
                yjs_state,
                updated_at: mut built_at,
                format,
            }) = row
            else {
                anyhow::bail!("Page not found: {page_id}");
            };
            let format = PageFormat::try_from(format).map_err(anyhow::Error::msg)?;

            let doc = match (format, yjs_state) {
                (PageFormat::Tree, Some(state)) => {
                    let (doc, put_back) = doc_from_tree_state(page_id, &state)?;
                    if put_back {
                        let kept = (state.as_slice(), content.as_str());
                        match save_put_back(pool, page_id, &doc, built_at, kept).await? {
                            Some(saved_at) => built_at = saved_at,
                            // Written meanwhile: load what was written.
                            None => continue,
                        }
                    }
                    doc
                }
                (PageFormat::Tree, None) => {
                    // The table's CHECK keeps a tree row from losing its state;
                    // this is the backstop. Its `content` is only the export, and
                    // a doc rebuilt from that would drop what markdown cannot hold.
                    tracing::error!(page_id, "a block page has no saved document; not opening it");
                    anyhow::bail!(
                        "page {page_id} is a block page with no saved document; it is not rebuilt from its markdown"
                    );
                }
                (PageFormat::Markdown, Some(state)) => doc_from_state(page_id, &state),
                // No Yjs state yet: the page's markdown is the doc's text.
                (PageFormat::Markdown, None) => doc_from_text(&content),
            };
            break (doc, built_at, format);
        };

        let page_doc = Arc::new(RwLock::new(PageDoc::new(doc, built_at, format)));

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

/// A new, empty doc for a markdown page. Every markdown doc the server
/// builds comes from here; a tree page's comes from the contract crate's
/// `new_doc` (`doc_from_tree_state`), whose block ops read the unit off the
/// doc.
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

/// The page's markdown as `txn` sees it: the Y.Text of a markdown page, the
/// export of a tree page.
fn markdown_of<T: ReadTxn>(txn: &T, format: PageFormat) -> String {
    match format {
        PageFormat::Markdown => content_of(txn),
        PageFormat::Tree => virtues_document::to_markdown(&virtues_document::read_doc(txn)),
    }
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

/// A tree page's doc from its saved state, or why it cannot be opened.
///
/// Built by the contract crate's `new_doc`, the doc every tree is written in
/// (UTF-16 offsets, as the browser counts). Unlike a markdown page, a state
/// that applies only in part is refused: served, the partial tree would be
/// the page every editor sees, and its next save would write it over the
/// bytes that still hold the rest. That includes a state holding edits that
/// wait on ones it lacks. So is a state without a contract stamp, which no
/// tree the server writes lacks.
///
/// A tree holding what the contract refuses (content that reached it past
/// the socket's check) is put back inside the contract before any editor is
/// served it (`virtues_document::put_back_in_contract`), and the second value
/// says so: the doc then holds a change of the server's own, which
/// `DocCache::get_or_create` saves before it hands the doc out. Unsaved, the
/// next load would put it back again under another client id, and a device
/// that synced the first would add every block again; what it held before
/// is kept as a restore point first (`save_put_back`). A tree that cannot be
/// put back is not opened. A tree stamped above this server's contract (a
/// newer server wrote it) is never put back: what this contract refuses
/// there is the newer one's content.
fn doc_from_tree_state(page_id: &str, state: &[u8]) -> anyhow::Result<(Doc, bool)> {
    let doc = virtues_document::new_doc();
    if let Err(error) = apply_v1(&doc, state) {
        tracing::error!(page_id, %error, "a block page's saved state did not load; not opening it");
        anyhow::bail!("page {page_id}'s saved document does not load ({error}); your server won't open part of it");
    }
    if doc.transact().has_missing_updates() {
        tracing::error!(page_id, "a block page's saved state holds edits waiting on ones it lacks; not opening it");
        anyhow::bail!("page {page_id}'s saved document holds edits that follow ones it lacks; your server won't open part of it");
    }
    let stamped = virtues_document::stamped_version(&doc.transact());
    match stamped {
        // A newer server wrote it (this one was rolled back), in a contract
        // this one does not know: what it refuses there is the newer one's
        // content, which putting back would delete, deletions a newer server
        // could not undo. It is served as it is; every write refuses it.
        Some(version) if version > virtues_document::contract().version => Ok((doc, false)),
        Some(version) if version >= 1 => {
            let put_back = virtues_document::put_back_in_contract(&mut doc.transact_mut());
            match put_back {
                Ok(None) => Ok((doc, false)),
                Ok(Some(refused)) => {
                    tracing::error!(
                        page_id,
                        problems = %problems_summary(&refused),
                        "a block page's saved document held what the contract refuses; put it back inside"
                    );
                    Ok((doc, true))
                }
                Err(refused) => {
                    tracing::error!(
                        page_id,
                        problems = %problems_summary(&refused),
                        "a block page's saved document holds what the contract refuses and could not be put back; not opening it"
                    );
                    anyhow::bail!("page {page_id}'s saved document holds what your server's document contract refuses, and it could not be repaired")
                }
            }
        }
        _ => {
            tracing::error!(page_id, "a block page's saved document has no contract stamp; not opening it");
            anyhow::bail!("page {page_id}'s saved document has no contract stamp");
        }
    }
}

/// Save a tree page's state as `doc_from_tree_state` put it back inside the
/// contract, with its export, unless the page was written after `read_at`,
/// when the state was read. The time it was saved, or `None` when the page
/// was written meanwhile. What it held before (`kept`: its state and its
/// export) is kept first as a restore point, as every change a machine makes
/// to a whole page is: a put-back deletes what the contract refuses.
async fn save_put_back(
    pool: &PgPool,
    page_id: &str,
    doc: &Doc,
    read_at: chrono::DateTime<chrono::Utc>,
    kept: (&[u8], &str),
) -> anyhow::Result<Option<chrono::DateTime<chrono::Utc>>> {
    crate::api::pages::create_version_from_snapshot(
        pool,
        page_id,
        kept.0,
        kept.1,
        "auto",
        Some(crate::api::pages::RESTORE_POINT),
    )
    .await
    .map_err(|e| anyhow::anyhow!("page {page_id}'s document was not kept before its repair: {e}"))?;
    let state = virtues_document::encode_state(&doc.transact(), &StateVector::default());
    let content = text_of_state_as(&state, PageFormat::Tree)
        .map_err(|e| anyhow::anyhow!("page {page_id}'s repaired document does not read: {e}"))?;
    let saved = sqlx::query_scalar(
        "UPDATE app_pages SET yjs_state = $1, content = $2, updated_at = now() \
         WHERE id = $3 AND format = $4 AND updated_at = $5 RETURNING updated_at",
    )
    .bind(&state)
    .bind(&content)
    .bind(page_id)
    .bind(PageFormat::Tree.as_str())
    .bind(read_at)
    .fetch_optional(pool)
    .await?;
    Ok(saved)
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
/// first draft. A Y.Text: only articles reach it, and the table keeps
/// articles markdown (`app_pages_tree_is_a_user_page`).
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
    /// database only with the doc's next change.
    ///
    /// A state of a different doc for the page replaces what is queued,
    /// unless it comes from a retired doc (`Retired`): the doc queued then
    /// is the page's, and its change would leave the queue while the doc
    /// still counts it unsaved, to be lost when the doc is dropped.
    async fn queue_save(
        &self,
        page_id: String,
        doc: &Arc<RwLock<PageDoc>>,
        yjs_state: Vec<u8>,
        generation: u64,
    ) {
        let retired = doc.read().await.retired() != Retired::No;
        let mut pending = self.pending.write().await;
        if let Some(queued) = pending.get(&page_id) {
            if queued.covers(doc, generation) || (retired && !Arc::ptr_eq(&queued.doc, doc)) {
                return;
            }
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
    ///
    /// A state whose doc's format the row no longer has is not written
    /// (`SaveError::FormatChanged`), and the doc counts it as saved: there is
    /// nothing to retry. The doc is marked `Retired::Reformatted`, which
    /// closes its sockets, and the next lookup of the page takes it out of
    /// the cache (`DocCache::get_or_create`).
    async fn write(
        &self,
        pool: &PgPool,
        page_id: &str,
        doc: &Arc<RwLock<PageDoc>>,
        yjs_state: &[u8],
        generation: u64,
    ) -> Result<(), SaveError> {
        let _one_at_a_time = self.writing.lock().await;
        let format = {
            let d = doc.read().await;
            if generation < d.saved {
                return Ok(());
            }
            d.format
        };
        match save_and_materialize(pool, page_id, yjs_state, format).await {
            Ok(()) => {
                doc.write().await.mark_saved(generation);
                Ok(())
            }
            Err(SaveError::FormatChanged) => {
                tracing::error!(
                    page = %page_id,
                    format = format.as_str(),
                    "the page's format changed under its doc; this state of it is not saved"
                );
                let mut d = doc.write().await;
                d.mark_saved(generation);
                // Nothing typed into it saves again: its sockets close.
                d.retire(Retired::Reformatted);
                Err(SaveError::FormatChanged)
            }
            Err(e) => Err(e),
        }
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
            // Logged by `write`; there is no format the state could be saved
            // under, so retrying cannot land it.
            if let Err(SaveError::FormatChanged) = written {
                continue;
            }
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

    // Edits each inside the contract can merge into a tree page that is
    // not (`admit_update` takes them); the server puts it right at once.
    let repaired = doc.contract() > 0 && doc.repair();

    // Relayed to every client, the edit and its repair as one update. A
    // client that applied the edit alone would hold the merge the repair
    // puts right, and Tiptap's binding deletes from the shared document a
    // node its schema refuses, with everything inside it, before the
    // repair could reach it.
    let relayed = if repaired {
        encode_sync_update(&doc.doc.transact().encode_diff_v1(&sv_before))
    } else {
        encode_sync_update(data)
    };
    let _ = doc.broadcast_tx.send(relayed);

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

/// Read a variable-length unsigned integer, returning (value, bytes_consumed).
/// `None` when the bytes end first, or when the number is too long for a
/// `usize` (a sender's ten bytes of continuation would shift past it).
fn read_var_uint(data: &[u8]) -> Option<(usize, usize)> {
    let mut value: usize = 0;
    let mut shift = 0u32;
    let mut pos = 0;

    loop {
        if pos >= data.len() || shift >= usize::BITS {
            return None;
        }
        let byte = data[pos];
        value |= ((byte & 0x7f) as usize).checked_shl(shift)?;
        pos += 1;

        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    Some((value, pos))
}

/// Whether a lib0 list's leading count is one its bytes can hold, each entry
/// taking at least `min_entry` bytes, and at most `most`. yrs reserves room
/// for the count it reads before it reads one entry, and fills that room: a
/// few bytes declaring tens of millions of entries would have it allocate
/// and touch hundreds of megabytes, every message.
fn count_fits(list: &[u8], min_entry: usize, most: usize) -> bool {
    match read_var_uint(list) {
        Some((count, used)) => count <= most && count.saturating_mul(min_entry) <= list.len() - used,
        None => false,
    }
}

/// An awareness message's update, when it reads as one a page keeps: each
/// entry is a client, a clock and a state (three bytes at least), and a page
/// keeps at most `Presence::MAX_CLIENTS`.
fn awareness_update(payload: &[u8]) -> Option<AwarenessUpdate> {
    let bytes = extract_sync_payload(payload)?;
    if !count_fits(bytes, 3, Presence::MAX_CLIENTS) {
        return None;
    }
    AwarenessUpdate::decode_v1(bytes).ok()
}

/// A client's state vector from its sync step 1: each entry is a client and
/// a clock (two bytes at least). One that does not read is empty, and the
/// client is sent the whole page.
fn client_state_vector(bytes: &[u8]) -> StateVector {
    if !count_fits(bytes, 2, usize::MAX) {
        tracing::warn!("a state vector declares more entries than it holds");
        return StateVector::default();
    }
    StateVector::decode_v1(bytes).unwrap_or_else(|e| {
        tracing::warn!("Failed to decode state vector: {}", e);
        StateVector::default()
    })
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

/// The text of a saved doc state (`app_pages.yjs_state`,
/// `app_page_versions.yjs_snapshot`), or why it could not be read.
///
/// The bytes say what they are: a state stamped with a contract version
/// (`virtues_document::stamped_version`) is a tree, read as its tree and its
/// markdown export, when its fragment holds blocks or it has no `content`
/// text; anything else is a markdown page's Y.Text. A markdown page raised
/// to a contract keeps its text, and no fragment, and reads as markdown. A
/// tree whose document also holds a `content` text (one an older binary let
/// in, empty or not) is still a tree: the blocks decide, not the stray root.
/// Every reader of a snapshot goes through here, so none of them needs to
/// know which kind of page it came from.
pub(crate) fn page_text_of_state(state: &[u8]) -> Result<PageText, ApplyError> {
    let doc = virtues_document::new_doc();
    apply_v1(&doc, state)?;
    let txn = doc.transact();
    if holds_tree(&txn) {
        let nodes = virtues_document::read_doc(&txn);
        return Ok(PageText {
            markdown: virtues_document::to_markdown(&nodes),
            tree: Some(nodes),
        });
    }
    Ok(PageText {
        markdown: content_of(&txn),
        tree: None,
    })
}

/// Whether a doc read with no page row at hand is a tree page's: stamped,
/// and either its fragment holds blocks or it has no `content` text.
fn holds_tree<T: ReadTxn>(txn: &T) -> bool {
    let stamped = virtues_document::stamped_version(txn).is_some_and(|v| v >= 1);
    let blocks = txn
        .get_xml_fragment(virtues_document::contract().fragment.as_str())
        .is_some_and(|frag| frag.len(txn) > 0);
    stamped && (blocks || txn.get_text("content").is_none())
}

/// The markdown a saved state holds as a page of `format` reads it: a tree
/// page's export, whatever other roots its document holds, or a markdown
/// page's Y.Text. For a reader that has the page's row.
pub(crate) fn text_of_state_as(state: &[u8], format: PageFormat) -> Result<String, ApplyError> {
    let doc = virtues_document::new_doc();
    apply_v1(&doc, state)?;
    let text = markdown_of(&doc.transact(), format);
    Ok(text)
}

/// The markdown of a saved doc state (`page_text_of_state`), or why it could
/// not be read.
pub(crate) fn text_of_state(yjs_state: &[u8]) -> Result<String, ApplyError> {
    page_text_of_state(yjs_state).map(|t| t.markdown)
}

/// The markdown of a saved doc state: a markdown page's text, a tree page's
/// export. Empty when the bytes are not a state or the state does not apply.
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

/// Why a page's state was not saved (`save_and_materialize`).
#[derive(Debug)]
pub(crate) enum SaveError {
    /// The row's format is no longer the one the doc was built with: a
    /// state of the old format must not land over the new one.
    FormatChanged,
    /// The state did not read, or the database refused the write.
    Other(anyhow::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FormatChanged => f.write_str(
                "the page's format changed while this edit was in flight, so your server didn't save it",
            ),
            Self::Other(e) => write!(f, "{e}"),
        }
    }
}

impl From<sqlx::Error> for SaveError {
    fn from(e: sqlx::Error) -> Self {
        Self::Other(e.into())
    }
}

/// Save a page's state and its markdown (`content`): the Y.Text, or a tree
/// page's export, so every reader of `content` reads text either way.
///
/// Written only while the row still has `format`, the format of the doc the
/// state came from: no state written under one format lands on a page that
/// has moved to the other.
async fn save_and_materialize(
    pool: &PgPool,
    page_id: &str,
    yjs_state: &[u8],
    format: PageFormat,
) -> Result<(), SaveError> {
    let content = text_of_state_as(yjs_state, format)
        .map_err(|e| SaveError::Other(anyhow::anyhow!("the page's state does not read: {e}")))?;

    let saved: Option<String> = sqlx::query_scalar(
        "UPDATE app_pages SET yjs_state = $1, content = $2, updated_at = now() \
         WHERE id = $3 AND format = $4 RETURNING id",
    )
    .bind(yjs_state)
    .bind(&content)
    .bind(page_id)
    .bind(format.as_str())
    .fetch_optional(pool)
    .await?;
    if saved.is_none() {
        // No row at all is a page purged while its doc was open: nothing to
        // save it to, as before. A row with the other format is not this
        // doc's page any more.
        let now: Option<String> = sqlx::query_scalar("SELECT format FROM app_pages WHERE id = $1")
            .bind(page_id)
            .fetch_optional(pool)
            .await?;
        if now.is_some() {
            return Err(SaveError::FormatChanged);
        }
    }

    tracing::debug!("Saved page {} ({} chars)", page_id, content.len());
    Ok(())
}

/// Why a server-side text write (`apply_text_edit`, `apply_text_diff`,
/// `replace_text`) did not simply land. The variants differ in what the page
/// says afterwards, which is the thing a caller has to report honestly.
#[derive(Debug, Clone, PartialEq)]
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
            Err(SaveError::FormatChanged) => {
                self.save_queue.dequeue(page_id, page_doc, generation).await;
                Err(TextWriteError::Other(SaveError::FormatChanged.to_string()))
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
    /// that is not on the page is `Other`, with nothing changed, and so is a
    /// tree page, which is edited by block (`edit_tree`).
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
            if doc.format == PageFormat::Tree {
                return Err(TextWriteError::Other(TREE_TEXT_WRITE.into()));
            }
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

    /// The page's current markdown, straight from the authoritative doc: the
    /// Y.Text, or a tree page's export.
    ///
    /// The editor reads this to build its prompt and hands the same string
    /// back as `expected_old`, which is what makes the staleness guard in
    /// `apply_text_diff` meaningful. Every text writer refuses a tree page,
    /// so its export is only ever read.
    pub async fn read_text(&self, page_id: &str) -> Result<String, String> {
        let page_doc = self
            .doc_cache
            .get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| format!("Failed to get page document: {e}"))?;
        let doc = page_doc.read().await;
        let text = markdown_of(&doc.doc.transact(), doc.format);
        Ok(text)
    }

    /// A tree page's tree, block ids included, from the live doc. A
    /// markdown page is `NotTree`.
    pub async fn read_tree(&self, page_id: &str) -> Result<Vec<Node>, TreeError> {
        let page_doc = self
            .doc_cache
            .get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| TreeError::Load(e.to_string()))?;
        let doc = page_doc.read().await;
        if doc.format != PageFormat::Tree {
            return Err(TreeError::NotTree);
        }
        let tree = virtues_document::read_doc(&doc.doc.transact());
        Ok(tree)
    }

    /// Apply block ops (`virtues_document::Op`) to a tree page, all or none,
    /// merged against `base`, the tree the writer read, when it has one.
    ///
    /// Under the doc's lock: a page stamped above this server's contract is
    /// refused (a newer server wrote it, and this one cannot check what it
    /// holds), and one below is raised first. The read, the check and the
    /// write are one transaction, so nothing lands between them. A refused
    /// batch writes and relays nothing, and comes back with the page as it
    /// is now. An applied batch goes to every open editor and is saved at
    /// once (`persist_now`).
    ///
    /// Callers take the page's `write_turn` themselves, around their
    /// versions.
    pub async fn edit_tree(
        &self,
        page_id: &str,
        base: Option<&[Node]>,
        ops: &[virtues_document::Op],
    ) -> Result<TreeEdit, TreeError> {
        let page_doc = self
            .doc_cache
            .get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| TreeError::Load(e.to_string()))?;

        let (before, applied, written, generation) = {
            let mut doc = page_doc.write().await;
            if doc.format != PageFormat::Tree {
                return Err(TreeError::NotTree);
            }
            let server = virtues_document::contract().version;
            if doc.contract() > server {
                let tree = virtues_document::read_doc(&doc.doc.transact());
                return Err(TreeError::Refused {
                    problems: vec![Problem::new(
                        "page",
                        "a newer version of Virtues wrote this page; update your server to edit it",
                    )],
                    tree,
                });
            }
            let raised = doc.raise_contract();
            let before = doc.written();

            let outcome = {
                let mut txn = doc.doc.transact_mut();
                let sv = txn.state_vector();
                // `apply_ops_in` checks the whole batch before it writes
                // anything, so a refusal leaves the transaction empty.
                match virtues_document::apply_ops_in(&mut txn, base, ops) {
                    Ok(applied) => {
                        let changed = txn.state_vector() != sv || !txn.delete_set().is_empty();
                        Ok((applied, changed.then(|| txn.encode_update_v1())))
                    }
                    Err(problems) => Err(problems),
                }
            };
            match outcome {
                Err(problems) => {
                    let tree = virtues_document::read_doc(&doc.doc.transact());
                    // A raise is the server's own change and stands.
                    let raised = raised.map(|generation| (doc.written().state, generation));
                    drop(doc);
                    if let Some((state, generation)) = raised {
                        self.save_queue
                            .queue_save(page_id.to_string(), &page_doc, state, generation)
                            .await;
                    }
                    return Err(TreeError::Refused { problems, tree });
                }
                Ok((applied, update)) => {
                    let generation = match update {
                        Some(update) => {
                            let _ = doc.broadcast_tx.send(encode_sync_update(&update));
                            Some(doc.record_change())
                        }
                        None => raised,
                    };
                    (before, applied, doc.written(), generation)
                }
            }
        };

        let after = match generation {
            Some(generation) => self.persist_now(page_id, &page_doc, written, generation).await,
            None => Ok(written),
        };
        Ok(TreeEdit {
            before,
            applied,
            after,
        })
    }

    /// Turn a tree page into `target`, block by block: the ops that make the
    /// page read as `target` (`virtues_document::diff_ops`), worked out and
    /// applied in one transaction under the doc's lock, so nothing lands
    /// between the read and the write. A block that does not change keeps
    /// its Yjs items, so a caret in it, or a keystroke an editor sends from
    /// before this write, still lands; a text block kept by id is patched
    /// where it changed rather than swapped. The write a restore makes, and
    /// the one a writer that produces a whole page makes.
    ///
    /// `Ok(None)` when the page already reads as `target`: nothing written.
    /// Otherwise the write goes to every open editor and is saved at once
    /// (`persist_now`), and comes back with the page just before it, read
    /// under the same lock: an editor's typing that landed after the
    /// caller's last read is in it, and nowhere else once this write has
    /// turned its block into the target's. `after` is what the write left,
    /// or `NotSaved` carrying it when the save failed. A page stamped above
    /// this server's contract, or one in markdown, is refused with nothing
    /// written. Callers take the page's `write_turn` themselves.
    pub async fn replace_tree(
        &self,
        page_id: &str,
        target: &[Node],
    ) -> Result<Option<TextEdit>, TextWriteError> {
        let page_doc = self
            .doc_cache
            .get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| TextWriteError::Other(format!("Failed to get page document: {e}")))?;

        let (before, written, generation, changed) = {
            let mut doc = page_doc.write().await;
            if doc.format != PageFormat::Tree {
                return Err(TextWriteError::Other(TreeError::NotTree.to_string()));
            }
            if doc.contract() > virtues_document::contract().version {
                return Err(TextWriteError::Other(
                    "a newer version of Virtues wrote this page; update your server to change it".into(),
                ));
            }
            let raised = doc.raise_contract();
            let before = doc.written();

            let outcome = {
                let mut txn = doc.doc.transact_mut();
                let ops = virtues_document::diff_ops(&virtues_document::read_doc(&txn), target);
                if ops.is_empty() {
                    Ok(None)
                } else {
                    // The ops are made from a tree of the server's own (a
                    // version), so no write's size limit bounds them: a page
                    // cleared to a line is put back whole. They are checked
                    // as a batch before anything is written, so a refusal
                    // leaves the transaction empty.
                    virtues_document::apply_own_ops_in(&mut txn, &ops)
                        .map(|_| Some(txn.encode_update_v1()))
                }
            };
            match outcome {
                Err(problems) => {
                    // A raise is the server's own change and stands.
                    let raised = raised.map(|generation| (doc.written().state, generation));
                    drop(doc);
                    if let Some((state, generation)) = raised {
                        self.save_queue
                            .queue_save(page_id.to_string(), &page_doc, state, generation)
                            .await;
                    }
                    return Err(TextWriteError::Other(format!(
                        "your server wrote nothing: {}",
                        problems_summary(&problems)
                    )));
                }
                Ok(Some(update)) => {
                    let _ = doc.broadcast_tx.send(encode_sync_update(&update));
                    let generation = doc.record_change();
                    (before, doc.written(), Some(generation), true)
                }
                Ok(None) => (before, doc.written(), raised, false),
            }
        };

        let after = match generation {
            Some(generation) => self.persist_now(page_id, &page_doc, written, generation).await,
            None => Ok(written),
        };
        Ok(changed.then_some(TextEdit { before, after }))
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
            if doc.format == PageFormat::Tree {
                return Err(TextWriteError::Other(TREE_TEXT_WRITE.into()));
            }
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
    /// On a markdown page, block separation is handled here: a blank line is
    /// inserted when the document is non-empty so appended markdown blocks
    /// never run together.
    ///
    /// On a tree page the markdown is converted to blocks by the contract's
    /// converter and written after the last block, any suggestion in it
    /// accepted (`parse_written_markdown`): suggestions are made by the
    /// page's owner, in the page. What the conversion changed comes back as
    /// `notes`. Markdown the converter refuses is `InvalidInput` naming its
    /// problems, with nothing written: the caller's to fix, not the server's.
    /// As every write to a tree page does (`edit_tree`, `replace_tree`), it
    /// refuses a page stamped above this server's contract, which it cannot
    /// check, and raises the stamp of one below it before writing, so no
    /// client that cannot read what it writes stays bound to the page.
    pub async fn append_markdown(
        &self,
        page_id: &str,
        markdown: &str,
    ) -> crate::error::Result<Appended> {
        let page_doc = self
            .doc_cache
            .get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| crate::error::Error::Other(format!("Failed to get page document: {}", e)))?;

        // Converted before the write lock, which open editors wait on, and
        // off the async workers, as every conversion is. A doc keeps its
        // format for its whole life, so the format read here is still the
        // doc's under that lock.
        let format = page_doc.read().await.format;
        let converted = match format {
            PageFormat::Markdown => None,
            PageFormat::Tree => {
                let markdown = markdown.to_string();
                let o = crate::api::pages::convert_off_workers(move || {
                    virtues_document::parse_written_markdown(&markdown)
                })
                .await?;
                if !o.errors.is_empty() {
                    return Err(crate::error::Error::InvalidInput(format!(
                        "Your server couldn't turn this markdown into blocks ({}), so it added nothing. Fix those parts and send it again.",
                        problems_summary(&o.errors)
                    )));
                }
                Some(o)
            }
        };

        let (written, generation, notes) = {
            let mut doc = page_doc.write().await;
            match converted {
                None => {
                    append_block_to_doc(&doc.doc, markdown);
                    let generation = doc.record_change();
                    let written = doc.written();
                    let _ = doc.broadcast_tx.send(encode_sync_update(&written.state));
                    (written, Some(generation), vec![])
                }
                Some(o) => {
                    if doc.contract() > virtues_document::contract().version {
                        return Err(crate::error::Error::InvalidInput(
                            "A newer version of Virtues wrote this page, so your server added nothing to it. Update your server to add to it."
                                .into(),
                        ));
                    }
                    // Every write is bounded by the page's size, which the
                    // converter checks only for what it converted.
                    let have = virtues_document::node_count(&virtues_document::read_doc(&doc.doc.transact()));
                    if have + virtues_document::node_count(&o.nodes) > virtues_document::MAX_NODES {
                        return Err(crate::error::Error::InvalidInput(format!(
                            "Your server added nothing: {}.",
                            virtues_document::model::too_many_nodes()
                        )));
                    }
                    let raised = doc.raise_contract();
                    let update = if o.nodes.is_empty() {
                        None
                    } else {
                        let mut txn = doc.doc.transact_mut();
                        let frag = virtues_document::ydoc::fragment(
                            virtues_document::contract(),
                            &mut txn,
                        );
                        let end = frag.len(&txn);
                        virtues_document::write_nodes(&mut txn, &frag, end, &o.nodes);
                        Some(txn.encode_update_v1())
                    };
                    // The append's change comes after the raise's, so it saves both.
                    let generation = match update {
                        Some(update) => {
                            let _ = doc.broadcast_tx.send(encode_sync_update(&update));
                            Some(doc.record_change())
                        }
                        None => raised,
                    };
                    (doc.written(), generation, o.notes)
                }
            }
        };

        if let Some(generation) = generation {
            self.save_queue
                .queue_save(page_id.to_string(), &page_doc, written.state, generation)
                .await;
        }

        Ok(Appended {
            text: written.text,
            notes,
        })
    }
}

/// The first few problems, for a refusal: each as "where: what".
pub(crate) fn problems_summary(problems: &[Problem]) -> String {
    const SHOWN: usize = 5;
    let mut out = problems
        .iter()
        .take(SHOWN)
        .map(|p| format!("{}: {}", p.at, p.message))
        .collect::<Vec<_>>()
        .join("; ");
    if problems.len() > SHOWN {
        out.push_str(&format!("; and {} more", problems.len() - SHOWN));
    }
    out
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
/// first (`virtues_document::admit_update_on_state`): once applied it would
/// be broadcast, and a stale peer would act on it before any later check. The
/// check reads the whole page, so it runs off the async workers: a worker
/// held that long stalls every socket and request it serves. The caller holds
/// the page's lock, so nothing lands on the page while it runs.
///
/// An update that holds edits following ones the page lacks is neither
/// applied nor refused (`Taken::Waits`): what waits would reach the page
/// unchecked once the page had what it waits on, and an honest client sends
/// one, since y-websocket sends a keystroke typed after a reconnect before
/// the sync that carries what it typed offline.
///
/// A markdown page (contract 0) has no contract to check its text against,
/// but its `meta` is the server's all the same: the stamp there decides the
/// page's contract when it is next loaded (`PageDoc::new`), so a client that
/// wrote one would shut every other client out. An update that does not
/// decode or apply is left to `apply_yjs_update`, which drops it, as it
/// always has on a markdown page.
async fn admit_update(doc: &PageDoc, update: &[u8]) -> Result<Taken, String> {
    if doc.contract() == 0 {
        // The check only reads the doc, so a panic inside it leaves nothing
        // half done.
        let writes = std::panic::catch_unwind(AssertUnwindSafe(|| {
            virtues_document::changes_meta(&doc.doc, update)
        }));
        return match writes {
            Ok(Ok(true)) => Err("the update writes `meta`, which only the server writes".into()),
            _ => Ok(Taken::Applied),
        };
    }
    let state = virtues_document::encode_state(&doc.doc.transact(), &StateVector::default());
    let update = update.to_vec();
    let admission = tokio::task::spawn_blocking(move || {
        virtues_document::admit_update_on_state(&state, &update)
    })
    .await
    .map_err(|e| format!("the update could not be checked: {e}"))?
    .map_err(|e| format!("the update does not apply: {e}"))?;
    match admission {
        Admission::Take => Ok(Taken::Applied),
        Admission::Waits => Ok(Taken::Waits),
        Admission::Refused(problems) => Err(refusal(&problems)),
    }
}

/// A check's problems as the reason a socket closes on.
fn refusal(problems: &[Problem]) -> String {
    let first = problems.first().map_or_else(
        || ("an unknown problem".to_string(), String::new()),
        |p| (p.message.clone(), p.at.clone()),
    );
    format!(
        "the update is outside the document contract: {} ({}){}",
        first.0,
        first.1,
        match problems.len() {
            0 | 1 => String::new(),
            n => format!(", and {} more", n - 1),
        }
    )
}

/// What came of a client's update that was not refused.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Taken {
    /// Applied, and relayed when it changed the page. On a markdown page,
    /// also an update that does not apply, which is dropped as ever.
    Applied,
    /// Not applied: it holds edits that follow ones the page lacks
    /// (`admit_update`). The socket asks the client to sync, and the
    /// client's answer carries those edits again with what they follow; an
    /// answer that waits too is asked for again only once the page changes
    /// (`Asking`).
    Waits,
}

/// Why a client's update was not taken, for the socket to close on.
#[derive(Debug, Clone, PartialEq)]
enum Refused {
    /// The update is outside what the page takes (`admit_update`).
    Update(String),
    /// The page changed format, so the doc the client is bound to is not the
    /// page's any more and nothing typed into it is saved.
    Reformatted,
}

impl Refused {
    /// The close code and the reason the socket closes with.
    fn close(&self) -> (u16, &str) {
        match self {
            Self::Update(reason) => (CLOSE_REFUSED, reason),
            Self::Reformatted => (
                CLOSE_REFORMATTED,
                "your server converted this page; open it again",
            ),
        }
    }
}

/// Apply an editor's update to the page's doc, record the human edit, and
/// queue the save. An update a tree page's contract refuses is not applied
/// or relayed, and comes back as the reason, for the socket to close on;
/// so is any update to a doc retired because its page changed format. One
/// that waits on edits the page lacks is not applied either, and comes back
/// as `Taken::Waits`, for the socket to ask its client to sync.
async fn take_client_update(
    state: &YjsState,
    page_id: &str,
    page_doc: &Arc<RwLock<PageDoc>>,
    update: &[u8],
) -> Result<Taken, Refused> {
    let mut doc = page_doc.write().await;
    // Checked under the same lock it is applied under: nothing lands between.
    if doc.retired() == Retired::Reformatted {
        return Err(Refused::Reformatted);
    }
    if admit_update(&doc, update).await.map_err(Refused::Update)? == Taken::Waits {
        return Ok(Taken::Waits);
    }
    let Some((full_state, changed, generation)) = apply_yjs_update(&mut doc, update) else {
        return Ok(Taken::Applied);
    };
    // Opening a page sends an update that changes nothing. A tree page
    // always has a saved state, so saving that again would only move
    // `updated_at` for a page nobody edited. A markdown doc may have been
    // built from `content` with no state saved yet, and its first save is
    // what writes one.
    let format = doc.format();
    drop(doc);
    if !changed && format == PageFormat::Tree {
        return Ok(Taken::Applied);
    }
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
    Ok(Taken::Applied)
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
/// A page from another origin is refused before the upgrade: the socket
/// hands whoever opens it the whole page, everyone's carets, and a way to
/// write.
pub async fn yjs_websocket_handler(
    ws: WebSocketUpgrade,
    Path(page_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<YjsState>,
    headers: axum::http::HeaderMap,
) -> Response {
    if let Some(refused) = crate::server::refuse_foreign_socket(&headers, "page") {
        return refused;
    }
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

    // Subscribe to broadcasts from other clients, to the doc's contract, and
    // to whether it is still the page's doc, all before binding: a raise or
    // a retirement between the two is still seen.
    let (mut broadcast_rx, mut contract_rx, mut retired_rx) = {
        let doc = page_doc.read().await;
        (
            doc.broadcast_tx.subscribe(),
            doc.contract.subscribe(),
            doc.retired.subscribe(),
        )
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
    if *retired_rx.borrow_and_update() == Retired::Reformatted {
        let (code, reason) = Refused::Reformatted.close();
        close_with(&mut socket, code, reason).await;
        return;
    }

    // Everyone already on the page, so their carets show at once.
    let present = page_doc.read().await.awareness().snapshot();
    if let Some(present) = present {
        if socket.send(Message::Binary(encode_awareness(&present))).await.is_err() {
            return;
        }
    }
    // The clients this socket gave a state, taken down when it closes.
    let mut announced: HashSet<ClientID> = HashSet::new();
    let mut asking = Asking::default();

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
                                            let client_sv = client_state_vector(sv_bytes);
                                            
                                            // Encode updates the client is missing
                                            let update = virtues_document::encode_state(&txn, &client_sv);
                                            encode_sync_step2(&update)
                                        };
                                        
                                        if socket.send(Message::Binary(response)).await.is_err() {
                                            break;
                                        }

                                        // Also send our state vector so client can send us their updates
                                        if socket.send(Message::Binary(asking.ask(&page_doc).await)).await.is_err() {
                                            break;
                                        }
                                    }
                                    MSG_SYNC_STEP2 => {
                                        // Client is responding to our state vector request with their updates
                                        asking.answered();
                                        // Extract the actual update from VarUint8Array format
                                        let update_bytes = match extract_sync_payload(sync_payload) {
                                            Some(bytes) => bytes,
                                            None => {
                                                tracing::warn!("Failed to extract update from sync step 2");
                                                continue;
                                            }
                                        };
                                        match take_update(binding, &state, &page_id, &page_doc, update_bytes).await {
                                            Ok(Taken::Applied) => {}
                                            Ok(Taken::Waits) => {
                                                let Some(request) = asking.waited(&page_doc, &page_id).await else {
                                                    continue;
                                                };
                                                if socket.send(Message::Binary(request)).await.is_err() {
                                                    break;
                                                }
                                            }
                                            Err(refused) => {
                                                let (code, reason) = refused.close();
                                                close_with(&mut socket, code, reason).await;
                                                break;
                                            }
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
                                        match take_update(binding, &state, &page_id, &page_doc, update_bytes).await {
                                            Ok(Taken::Applied) => {}
                                            Ok(Taken::Waits) => {
                                                let Some(request) = asking.waited(&page_doc, &page_id).await else {
                                                    continue;
                                                };
                                                if socket.send(Message::Binary(request)).await.is_err() {
                                                    break;
                                                }
                                            }
                                            Err(refused) => {
                                                let (code, reason) = refused.close();
                                                close_with(&mut socket, code, reason).await;
                                                break;
                                            }
                                        }
                                    }
                                    _ => {
                                        tracing::warn!("Unknown sync type: {}", sync_type);
                                    }
                                }
                            }
                            MSG_AWARENESS => {
                                // Carets, names, colours: kept for clients
                                // that join later, and relayed as sent.
                                let doc = page_doc.read().await;
                                let update = awareness_update(payload);
                                match update {
                                    Some(update) => announced.extend(doc.awareness().apply(update)),
                                    None => tracing::debug!(page_id, "an awareness message did not decode"),
                                }
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
                if let Some(request) = asking.page_moved(&page_doc).await {
                    if socket.send(Message::Binary(request)).await.is_err() {
                        break;
                    }
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
            // The page changed format: what the client types here is never
            // saved, and it would ride along in the save queue.
            Ok(()) = retired_rx.changed() => {
                if *retired_rx.borrow_and_update() == Retired::Reformatted {
                    tracing::info!(page_id, "closing a client on a page that changed format");
                    let (code, reason) = Refused::Reformatted.close();
                    close_with(&mut socket, code, reason).await;
                    break;
                }
            }
            else => break,
        }
    }

    // A closed tab's caret goes with its socket: the clients that stay are
    // told its state is gone.
    {
        let doc = page_doc.read().await;
        let gone = doc.awareness().remove(&announced);
        if let Some(gone) = gone {
            let _ = doc.broadcast_tx.send(encode_awareness(&gone));
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
) -> Result<Taken, Refused> {
    match binding {
        Binding::ReadWrite => {
            let taken = take_client_update(state, page_id, page_doc, update).await;
            match &taken {
                Err(refused) => {
                    tracing::warn!(page_id, reason = refused.close().1, "refused an editor's update");
                }
                Ok(Taken::Waits) => {
                    tracing::debug!(page_id, "an editor's update waits on edits the page lacks");
                }
                Ok(Taken::Applied) => {}
            }
            taken
        }
        Binding::ReadOnly => {
            tracing::debug!(page_id, "dropped an update from a client newer than this server");
            Ok(Taken::Applied)
        }
    }
}

/// A socket's sync requests to its client: the server's sync step 1, its
/// state vector, which a client answers with everything it holds that the
/// page lacks. Yjs puts the edits a client holds pending into everything it
/// encodes, so a client holding edits that follow ones neither it nor the
/// page has answers every request with an update that waits
/// (`Taken::Waits`). Asked again at once, the two would trade a check of
/// the whole page back and forth, under the page's lock, for as long as the
/// socket stayed open. So a request goes out at a state of the page once:
/// an answer at that state that waits leaves the client asked again only
/// once the page changes, and a keystroke that waits meanwhile asks nothing
/// a request at that state has not.
#[derive(Default)]
struct Asking {
    /// The page's state vector the last request went out at.
    at: Option<Vec<u8>>,
    /// That request's answer is still to come.
    outstanding: bool,
    /// Its answer waited: the client is asked again once the page changes.
    stalled: bool,
}

impl Asking {
    /// A request at the page's state now.
    async fn ask(&mut self, page_doc: &Arc<RwLock<PageDoc>>) -> Vec<u8> {
        let sv = page_state(page_doc).await;
        let request = encode_sync_step1(&sv);
        self.at = Some(sv);
        self.outstanding = true;
        self.stalled = false;
        request
    }

    /// The client answered a request (its sync step 2).
    fn answered(&mut self) {
        self.outstanding = false;
    }

    /// The request to send for an update that waited, if any: none while a
    /// request at the page's state as it is now is unanswered, or was
    /// answered with what waits.
    async fn waited(&mut self, page_doc: &Arc<RwLock<PageDoc>>, page_id: &str) -> Option<Vec<u8>> {
        let sv = page_state(page_doc).await;
        if self.at.as_deref() != Some(sv.as_slice()) {
            return Some(self.ask(page_doc).await);
        }
        if !self.outstanding && !self.stalled {
            self.stalled = true;
            tracing::warn!(
                page_id,
                "an editor holds edits that wait on edits neither it nor the page has; it is asked to sync again once the page changes"
            );
        }
        None
    }

    /// The request to send now the page may have changed: a stalled
    /// client's, once it has.
    async fn page_moved(&mut self, page_doc: &Arc<RwLock<PageDoc>>) -> Option<Vec<u8>> {
        if !self.stalled || self.at.as_deref() == Some(page_state(page_doc).await.as_slice()) {
            return None;
        }
        Some(self.ask(page_doc).await)
    }
}

/// The page's state vector, encoded.
async fn page_state(page_doc: &Arc<RwLock<PageDoc>>) -> Vec<u8> {
    page_doc.read().await.doc.transact().state_vector().encode_v1()
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

    /// The check of a block page's update reads the whole page, so it runs
    /// off the async workers: the worker that took the socket's message
    /// goes on serving everything else meanwhile.
    #[tokio::test(flavor = "current_thread")]
    async fn a_block_pages_update_is_checked_off_the_async_workers() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let o = virtues_document::parse_html(&"<p>Coffee at nine.</p>".repeat(20_000), "doc");
        let doc = virtues_document::doc_from_nodes(o.nodes);
        let peer = virtues_document::new_doc();
        apply_v1(&peer, &doc.transact().encode_state_as_update_v1(&StateVector::default())).unwrap();
        let before = peer.transact().state_vector();
        {
            use yrs::{Text, XmlFragment, XmlOut};
            let mut txn = peer.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            if let Some(XmlOut::Element(p)) = frag.get(&txn, 0) {
                if let Some(XmlOut::Text(t)) = p.get(&txn, 0) {
                    t.insert(&mut txn, 0, "Oh, ");
                }
            }
        }
        let update = peer.transact().encode_diff_v1(&before);
        let page = page_doc(doc);
        assert_eq!(page.format(), PageFormat::Tree);

        let ticks = Arc::new(AtomicUsize::new(0));
        let ticker = {
            let ticks = ticks.clone();
            tokio::spawn(async move {
                loop {
                    ticks.fetch_add(1, Ordering::SeqCst);
                    tokio::task::yield_now().await;
                }
            })
        };
        tokio::task::yield_now().await;
        let start = ticks.load(Ordering::SeqCst);
        assert_eq!(admit_update(&page, &update).await, Ok(Taken::Applied));
        let during = ticks.load(Ordering::SeqCst) - start;
        ticker.abort();
        assert!(during > 0, "the worker served nothing while the update was checked");
    }

    /// A few bytes declaring millions of entries are not decoded: yrs would
    /// reserve and fill room for every one before reading the first. A
    /// client's own caret, and a state vector that holds what it declares,
    /// read as ever; a number longer than a `usize` is no number.
    #[test]
    fn a_message_declaring_more_entries_than_it_holds_is_not_decoded() {
        let mut count = vec![];
        write_var_uint(&mut count, 1 << 26);
        assert_eq!(count.len(), 4);
        let mut payload = vec![];
        write_var_uint8_array(&mut payload, &count);
        assert!(awareness_update(&payload).is_none());
        assert_eq!(client_state_vector(&count), StateVector::default());
        assert!(!count_fits(&count, 2, usize::MAX));

        let mut over = vec![];
        write_var_uint(&mut over, Presence::MAX_CLIENTS + 1);
        over.extend(std::iter::repeat_n(0u8, 3 * (Presence::MAX_CLIENTS + 1)));
        assert!(!count_fits(&over, 3, Presence::MAX_CLIENTS));

        let caret = AwarenessUpdate {
            clients: HashMap::from([(
                ClientID::new(7),
                AwarenessUpdateEntry { clock: 1, json: Arc::from("{\"user\":{\"name\":\"Nick\"}}") },
            )]),
        };
        let mut payload = vec![];
        write_var_uint8_array(&mut payload, &caret.encode_v1());
        assert_eq!(awareness_update(&payload).map(|u| u.clients.len()), Some(1));

        let doc = setup_doc("Coffee.");
        let sv = doc.transact().state_vector();
        assert_eq!(client_state_vector(&sv.encode_v1()), sv);

        assert_eq!(read_var_uint(&[0xff; 11]), None);
    }

    /// Helper: read Y.Text content from a doc
    fn read_content(doc: &Doc) -> String {
        content_of(&doc.transact())
    }

    // ── the claim signal: only a CHANGING update counts as an edit ──────────

    /// Helper: a PageDoc around an existing Doc, for apply_yjs_update: a
    /// tree page when the doc is a stamped tree, as `page_text_of_state`
    /// reads one, else markdown.
    fn page_doc(doc: Doc) -> PageDoc {
        let format = if holds_tree(&doc.transact()) {
            PageFormat::Tree
        } else {
            PageFormat::Markdown
        };
        PageDoc::new(doc, chrono::Utc::now(), format)
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
                format: None,
            },
        )
        .await
        .unwrap();
        let state = {
            let doc = setup_doc(content);
            let txn = doc.transact();
            txn.encode_state_as_update_v1(&StateVector::default())
        };
        save_and_materialize(pool, &page.id, &state, PageFormat::Markdown)
            .await
            .unwrap();
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
                format: None,
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
    fn every_markdown_doc_counts_text_in_bytes() {
        assert_eq!(new_doc().offset_kind(), OffsetKind::Bytes);
        for doc in [doc_from_text(MIXED), doc_from_state("p", &state_from_text(MIXED))] {
            assert_eq!(doc.offset_kind(), OffsetKind::Bytes);
            let txn = doc.transact();
            assert_eq!(txn.get_text("content").unwrap().len(&txn), MIXED.len() as u32);
        }
    }

    /// A tree doc counts as the browser does, UTF-16 units: the block ops
    /// read the unit off the doc, and the browser's offsets are the ones a
    /// tree's relative positions are shared in.
    #[test]
    fn every_tree_doc_counts_text_in_utf16() {
        let o = virtues_document::parse_markdown(MIXED);
        let state = virtues_document::encode_state(
            &virtues_document::doc_from_nodes(o.nodes).transact(),
            &StateVector::default(),
        );
        let (doc, _) = doc_from_tree_state("p", &state).unwrap();
        assert_eq!(doc.offset_kind(), OffsetKind::Utf16);
        assert!(doc.client_id().get() <= u64::from(u32::MAX));
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
            assert_eq!(
                page_text_of_state(state).unwrap(),
                PageText { markdown: text.into(), tree: None },
                "{name}: shared"
            );
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

    /// A tree page whose document holds `html`, under the current contract.
    async fn tree_page(pool: &PgPool, html: &str) -> String {
        let page_id = unsaved_page(pool, "").await;
        let o = virtues_document::parse_html(html, "doc");
        assert!(o.errors.is_empty(), "{:?}", o.errors);
        let doc = virtues_document::doc_from_nodes(o.nodes);
        let state = doc.transact().encode_state_as_update_v1(&StateVector::default());
        let export = virtues_document::to_markdown(&virtues_document::read_doc(&doc.transact()));
        sqlx::query(
            "UPDATE app_pages SET yjs_state = $1, content = $2, format = 'tree' WHERE id = $3",
        )
        .bind(&state)
        .bind(&export)
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
            Self::connect_with(addr, page_id, contract, new_doc()).await
        }

        /// Connect with a doc this client already holds, as a tab does when
        /// its socket opens again.
        async fn connect_with(addr: std::net::SocketAddr, page_id: &str, contract: Option<u32>, doc: Doc) -> Self {
            let query = contract.map(|c| format!("?contract={c}")).unwrap_or_default();
            let (socket, _) =
                tokio_tungstenite::connect_async(format!("ws://{addr}/ws/yjs/{page_id}{query}"))
                    .await
                    .unwrap();
            let mut client = Client { socket, doc };
            let sv = client.doc.transact().state_vector().encode_v1();
            client.send(encode_sync_step1(&sv)).await;
            client
        }

        /// Answer the server's sync step 1, as y-websocket does: everything
        /// this client holds that the page lacks.
        async fn answer(&mut self, yjs: &YjsState, page_id: &str) {
            let server = {
                let held = yjs.doc_cache.get_or_create(page_id, &yjs.pool).await.unwrap();
                let held = held.read().await;
                let sv = held.doc.transact().state_vector();
                sv
            };
            let update = self.doc.transact().encode_diff_v1(&server);
            self.send(encode_sync_step2(&update)).await;
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

        /// The next awareness message the server sends, past any sync
        /// message; `None` when none comes or the socket closes.
        async fn hear_awareness(&mut self) -> Option<AwarenessUpdate> {
            use futures::StreamExt;
            use tokio_tungstenite::tungstenite::Message as Ws;
            loop {
                let next = tokio::time::timeout(Duration::from_millis(500), self.socket.next())
                    .await
                    .ok()?;
                match next {
                    Some(Ok(Ws::Binary(data))) => {
                        let Some((MSG_AWARENESS, payload)) = parse_message(&data) else {
                            continue;
                        };
                        let bytes = extract_sync_payload(payload).expect("framed");
                        return Some(AwarenessUpdate::decode_v1(bytes).expect("an awareness update"));
                    }
                    Some(Ok(_)) => continue,
                    Some(Err(_)) | None => return None,
                }
            }
        }

        /// Send an awareness state for `client`, as y-websocket does.
        async fn announce(&mut self, client: u64, clock: u32, json: &str) {
            self.send(encode_awareness(&presence(&[(client, clock, json)])))
                .await;
        }
    }

    /// An awareness update of `(client, clock, json)` entries.
    fn presence(entries: &[(u64, u32, &str)]) -> AwarenessUpdate {
        AwarenessUpdate {
            clients: entries
                .iter()
                .map(|(client, clock, json)| {
                    (
                        ClientID::new(*client),
                        AwarenessUpdateEntry {
                            clock: *clock,
                            json: Arc::from(*json),
                        },
                    )
                })
                .collect(),
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
        // What is saved is the keystroke, and no text root beside the tree.
        run_the_save_loop(&yjs).await;
        let saved: Vec<u8> = sqlx::query_scalar("SELECT yjs_state FROM app_pages WHERE id = $1")
            .bind(&page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let (saved_doc, _) = doc_from_tree_state(&page_id, &saved).unwrap();
        assert!(saved_doc.transact().get_text("content").is_none());
        assert_eq!(first_paragraph(&saved_doc), "Oh, Hello");
        assert_eq!(stored(&pool, &page_id).await.0, "Oh, Hello\n", "the export");
    }

    /// Items written after one made up under the server's own id would wait,
    /// out of the check's sight, until the server's next write supplied
    /// that item, and then reach the page unchecked: a `<script>` and a
    /// `javascript:` link, locking every editor out of the page as they did.
    /// The update is not applied (its client is asked to sync instead); the
    /// server's write and the next editor's keystroke land as ever.
    #[sqlx::test]
    async fn an_update_waiting_on_an_item_the_page_lacks_is_not_applied(pool: PgPool) {
        use yrs::{Text, XmlElementPrelim, XmlFragment, XmlTextPrelim};
        let page_id = tree_page(&pool, "<p>Hello</p>").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;
        let mut attacker = Client::connect(addr, &page_id, Some(version)).await;
        let mut typist = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(attacker.hear().await, Heard::Synced);
        assert_eq!(typist.hear().await, Heard::Synced);

        let server = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap().read().await.doc.client_id();
        let copy = |client| {
            let doc = Doc::with_options(Options { client_id: client, ..Options::default() });
            let state = attacker.doc.transact().encode_state_as_update_v1(&StateVector::default());
            apply_v1(&doc, &state).unwrap();
            doc
        };
        let fake = copy(server);
        {
            let mut txn = fake.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            frag.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
        }
        let faked = fake.transact().state_vector();
        let crafted = Doc::new();
        apply_v1(&crafted, &fake.transact().encode_state_as_update_v1(&StateVector::default())).unwrap();
        {
            let mut txn = crafted.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            frag.push_back(&mut txn, XmlElementPrelim::empty("script"));
            let p = frag.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
            let text = p.push_back(&mut txn, XmlTextPrelim::new(""));
            let link = std::collections::HashMap::from([(
                std::sync::Arc::<str>::from("link"),
                yrs::Any::from(std::collections::HashMap::from([(
                    "href".to_string(),
                    yrs::Any::from("javascript:alert(document.domain)"),
                )])),
            )]);
            text.insert_with_attributes(&mut txn, 0, "click me", link.into_iter().collect());
        }
        let update = crafted.transact().encode_diff_v1(&faked);
        attacker.send(encode_sync_update(&update)).await;
        let quiet = tokio::time::timeout(Duration::from_millis(300), typist.hear()).await;
        assert!(quiet.is_err(), "nothing relayed: {quiet:?}");

        // The server writes, which would have supplied the item waited on.
        let base = yjs.read_tree(&page_id).await.unwrap();
        yjs.edit_tree(&page_id, Some(&base), &[virtues_document::Op::Append { html: "<p>More.</p>".into() }])
            .await
            .unwrap();
        let held = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        {
            let held = held.read().await;
            assert!(!held.doc.transact().has_missing_updates());
            assert_eq!(virtues_document::validate_doc(&held.doc.transact()), []);
        }
        // An editor's keystroke is taken.
        assert_eq!(typist.hear().await, Heard::Update);
        typist.edit(|txn| type_into_first_paragraph(txn, "Oh, ")).await;
        let quiet = tokio::time::timeout(Duration::from_millis(300), typist.close()).await;
        assert!(quiet.is_err(), "the typist was not closed: {quiet:?}");
        let html = virtues_document::to_html(&yjs.read_tree(&page_id).await.unwrap(), false);
        assert_eq!(html, "<p>Oh, Hello</p><p>More.</p>");
    }

    /// A device that typed offline reconnects, and the person types again
    /// before y-websocket has sent the sync that carries the offline edits:
    /// the keystroke reaches the server first, waiting on them. The socket
    /// stays open and asks for a sync, and once the device's sync lands,
    /// both edits are on the page.
    #[sqlx::test]
    async fn a_keystroke_ahead_of_a_reconnects_offline_edits_lands_with_them(pool: PgPool) {
        use yrs::{Text, XmlFragment, XmlOut};
        let page_id = tree_page(&pool, "<p>Hello</p>").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;
        let mut first = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(first.hear().await, Heard::Synced);
        let device = std::mem::replace(&mut first.doc, new_doc());
        drop(first);

        let type_at_end = |typed: &str| {
            let mut txn = device.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(XmlOut::Element(p)) = frag.get(&txn, 0) else {
                panic!("a paragraph")
            };
            let Some(XmlOut::Text(t)) = p.get(&txn, 0) else {
                panic!("its text")
            };
            let end = t.len(&txn);
            t.insert(&mut txn, end, typed);
        };
        // Offline: nothing is sent.
        type_at_end(" world");

        // The socket opens again: sync step 1, then the keystroke at once.
        let mut again = Client::connect_with(addr, &page_id, Some(version), device).await;
        let before = again.doc.transact().state_vector();
        {
            let doc = &again.doc;
            let mut txn = doc.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(XmlOut::Element(p)) = frag.get(&txn, 0) else {
                panic!("a paragraph")
            };
            let Some(XmlOut::Text(t)) = p.get(&txn, 0) else {
                panic!("its text")
            };
            let end = t.len(&txn);
            t.insert(&mut txn, end, "!");
        }
        let keystroke = again.doc.transact().encode_diff_v1(&before);
        again.send(encode_sync_update(&keystroke)).await;
        assert_eq!(again.hear().await, Heard::Synced);

        again.answer(&yjs, &page_id).await;
        let quiet = tokio::time::timeout(Duration::from_millis(300), again.close()).await;
        assert!(quiet.is_err(), "the socket stayed open: {quiet:?}");
        let html = virtues_document::to_html(&yjs.read_tree(&page_id).await.unwrap(), false);
        assert_eq!(html, "<p>Hello world!</p>");
    }

    /// Read what the server says for `window`, applying the state it sends,
    /// and answer each sync request as Yjs does: with everything the client
    /// holds that the page lacks, the edits it holds `pending` included.
    /// The requests heard.
    async fn answer_as_yjs(client: &mut Client, pending: &[u8], window: Duration) -> usize {
        use futures::StreamExt;
        use tokio_tungstenite::tungstenite::Message as Ws;
        let deadline = Instant::now() + window;
        let mut asked = 0;
        loop {
            let Ok(next) = tokio::time::timeout_at(deadline, client.socket.next()).await else {
                return asked;
            };
            let Some(Ok(Ws::Binary(data))) = next else {
                if matches!(next, Some(Ok(_))) {
                    continue;
                }
                return asked;
            };
            let Some((MSG_SYNC, payload)) = parse_message(&data) else {
                continue;
            };
            let Some((kind, rest)) = parse_sync_message(payload) else {
                continue;
            };
            let body = extract_sync_payload(rest).unwrap();
            if kind == MSG_SYNC_STEP1 {
                asked += 1;
                let diff = client.doc.transact().encode_diff_v1(&client_state_vector(body));
                let answer = yrs::merge_updates_v1([diff.as_slice(), pending]).unwrap();
                client.send(encode_sync_step2(&answer)).await;
            } else {
                apply_v1(&client.doc, body).unwrap();
            }
        }
    }

    /// A device that missed a relayed keystroke holds the next one pending,
    /// waiting on it, and the server lost both (it went down before saving
    /// them). Yjs puts pending edits into every update it encodes, so the
    /// device answers each sync request with an update that waits. The
    /// socket asks once per state of the page, not again at once each time
    /// (the two traded a check of the whole page back and forth), and asks
    /// again once the page changes: when another client brings the missed
    /// keystroke, the device's edits land with it.
    #[sqlx::test]
    async fn a_client_whose_answer_waits_is_asked_again_once_the_page_changes(pool: PgPool) {
        use yrs::{Text, XmlFragment, XmlOut};
        let page_id = tree_page(&pool, "<p>Hello</p>").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;
        let mut first = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(first.hear().await, Heard::Synced);
        let device = std::mem::replace(&mut first.doc, new_doc());
        drop(first);

        let type_at = |doc: &Doc, at: u32, typed: &str| {
            let before = doc.transact().state_vector();
            {
                let mut txn = doc.transact_mut();
                let frag = txn.get_or_insert_xml_fragment("doc");
                let Some(XmlOut::Element(p)) = frag.get(&txn, 0) else {
                    panic!("a paragraph")
                };
                let Some(XmlOut::Text(t)) = p.get(&txn, 0) else {
                    panic!("its text")
                };
                let at = if at == u32::MAX { t.len(&txn) } else { at };
                t.insert(&mut txn, at, typed);
            }
            doc.transact().encode_diff_v1(&before)
        };
        // Another device types twice; this one hears only the second keystroke.
        let other = new_doc();
        apply_v1(&other, &device.transact().encode_diff_v1(&StateVector::default())).unwrap();
        let missed = type_at(&other, 0, "X");
        let pending = type_at(&other, 1, "Y");
        apply_v1(&device, &pending).unwrap();
        type_at(&device, u32::MAX, " world");

        let mut again = Client::connect_with(addr, &page_id, Some(version), device).await;
        let asked = answer_as_yjs(&mut again, &pending, Duration::from_millis(800)).await;
        assert_eq!(asked, 1, "the socket asked again at a state its answer already waited at");
        let html = virtues_document::to_html(&yjs.read_tree(&page_id).await.unwrap(), false);
        assert_eq!(html, "<p>Hello</p>");

        // The missed keystroke arrives from another client: the page changed,
        // and the device is asked again.
        let mut bringer = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(bringer.hear().await, Heard::Synced);
        bringer.send(encode_sync_update(&missed)).await;
        let asked = answer_as_yjs(&mut again, &pending, Duration::from_millis(800)).await;
        assert_eq!(asked, 1);
        let html = virtues_document::to_html(&yjs.read_tree(&page_id).await.unwrap(), false);
        assert_eq!(html, "<p>XYHello world</p>");
    }

    /// A saved tree holding what the contract refuses (content that reached
    /// it past the socket's check) is put back inside the contract when the
    /// page is opened: no editor is served it, and its text stays.
    #[sqlx::test]
    async fn a_saved_tree_outside_the_contract_is_put_back_when_opened(pool: PgPool) {
        use yrs::{Text, XmlElementPrelim, XmlFragment, XmlTextPrelim};
        let page_id = tree_page(&pool, "<p>Hello</p>").await;
        let (doc, _) = doc_from_tree_state(&page_id, &saved_state(&pool, &page_id).await.unwrap()).unwrap();
        {
            let mut txn = doc.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            let p = frag.push_back(&mut txn, XmlElementPrelim::empty("paragraph"));
            let text = p.push_back(&mut txn, XmlTextPrelim::new(""));
            let link = std::collections::HashMap::from([(
                std::sync::Arc::<str>::from("link"),
                yrs::Any::from(std::collections::HashMap::from([(
                    "href".to_string(),
                    yrs::Any::from("javascript:alert(document.domain)"),
                )])),
            )]);
            text.insert_with_attributes(&mut txn, 0, "click me", link.into_iter().collect());
        }
        let bad = doc.transact().encode_state_as_update_v1(&StateVector::default());
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(&bad)
            .bind(&page_id)
            .execute(&pool)
            .await
            .unwrap();
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;
        {
            let held = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
            let held = held.read().await;
            assert_eq!(virtues_document::validate_doc(&held.doc.transact()), []);
            let html = virtues_document::to_html(&virtues_document::read_doc(&held.doc.transact()), false);
            assert_eq!(html, "<p>Hello</p><p>click me</p>");
        }
        // It is saved as put back, so the next load reads it as it is.
        let (_, put_back) = doc_from_tree_state(&page_id, &saved_state(&pool, &page_id).await.unwrap()).unwrap();
        assert!(!put_back);
        assert!(!stored(&pool, &page_id).await.0.contains("javascript:"));
        // What it held before is kept, a restore point, to be put back from History.
        let kept: Vec<(Vec<u8>, String, Option<String>)> = sqlx::query_as(
            "SELECT yjs_snapshot, created_by, description FROM app_page_versions WHERE page_id = $1",
        )
        .bind(&page_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].0, bad);
        assert!(crate::api::pages::is_restore_point(&kept[0].1, kept[0].2.as_deref()));

        // A device syncs the page; then it is opened again in a server
        // with nothing cached (a restart), and the device syncs again.
        let mut device = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(device.hear().await, Heard::Synced);
        let doc = std::mem::replace(&mut device.doc, new_doc());
        drop(device);
        let restarted = YjsState::new(pool.clone());
        let addr = serve(restarted.clone()).await;
        let mut device = Client::connect_with(addr, &page_id, Some(version), doc).await;
        assert_eq!(device.hear().await, Heard::Synced);
        device.answer(&restarted, &page_id).await;
        let quiet = tokio::time::timeout(Duration::from_millis(300), device.close()).await;
        assert!(quiet.is_err(), "the socket stayed open: {quiet:?}");
        let html = virtues_document::to_html(&restarted.read_tree(&page_id).await.unwrap(), false);
        assert_eq!(html, "<p>Hello</p><p>click me</p>", "no block twice");
    }

    /// Text written into a `content` root and deleted again in one update
    /// leaves that root empty, and in the document for good. It is refused
    /// all the same.
    #[sqlx::test]
    async fn an_update_that_writes_a_text_root_and_empties_it_is_refused(pool: PgPool) {
        let page_id = tree_page(&pool, "<p>Hello</p>").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;

        let mut writer = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(writer.hear().await, Heard::Synced);
        writer
            .edit(|txn| {
                let text = txn.get_or_insert_text("content");
                text.insert(txn, 0, "x");
                text.remove_range(txn, 0, 1);
            })
            .await;
        assert_eq!(writer.close().await, Heard::Closed(CLOSE_REFUSED));

        let held = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(held.read().await.doc.transact().get_text("content").is_none());
    }

    /// A tree page whose saved document also holds a `content` text: one
    /// written and emptied in a single update (`typed` is `None`), or one an
    /// older binary's editor typed into.
    async fn tree_page_with_a_text_root(pool: &PgPool, html: &str, typed: Option<&str>) -> String {
        let page_id = tree_page(pool, html).await;
        let doc = virtues_document::new_doc();
        doc.transact_mut()
            .apply_update(Update::decode_v1(&saved_state(pool, &page_id).await.unwrap()).unwrap())
            .unwrap();
        {
            let mut txn = doc.transact_mut();
            let text = txn.get_or_insert_text("content");
            text.insert(&mut txn, 0, typed.unwrap_or("x"));
            if typed.is_none() {
                text.remove_range(&mut txn, 0, 1);
            }
        }
        let state = doc.transact().encode_state_as_update_v1(&StateVector::default());
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(&state)
            .bind(&page_id)
            .execute(pool)
            .await
            .unwrap();
        page_id
    }

    /// The blocks decide whether a state is a tree, not whether a `content`
    /// text is there: such a page saves its export, its versions read as
    /// trees, and putting one back puts back its blocks.
    #[sqlx::test]
    async fn a_tree_whose_document_holds_a_text_root_is_read_saved_and_restored_as_a_tree(pool: PgPool) {
        const HTML: &str = "<h2>Trip</h2><p>Lunch with Nick.</p><p>Book seats.</p>";
        const EXPORT: &str = "## Trip\n\nLunch with Nick.\n\nBook seats.\n";
        for typed in [None, Some("# typed into the old editor")] {
            let page_id = tree_page_with_a_text_root(&pool, HTML, typed).await;
            let state = saved_state(&pool, &page_id).await.unwrap();
            let read = page_text_of_state(&state).unwrap();
            assert_eq!(read.markdown, EXPORT, "{typed:?}");
            assert_eq!(read.tree.map(|t| t.len()), Some(3), "{typed:?}");
            assert_eq!(extract_text_content(&state), EXPORT, "{typed:?}");

            let yjs = YjsState::new(pool.clone());
            let v1 = pages::cut_version_now(&pool, &yjs, &page_id, "user", Some("Saved"))
                .await
                .unwrap();
            let shown = pages::get_version(&pool, &v1.id).await.unwrap();
            assert_eq!(shown.format, PageFormat::Tree, "{typed:?}");
            assert_eq!(shown.markdown, EXPORT, "{typed:?}");
            assert!(shown.html.is_some(), "{typed:?}");

            // A block edit is saved, its export the `content` column.
            let tree = yjs.read_tree(&page_id).await.unwrap();
            yjs.edit_tree(
                &page_id,
                Some(&tree),
                &[virtues_document::Op::Replace {
                    id: block_ids(&tree)[2].clone(),
                    html: "<p>Seats booked.</p>".into(),
                }],
            )
            .await
            .unwrap()
            .after
            .unwrap();
            assert_eq!(
                stored(&pool, &page_id).await.0,
                "## Trip\n\nLunch with Nick.\n\nSeats booked.\n",
                "{typed:?}"
            );

            let restored = pages::restore_version(&pool, &yjs, &page_id, &v1.id).await.unwrap();
            assert!(restored.changed, "{typed:?}");
            assert_eq!(
                texts(&yjs.read_tree(&page_id).await.unwrap()),
                ["Trip", "Lunch with Nick.", "Book seats."],
                "{typed:?}"
            );
            assert_eq!(stored(&pool, &page_id).await.0, EXPORT, "{typed:?}");
        }
    }

    /// A version that holds nothing is not put back over a block page: it
    /// would leave one empty paragraph where every block was.
    #[sqlx::test]
    async fn an_empty_version_is_not_put_back_over_a_block_page(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "## Trip\n\nLunch with Nick.\n").await;
        let yjs = YjsState::new(pool.clone());
        let empty = pages::create_version_from_snapshot(&pool, &page_id, &state_from_text(""), "", "auto", None)
            .await
            .unwrap();
        let err = pages::restore_version(&pool, &yjs, &page_id, &empty.id)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("is empty"), "{err}");
        assert_eq!(texts(&yjs.read_tree(&page_id).await.unwrap()), ["Trip", "Lunch with Nick."]);
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

    /// One device suggests deleting a sentence; another, offline, pastes a
    /// suggested insertion inside it. Each edit is inside the contract on
    /// its own device; merged, the inserted words carry both suggestions,
    /// since a mark is positional in Yjs. Refused, the second device's
    /// socket closed and its stored copy, with every edit it had not synced,
    /// was cleared. Taken: the server takes the deletion off those words,
    /// every device follows, and the page is saved inside the contract.
    #[sqlx::test]
    async fn a_suggestion_pasted_inside_a_suggested_deletion_is_taken_and_repaired(pool: PgPool) {
        use yrs::types::Attrs;
        use yrs::{Any, Text, XmlFragment, XmlOut, XmlTextRef};
        let page_id = tree_page(&pool, "<p>hello world</p>").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;
        let mut laptop = Client::connect(addr, &page_id, Some(version)).await;
        let mut phone = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(laptop.hear().await, Heard::Synced);
        assert_eq!(phone.hear().await, Heard::Synced);

        fn text_of(txn: &mut yrs::TransactionMut) -> XmlTextRef {
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(XmlOut::Element(p)) = frag.get(txn, 0) else { panic!("a paragraph") };
            let Some(XmlOut::Text(t)) = p.get(txn, 0) else { panic!("its text") };
            t
        }
        fn proposal(key: &str, id: &str) -> Attrs {
            let mut attrs = Attrs::new();
            let mut inner = std::collections::HashMap::new();
            inner.insert("proposal".to_string(), Any::from(id));
            attrs.insert(key.into(), Any::from(inner));
            attrs
        }
        // Ask AI's rewrite on the laptop; a copied suggestion pasted on the phone.
        laptop
            .edit(|txn| {
                let t = text_of(txn);
                t.format(txn, 0, 11, proposal("proposedDeletion", "p1"));
                t.insert_with_attributes(txn, 11, "Hi there", proposal("proposedInsertion", "p1"));
            })
            .await;
        phone
            .edit(|txn| {
                let t = text_of(txn);
                t.insert_with_attributes(txn, 6, "brave ", proposal("proposedInsertion", "p2"));
            })
            .await;
        assert_eq!(settle(&mut laptop).await, None, "the laptop stays connected");
        assert_eq!(settle(&mut phone).await, None, "the phone stays connected");

        let repaired = "<p><virtues-del proposal=\"p1\">hello </virtues-del><virtues-ins proposal=\"p2\">brave </virtues-ins>\
                        <virtues-del proposal=\"p1\">world</virtues-del><virtues-ins proposal=\"p1\">Hi there</virtues-ins></p>";
        let held = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        {
            let held = held.read().await;
            assert_eq!(virtues_document::validate_doc(&held.doc.transact()), []);
            assert_eq!(page_html(&held.doc), repaired);
        }
        assert_eq!(page_html(&laptop.doc), repaired);
        assert_eq!(page_html(&phone.doc), repaired);

        yjs.flush_pending_saves().await;
        let saved: Vec<u8> = sqlx::query_scalar("SELECT yjs_state FROM app_pages WHERE id = $1")
            .bind(&page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let saved = doc_from_state(&page_id, &saved);
        assert_eq!(virtues_document::validate_doc(&saved.transact()), []);
        assert_eq!(page_html(&saved), repaired);
    }

    /// Two devices each delete one of an item's two leading paragraphs, a
    /// list nested under them. Their merge leaves the item a list with no
    /// paragraph, which Tiptap's binding cannot draw and deletes, the nested
    /// list with it, if the merge reaches it before the server's repair: so
    /// every device hears the edit and its repair together, and never holds
    /// the merge.
    #[sqlx::test]
    async fn an_edit_and_its_repair_are_relayed_as_one(pool: PgPool) {
        use yrs::{XmlFragment, XmlOut};
        let page_id = tree_page(
            &pool,
            "<ul><li><p>p1</p><p>p2</p><ul><li><p>nested item the person wrote</p></li></ul></li></ul>",
        )
        .await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;
        let mut laptop = Client::connect(addr, &page_id, Some(version)).await;
        let mut phone = Client::connect(addr, &page_id, Some(version)).await;
        let mut reader = Client::connect(addr, &page_id, Some(version)).await;
        for c in [&mut laptop, &mut phone, &mut reader] {
            assert_eq!(c.hear().await, Heard::Synced);
        }
        let delete_paragraph = |at: u32| {
            move |txn: &mut yrs::TransactionMut| {
                let frag = txn.get_or_insert_xml_fragment("doc");
                let Some(XmlOut::Element(list)) = frag.get(txn, 0) else { panic!("a list") };
                let Some(XmlOut::Element(item)) = list.get(txn, 0) else { panic!("an item") };
                item.remove_range(txn, at, 1);
            }
        };
        laptop.edit(delete_paragraph(0)).await;
        phone.edit(delete_paragraph(1)).await;
        let mut heard = 0;
        while let Ok(update) = tokio::time::timeout(Duration::from_millis(300), reader.hear()).await {
            assert_eq!(update, Heard::Update);
            heard += 1;
            assert_eq!(
                virtues_document::validate_doc(&reader.doc.transact()),
                [],
                "after update {heard}: {}",
                page_html(&reader.doc)
            );
        }
        assert!(heard >= 2, "{heard}");
        assert!(page_html(&reader.doc).contains("nested item the person wrote"), "{}", page_html(&reader.doc));
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
            assert_eq!(take_client_update(&yjs, &page_id, &doc, &update).await, Ok(Taken::Applied));
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
            assert!(
                matches!(&refused, Refused::Update(reason) if reason.contains("JSON content")),
                "{refused:?}"
            );
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

                // The check of a tree page's update runs on the runtime's
                // blocking threads, each with a worker's stack too.
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .thread_stack_size(2 << 20)
                    .build()
                    .unwrap();
                let mut markdown = page_doc(doc_from_text("x"));
                assert!(apply_yjs_update(&mut markdown, &shallow).is_some());
                assert_eq!(runtime.block_on(admit_update(&markdown, &deep)), Ok(Taken::Applied));
                assert!(apply_yjs_update(&mut markdown, &deep).is_none());
                assert_eq!(read_content(&markdown.doc), "x");

                let o = virtues_document::parse_html("<p>x</p>", "doc");
                let tree = page_doc(virtues_document::doc_from_nodes(o.nodes));
                let refused = runtime.block_on(admit_update(&tree, &deep)).unwrap_err();
                assert!(refused.contains("nests more than"), "{refused}");
            })
            .unwrap()
            .join()
            .unwrap();
    }

    // ── tree pages: load, text paths, saves, block edits, carets ───────────

    fn trip_request(content: &str) -> pages::CreatePageRequest {
        pages::CreatePageRequest {
            title: "Trip".into(),
            content: content.into(),
            project_id: None,
            icon: None,
            icon_color: None,
            cover_url: None,
            tags: None,
            format: None,
        }
    }

    /// A tree page created from markdown, as the flag creates one.
    async fn tree_from_markdown(pool: &PgPool, markdown: &str) -> String {
        pages::create_page_as(pool, trip_request(markdown), PageFormat::Tree)
            .await
            .unwrap()
            .page
            .id
    }

    async fn saved_state(pool: &PgPool, page_id: &str) -> Option<Vec<u8>> {
        sqlx::query_scalar("SELECT yjs_state FROM app_pages WHERE id = $1")
            .bind(page_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn state_vector_of(doc: &Arc<RwLock<PageDoc>>) -> StateVector {
        doc.read().await.doc.transact().state_vector()
    }

    fn block_ids(tree: &[Node]) -> Vec<String> {
        tree.iter().map(|n| n.id().expect("a block id").to_string()).collect()
    }

    fn texts(tree: &[Node]) -> Vec<String> {
        tree.iter().map(Node::text_content).collect()
    }

    /// Insert `typed` at `offset` in the `block`th block's text, as an
    /// editor's keystrokes do.
    fn type_at(txn: &mut yrs::TransactionMut, block: u32, offset: u32, typed: &str) {
        use yrs::XmlOut;
        let frag = txn.get_or_insert_xml_fragment("doc");
        let Some(XmlOut::Element(p)) = frag.get(txn, block) else {
            panic!("a block")
        };
        let Some(XmlOut::Text(t)) = p.get(txn, 0) else {
            panic!("its text")
        };
        t.insert(txn, offset, typed);
    }

    /// A tree state that applies only in part, or not at all: cut off in the
    /// middle of an update.
    fn truncated(state: &[u8]) -> Vec<u8> {
        state[..state.len() / 2].to_vec()
    }

    #[sqlx::test]
    async fn a_tree_page_that_cannot_load_is_not_opened(pool: PgPool) {
        // The table refuses a tree row without its document; the load is the
        // backstop for a database that lost the constraint.
        sqlx::query("ALTER TABLE app_pages DROP CONSTRAINT app_pages_tree_has_document")
            .execute(&pool)
            .await
            .unwrap();
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;

        let no_state = unsaved_page(&pool, "The export.\n").await;
        sqlx::query("UPDATE app_pages SET format = 'tree' WHERE id = $1")
            .bind(&no_state)
            .execute(&pool)
            .await
            .unwrap();
        let err = yjs.doc_cache.get_or_create(&no_state, &pool).await.err().expect("refused");
        assert!(err.to_string().contains("not rebuilt from its markdown"), "{err}");
        assert_eq!(stored(&pool, &no_state).await, ("The export.\n".to_string(), false));

        // A tree with no contract stamp.
        let unstamped = {
            let o = virtues_document::parse_html("<p>Hello</p>", "doc");
            let doc = virtues_document::new_doc();
            {
                let mut txn = doc.transact_mut();
                let frag =
                    virtues_document::ydoc::fragment(virtues_document::contract(), &mut txn);
                virtues_document::write_nodes(&mut txn, &frag, 0, &o.nodes);
            }
            let state = doc.transact().encode_state_as_update_v1(&StateVector::default());
            state
        };
        let page = tree_page(&pool, "<p>Hello</p>").await;
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(&unstamped)
            .bind(&page)
            .execute(&pool)
            .await
            .unwrap();
        let err = yjs.doc_cache.get_or_create(&page, &pool).await.err().expect("refused");
        assert!(err.to_string().contains("no contract stamp"), "{err}");

        // A tree state cut off mid-update: not opened, so nothing is served
        // from it and nothing saved over it.
        let cut = tree_from_markdown(&pool, "## Plan\n\nLunch with Nick.\n\n- [ ] Book seats\n").await;
        let whole = saved_state(&pool, &cut).await.unwrap();
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(truncated(&whole))
            .bind(&cut)
            .execute(&pool)
            .await
            .unwrap();
        assert!(yjs.doc_cache.get_or_create(&cut, &pool).await.is_err());
        assert!(yjs.read_text(&cut).await.is_err());
        let mut client = Client::connect(addr, &cut, Some(virtues_document::contract().version)).await;
        assert!(matches!(client.hear().await, Heard::Closed(_)), "nothing served");
        run_the_save_loop(&yjs).await;
        assert_eq!(saved_state(&pool, &cut).await.unwrap(), truncated(&whole), "nothing saved");
    }

    /// A markdown row whose state carries a stamp (a page raised to a
    /// contract, its text still a Y.Text) loads as markdown.
    #[sqlx::test]
    async fn a_stamped_markdown_page_loads_as_markdown(pool: PgPool) {
        let page_id = saved_page(&pool, "Some notes.\n").await;
        let yjs = YjsState::new(pool.clone());
        yjs.raise_contract(&page_id).await.unwrap();

        let fresh = YjsState::new(pool.clone());
        let doc = fresh.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert_eq!(doc.read().await.format(), PageFormat::Markdown);
        assert_eq!(doc.read().await.contract(), virtues_document::contract().version);
        assert_eq!(fresh.read_text(&page_id).await.unwrap(), "Some notes.\n");
        let saved = saved_state(&pool, &page_id).await.unwrap();
        assert_eq!(
            page_text_of_state(&saved).unwrap(),
            PageText {
                markdown: "Some notes.\n".into(),
                tree: None
            }
        );
    }

    /// Server writes skip `admit_update`, so each Y.Text writer refuses a
    /// tree page itself, under the lock, before it writes anything. Readers
    /// get the export.
    #[sqlx::test]
    async fn text_writers_refuse_a_tree_page_and_readers_get_its_export(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "## Plan\n\nFirst line.\n").await;
        let yjs = YjsState::new(pool.clone());
        let doc = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        let export = "## Plan\n\nFirst line.\n";
        let before = state_vector_of(&doc).await;

        let refused = TextWriteError::Other(TREE_TEXT_WRITE.into());
        assert_eq!(
            yjs.apply_text_edit(&page_id, "First", "1st").await.unwrap_err(),
            refused
        );
        assert_eq!(yjs.apply_text_edit(&page_id, "", "All new.").await.unwrap_err(), refused);
        assert_eq!(
            yjs.apply_text_diff(&page_id, export, "## Plan\n\n1st line.\n").await.unwrap_err(),
            refused
        );
        assert_eq!(yjs.replace_text(&page_id, export, "Other.\n").await.unwrap_err(), refused);

        assert_eq!(state_vector_of(&doc).await, before, "nothing written");
        assert!(doc.read().await.doc.transact().get_text("content").is_none(), "no stray root");

        assert_eq!(yjs.read_text(&page_id).await.unwrap(), export);
        let written = yjs.text_and_state(&page_id).await.unwrap();
        assert_eq!(written.text, export);
        assert_eq!(texts(written.tree.as_deref().unwrap()), ["Plan", "First line."]);
        assert_eq!(extract_text_content(&written.state), export);
    }

    /// An editor's keystroke on a tree page saves the export as `content`.
    #[sqlx::test]
    async fn a_keystroke_on_a_tree_page_saves_its_export(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "Lunch with [@Nick](/person/person_1).\n").await;
        let yjs = YjsState::new(pool.clone());
        let doc = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();

        let client = new_doc();
        apply_v1(&client, &doc.read().await.written().state).unwrap();
        let sv = client.transact().state_vector();
        type_at(&mut client.transact_mut(), 0, 0, "Late ");
        let typed = client.transact().encode_diff_v1(&sv);
        take_client_update(&yjs, &page_id, &doc, &typed).await.unwrap();

        run_the_save_loop(&yjs).await;
        let (content, _) = stored(&pool, &page_id).await;
        assert_eq!(content, "Late Lunch with [@Nick](/person/person_1).\n");
        let saved = saved_state(&pool, &page_id).await.unwrap();
        assert_eq!(extract_text_content(&saved), content);
    }

    /// Opening a tree page sends an update that changes nothing, and nothing
    /// is saved for it: the row's `updated_at` stays where it was.
    #[sqlx::test]
    async fn opening_a_tree_page_saves_nothing(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "Lunch with [@Nick](/person/person_1).\n").await;
        let updated_at = |pool: PgPool, page_id: String| async move {
            sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>(
                "SELECT updated_at FROM app_pages WHERE id = $1",
            )
            .bind(page_id)
            .fetch_one(&pool)
            .await
            .unwrap()
        };
        let before = updated_at(pool.clone(), page_id.clone()).await;
        let yjs = YjsState::new(pool.clone());
        let doc = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();

        // An editor that already holds the page answers the server's state
        // with an empty diff.
        let client = new_doc();
        apply_v1(&client, &doc.read().await.written().state).unwrap();
        let sv = doc.read().await.doc.transact().state_vector();
        let nothing = client.transact().encode_diff_v1(&sv);
        take_client_update(&yjs, &page_id, &doc, &nothing).await.unwrap();

        run_the_save_loop(&yjs).await;
        assert_eq!(updated_at(pool.clone(), page_id.clone()).await, before);
        assert!(!doc.read().await.has_unsaved_changes());
    }

    /// A state of a doc whose page has since changed format is not saved
    /// over it, is not retried, and the doc gives way to the page's new one.
    /// Move a page to a tree holding `markdown`'s blocks, as a conversion
    /// does, behind any doc this process holds.
    async fn flip_to_tree(pool: &PgPool, page_id: &str, markdown: &str) -> Vec<u8> {
        let o = virtues_document::parse_markdown(markdown);
        let tree = virtues_document::doc_from_nodes(o.nodes);
        let state = tree.transact().encode_state_as_update_v1(&StateVector::default());
        sqlx::query("UPDATE app_pages SET format = 'tree', yjs_state = $1, content = $2 WHERE id = $3")
            .bind(&state)
            .bind(markdown)
            .bind(page_id)
            .execute(pool)
            .await
            .unwrap();
        state
    }

    /// What a block editor sends after its owner types `typed` at the start
    /// of the first paragraph.
    async fn typed_in_a_tree(doc: &Arc<RwLock<PageDoc>>, typed: &str) -> Vec<u8> {
        let client = virtues_document::new_doc();
        {
            let state = doc.read().await.written().state;
            let mut txn = client.transact_mut();
            txn.apply_update(Update::decode_v1(&state).unwrap()).unwrap();
        }
        let sv = client.transact().state_vector();
        type_into_first_paragraph(&mut client.transact_mut(), typed);
        let diff = client.transact().encode_state_as_update_v1(&sv);
        diff
    }

    /// When a page changes format, an editor still bound to its old doc is
    /// closed, since nothing it types there is saved, and what it typed on
    /// its way out does not push the new doc's change out of the save queue.
    #[sqlx::test]
    async fn a_page_that_changes_format_closes_its_old_editors(pool: PgPool) {
        let page_id = saved_page(&pool, "Your line.\n").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let mut old_editor = Client::connect(addr, &page_id, None).await;
        assert_eq!(old_editor.hear().await, Heard::Synced);
        let markdown = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();

        flip_to_tree(&pool, &page_id, "Your line, as blocks.\n").await;
        let tree = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(!Arc::ptr_eq(&markdown, &tree));
        assert_eq!(tree.read().await.format(), PageFormat::Tree);
        assert_eq!(old_editor.close().await, Heard::Closed(CLOSE_REFORMATTED));
        // A markdown editor opening the page now meets the tree, and is
        // refused as any editor below its contract is.
        let mut late = Client::connect(addr, &page_id, None).await;
        assert_eq!(late.hear().await, Heard::Closed(CLOSE_CONTRACT));

        // The new doc's change waits in the queue; the old doc takes nothing
        // more, and a state it queued on its way out does not replace it.
        let typed = typed_in_a_tree(&tree, "New. ").await;
        take_client_update(&yjs, &page_id, &tree, &typed).await.unwrap();
        let old_typing = typed_in_an_editor(&markdown, "Lost.\n").await;
        assert_eq!(
            take_client_update(&yjs, &page_id, &markdown, &old_typing).await,
            Err(Refused::Reformatted)
        );
        let (state, generation) = {
            let mut d = markdown.write().await;
            apply_v1(&d.doc, &old_typing).unwrap();
            let generation = d.record_change();
            (d.written().state, generation)
        };
        yjs.save_queue.queue_save(page_id.clone(), &markdown, state, generation).await;

        run_the_save_loop(&yjs).await;
        assert!(!tree.read().await.has_unsaved_changes(), "the tree's change was saved");
        assert_eq!(stored(&pool, &page_id).await.0, "New. Your line, as blocks.\n");
        assert!(yjs.save_queue.pending.read().await.is_empty());
    }

    /// A doc with a keystroke still unsaved is retired all the same once its
    /// page has moved to the other format: handed out, it would take every
    /// new editor and tool call onto the old format.
    #[sqlx::test]
    async fn an_unsaved_doc_is_not_handed_out_after_its_page_changes_format(pool: PgPool) {
        let page_id = saved_page(&pool, "Your line.\n").await;
        let yjs = YjsState::new(pool.clone());
        let markdown = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        let typed = typed_in_an_editor(&markdown, "More.\n").await;
        take_client_update(&yjs, &page_id, &markdown, &typed).await.unwrap();
        assert!(markdown.read().await.has_unsaved_changes());

        flip_to_tree(&pool, &page_id, "Your line, as blocks.\n").await;
        let now = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(!Arc::ptr_eq(&markdown, &now));
        assert_eq!(now.read().await.format(), PageFormat::Tree);
        assert_eq!(markdown.read().await.retired(), Retired::Reformatted);
        assert_eq!(yjs.read_text(&page_id).await.unwrap(), "Your line, as blocks.\n");
    }

    #[sqlx::test]
    async fn a_save_whose_format_changed_is_dropped_and_the_doc_retired(pool: PgPool) {
        let page_id = saved_page(&pool, "Your line.\n").await;
        let yjs = YjsState::new(pool.clone());
        let markdown = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        let typed = typed_in_an_editor(&markdown, "More.\n").await;
        take_client_update(&yjs, &page_id, &markdown, &typed).await.unwrap();

        // The page moves to a tree while that keystroke waits to be saved.
        let o = virtues_document::parse_markdown("Your line, as blocks.\n");
        let tree = virtues_document::doc_from_nodes(o.nodes);
        let tree_state = tree.transact().encode_state_as_update_v1(&StateVector::default());
        sqlx::query(
            "UPDATE app_pages SET format = 'tree', yjs_state = $1, \
             content = 'Your line, as blocks.\n' WHERE id = $2",
        )
        .bind(&tree_state)
        .bind(&page_id)
        .execute(&pool)
        .await
        .unwrap();

        run_the_save_loop(&yjs).await;
        assert_eq!(saved_state(&pool, &page_id).await.unwrap(), tree_state, "not written");
        assert_eq!(stored(&pool, &page_id).await.0, "Your line, as blocks.\n");
        assert!(yjs.save_queue.pending.read().await.is_empty(), "not requeued");
        assert!(!markdown.read().await.has_unsaved_changes());

        let now = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        assert!(!Arc::ptr_eq(&markdown, &now), "the markdown doc is retired");
        assert_eq!(now.read().await.format(), PageFormat::Tree);
        assert_eq!(yjs.read_text(&page_id).await.unwrap(), "Your line, as blocks.\n");

        // A machine edit through the retired doc's path reports the same.
        let err = yjs
            .save_queue
            .write(&pool, &page_id, &markdown, &typed, u64::MAX)
            .await
            .unwrap_err();
        assert!(matches!(err, SaveError::FormatChanged), "{err}");
    }

    #[sqlx::test]
    async fn a_block_edit_lands_reaches_editors_and_is_saved(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "## Plan\n\nLunch on Friday.\n\nOld line.\n").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let mut editor =
            Client::connect(addr, &page_id, Some(virtues_document::contract().version)).await;
        assert_eq!(editor.hear().await, Heard::Synced);

        let base = yjs.read_tree(&page_id).await.unwrap();
        let ids = block_ids(&base);
        let ops = [
            virtues_document::Op::Replace {
                id: ids[1].clone(),
                html: "<p>Lunch on Saturday.</p>".into(),
            },
            virtues_document::Op::Delete { id: ids[2].clone() },
            virtues_document::Op::Append {
                html: "<p>New line.</p>".into(),
            },
        ];
        let edit = yjs.edit_tree(&page_id, Some(&base), &ops).await.unwrap();
        assert_eq!(edit.before.text, "## Plan\n\nLunch on Friday.\n\nOld line.\n");
        let after = edit.after.unwrap();
        assert_eq!(after.text, "## Plan\n\nLunch on Saturday.\n\nNew line.\n");
        assert_eq!(edit.applied.written.len(), 2);
        assert_eq!(block_ids(after.tree.as_deref().unwrap())[..2], ids[..2], "ids kept");

        assert_eq!(editor.hear().await, Heard::Update);
        assert_eq!(
            texts(&virtues_document::read_doc(&editor.doc.transact())),
            ["Plan", "Lunch on Saturday.", "New line."]
        );
        assert_eq!(stored(&pool, &page_id).await.0, after.text, "saved at once");
        assert_eq!(saved_state(&pool, &page_id).await.unwrap(), after.state);
    }

    #[sqlx::test]
    async fn a_refused_block_edit_writes_and_relays_nothing(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "First.\n\nSecond.\n").await;
        let yjs = YjsState::new(pool.clone());
        let doc = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        let mut relayed = doc.read().await.broadcast_tx.subscribe();
        let before = state_vector_of(&doc).await;
        let base = yjs.read_tree(&page_id).await.unwrap();
        let ids = block_ids(&base);

        let ops = [
            virtues_document::Op::Replace {
                id: ids[0].clone(),
                html: "<p>First, edited.</p>".into(),
            },
            virtues_document::Op::Replace {
                id: "nosuchid".into(),
                html: "<p>Gone.</p>".into(),
            },
            virtues_document::Op::Append {
                html: "<div>Not in the contract.</div>".into(),
            },
        ];
        let Err(TreeError::Refused { problems, tree }) =
            yjs.edit_tree(&page_id, Some(&base), &ops).await
        else {
            panic!("refused");
        };
        assert!(problems.iter().any(|p| p.message.contains("no block has id `nosuchid`")), "{problems:?}");
        assert!(problems.iter().any(|p| p.at.starts_with("op 3")), "{problems:?}");
        assert_eq!(tree, base, "the page as it is");
        assert_eq!(state_vector_of(&doc).await, before, "nothing written");
        assert!(relayed.try_recv().is_err(), "nothing relayed");
        assert_eq!(yjs.read_text(&page_id).await.unwrap(), "First.\n\nSecond.\n");

        // A markdown page is not edited by block.
        let markdown = saved_page(&pool, "Notes.\n").await;
        assert!(matches!(
            yjs.edit_tree(&markdown, None, &ops[..1]).await,
            Err(TreeError::NotTree)
        ));
        assert!(matches!(yjs.read_tree(&markdown).await, Err(TreeError::NotTree)));
    }

    /// A page a newer server wrote (stamped above this one's contract) is
    /// not edited here: this server cannot check what it holds.
    #[sqlx::test]
    async fn a_page_stamped_above_the_servers_contract_is_not_edited(pool: PgPool) {
        let page_id = tree_page(&pool, "<p>Hello</p>").await;
        let newer = {
            let (doc, _) = doc_from_tree_state(&page_id, &saved_state(&pool, &page_id).await.unwrap()).unwrap();
            {
                let mut txn = doc.transact_mut();
                let meta = txn.get_or_insert_map("meta");
                yrs::Map::insert(
                    &meta,
                    &mut txn,
                    "contract",
                    yrs::Any::from(i64::from(virtues_document::contract().version) + 1),
                );
            }
            let state = doc.transact().encode_state_as_update_v1(&StateVector::default());
            state
        };
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(&newer)
            .bind(&page_id)
            .execute(&pool)
            .await
            .unwrap();
        let yjs = YjsState::new(pool.clone());
        let base = yjs.read_tree(&page_id).await.unwrap();
        let ops = [virtues_document::Op::Append {
            html: "<p>More.</p>".into(),
        }];
        let Err(TreeError::Refused { problems, .. }) = yjs.edit_tree(&page_id, Some(&base), &ops).await
        else {
            panic!("refused");
        };
        assert!(problems[0].message.contains("newer version of Virtues"), "{problems:?}");
        assert_eq!(saved_state(&pool, &page_id).await.unwrap(), newer);
    }

    /// A page a newer server wrote can hold nodes this server's contract does
    /// not know (after `virtues rollback`). Opening it here, as reading the
    /// page or binding a socket does, serves it as it is: putting it back
    /// would delete those nodes and save the deletion, which no later
    /// server could undo.
    #[sqlx::test]
    async fn a_page_stamped_above_the_servers_contract_is_opened_as_it_is(pool: PgPool) {
        use yrs::{Xml, XmlElementPrelim, XmlFragment, XmlTextPrelim};
        let page_id = tree_page(&pool, "<p>Keep me</p><p>Second</p>").await;
        let newer = {
            let (doc, _) = doc_from_tree_state(&page_id, &saved_state(&pool, &page_id).await.unwrap()).unwrap();
            {
                let mut txn = doc.transact_mut();
                let frag = txn.get_or_insert_xml_fragment("doc");
                let event = frag.insert(&mut txn, 1, XmlElementPrelim::empty("calendarEvent"));
                event.insert_attribute(&mut txn, "title", "Standup");
                event.push_back(&mut txn, XmlTextPrelim::new("Standup at 9"));
                let meta = txn.get_or_insert_map("meta");
                yrs::Map::insert(
                    &meta,
                    &mut txn,
                    "contract",
                    yrs::Any::from(i64::from(virtues_document::contract().version) + 1),
                );
            }
            let state = doc.transact().encode_state_as_update_v1(&StateVector::default());
            state
        };
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(&newer)
            .bind(&page_id)
            .execute(&pool)
            .await
            .unwrap();
        let yjs = YjsState::new(pool.clone());
        assert_eq!(
            yjs.page_contract(&page_id).await.unwrap(),
            virtues_document::contract().version + 1
        );
        assert_eq!(saved_state(&pool, &page_id).await.unwrap(), newer);
        let held = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        let kinds: Vec<String> = {
            let held = held.read().await;
            let txn = held.doc.transact();
            let frag = txn.get_xml_fragment("doc").unwrap();
            frag.children(&txn)
                .filter_map(|c| match c {
                    yrs::XmlOut::Element(e) => Some(e.tag().to_string()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(kinds, ["paragraph", "calendarEvent", "paragraph"]);
        let versions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM app_page_versions WHERE page_id = $1")
            .bind(&page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(versions, 0);
    }

    /// Appending markdown is a write like an edit: a page this server cannot
    /// check is not written to, and nothing is saved over it.
    #[sqlx::test]
    async fn markdown_is_not_appended_to_a_page_stamped_above_the_servers_contract(pool: PgPool) {
        let page_id = tree_page(&pool, "<p>Hello</p>").await;
        let newer = {
            let (doc, _) = doc_from_tree_state(&page_id, &saved_state(&pool, &page_id).await.unwrap()).unwrap();
            {
                let mut txn = doc.transact_mut();
                let meta = txn.get_or_insert_map("meta");
                yrs::Map::insert(
                    &meta,
                    &mut txn,
                    "contract",
                    yrs::Any::from(i64::from(virtues_document::contract().version) + 1),
                );
            }
            let state = doc.transact().encode_state_as_update_v1(&StateVector::default());
            state
        };
        sqlx::query("UPDATE app_pages SET yjs_state = $1 WHERE id = $2")
            .bind(&newer)
            .bind(&page_id)
            .execute(&pool)
            .await
            .unwrap();
        let content_before = stored(&pool, &page_id).await.0;
        let yjs = YjsState::new(pool.clone());
        let err = yjs.append_markdown(&page_id, "More.").await.unwrap_err();
        assert!(
            matches!(&err, crate::error::Error::InvalidInput(m) if m.contains("newer version of Virtues")),
            "{err}"
        );
        assert!(!yjs.read_text(&page_id).await.unwrap().contains("More."));
        run_the_save_loop(&yjs).await;
        assert_eq!(saved_state(&pool, &page_id).await.unwrap(), newer);
        assert_eq!(stored(&pool, &page_id).await.0, content_before);
    }

    /// A block edit whose save fails is on the page all the same.
    #[sqlx::test]
    async fn a_block_edit_that_could_not_be_saved_is_still_made(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "Coffee.\n").await;
        let yjs = YjsState::new(pool.clone());
        let base = yjs.read_tree(&page_id).await.unwrap();
        refuse_page_saves(&pool).await;
        let ops = [virtues_document::Op::Append {
            html: "<p>Tea.</p>".into(),
        }];
        let edit = yjs.edit_tree(&page_id, Some(&base), &ops).await.unwrap();
        let Err(TextWriteError::NotSaved { written, .. }) = edit.after else {
            panic!("expected NotSaved, got {:?}", edit.after);
        };
        assert_eq!(written.text, "Coffee.\n\nTea.\n");
        assert_eq!(yjs.read_text(&page_id).await.unwrap(), written.text);

        allow_page_saves(&pool).await;
        run_the_save_loop(&yjs).await;
        assert_eq!(stored(&pool, &page_id).await.0, written.text);
    }

    /// The owner types in a paragraph after the model read it. The model's
    /// edit to other words of it merges with theirs; its edit to the same
    /// words is refused, with the page as it is now.
    #[sqlx::test]
    async fn a_block_edit_merges_with_typing_in_other_words_and_refuses_the_same(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "Lunch on Friday.\n").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;
        let mut owner = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(owner.hear().await, Heard::Synced);

        let base = yjs.read_tree(&page_id).await.unwrap();
        let id = block_ids(&base)[0].clone();
        owner.edit(|txn| type_at(txn, 0, 0, "Long ")).await;
        assert_eq!(owner.hear().await, Heard::Update);
        assert_eq!(yjs.read_text(&page_id).await.unwrap(), "Long Lunch on Friday.\n");

        let edit = yjs
            .edit_tree(
                &page_id,
                Some(&base),
                &[virtues_document::Op::Replace {
                    id: id.clone(),
                    html: "<p>Lunch on Saturday.</p>".into(),
                }],
            )
            .await
            .unwrap();
        assert!(
            edit.applied.notes.iter().any(|n| n.message.contains("merged with a concurrent edit")),
            "{:?}",
            edit.applied.notes
        );
        assert_eq!(edit.after.unwrap().text, "Long Lunch on Saturday.\n");

        // Now both change the same word.
        let base = yjs.read_tree(&page_id).await.unwrap();
        assert_eq!(owner.hear().await, Heard::Update);
        owner
            .edit(|txn| type_at(txn, 0, "Long Lunch on ".len() as u32, "late "))
            .await;
        assert_eq!(owner.hear().await, Heard::Update);
        let Err(TreeError::Refused { problems, tree }) = yjs
            .edit_tree(
                &page_id,
                Some(&base),
                &[virtues_document::Op::Replace {
                    id,
                    html: "<p>Long Lunch on Sunday.</p>".into(),
                }],
            )
            .await
        else {
            panic!("refused");
        };
        assert!(problems[0].message.contains("editing the same words"), "{problems:?}");
        assert_eq!(texts(&tree), ["Long Lunch on late Saturday."]);
    }

    #[sqlx::test]
    async fn appending_markdown_to_a_tree_page_adds_blocks_at_the_end(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "## Plan\n\nFirst.\n").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let mut editor =
            Client::connect(addr, &page_id, Some(virtues_document::contract().version)).await;
        assert_eq!(editor.hear().await, Heard::Synced);

        let appended = yjs
            .append_markdown(&page_id, "---\nsource: pdf\n---\n\n> A highlight.\n\n- [ ] Read it\n")
            .await
            .unwrap();
        assert_eq!(appended.text, "## Plan\n\nFirst.\n\n> A highlight.\n\n- [ ] Read it\n");
        assert!(
            appended.notes.iter().any(|n| n.message.contains("front matter")),
            "{:?}",
            appended.notes
        );
        assert_eq!(editor.hear().await, Heard::Update);
        let tree = virtues_document::read_doc(&editor.doc.transact());
        assert_eq!(texts(&tree), ["Plan", "First.", "A highlight.", "Read it"]);
        assert!(tree.iter().all(|n| n.id().is_some()), "every new block has an id");

        run_the_save_loop(&yjs).await;
        assert_eq!(stored(&pool, &page_id).await.0, appended.text);

        // A markdown page appends as text, with no notes.
        let markdown = saved_page(&pool, "Note.").await;
        let appended = yjs.append_markdown(&markdown, "> q").await.unwrap();
        assert_eq!(appended.text, "Note.\n\n> q");
        assert!(appended.notes.is_empty());
    }

    /// Text sent into a page (a PDF's highlight) makes no suggestion, in
    /// the contract's tags written raw or in CriticMarkup: those are the
    /// page owner's to make, so the text comes in accepted.
    #[sqlx::test]
    async fn appended_markdown_makes_no_suggestion(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "First.\n").await;
        let yjs = YjsState::new(pool.clone());
        let appended = yjs
            .append_markdown(
                &page_id,
                "> Meet on <VIRTUES-DEL proposal=\"x\">Friday</VIRTUES-DEL>\
                 <virtues-ins proposal=\"x\">Saturday</virtues-ins>, {--early--}{++late++}.\n",
            )
            .await
            .unwrap();
        assert_eq!(appended.text, "First.\n\n> Meet on Saturday, late.\n");
        assert!(appended.notes.iter().any(|n| n.message.contains("as accepted")), "{:?}", appended.notes);
        let tree = yjs.read_tree(&page_id).await.unwrap();
        assert!(!crate::tools::holds_proposal(&tree));
    }

    /// A page turned into another tree block by block: a block the target
    /// shares keeps its Yjs items, so a keystroke an editor made in it before
    /// the write, and sends after, still lands. Every open editor receives
    /// the write, and it is saved at once.
    #[sqlx::test]
    async fn a_tree_is_replaced_block_by_block_and_typing_beside_it_survives(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "First.\n\nSecond.\n\nThird.\n").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let mut editor =
            Client::connect(addr, &page_id, Some(virtues_document::contract().version)).await;
        assert_eq!(editor.hear().await, Heard::Synced);

        let current = yjs.read_tree(&page_id).await.unwrap();
        let ids = block_ids(&current);
        // The first block reworded, the second as it is, the third gone.
        let mut target = current[..2].to_vec();
        target[0].content = vec![Node::text("First, put back.", vec![])];

        // The owner types in the second block; it reaches the server later.
        let sv = editor.doc.transact().state_vector();
        type_at(&mut editor.doc.transact_mut(), 1, 0, "Typed ");
        let typed = editor.doc.transact().encode_diff_v1(&sv);

        let edit = yjs
            .replace_tree(&page_id, &target)
            .await
            .unwrap()
            .expect("the page changed");
        assert_eq!(edit.before.text, "First.\n\nSecond.\n\nThird.\n");
        let written = edit.after.unwrap();
        assert_eq!(written.text, "First, put back.\n\nSecond.\n");
        assert_eq!(block_ids(written.tree.as_deref().unwrap()), ids[..2], "ids kept");
        assert_eq!(stored(&pool, &page_id).await.0, written.text, "saved at once");
        assert_eq!(editor.hear().await, Heard::Update);

        editor.send(encode_sync_update(&typed)).await;
        assert_eq!(editor.hear().await, Heard::Update);
        let merged = ["First, put back.", "Typed Second."];
        assert_eq!(texts(&yjs.read_tree(&page_id).await.unwrap()), merged);
        assert_eq!(texts(&virtues_document::read_doc(&editor.doc.transact())), merged);

        // A page that already reads as the target: nothing written or relayed.
        let doc = yjs.doc_cache.get_or_create(&page_id, &pool).await.unwrap();
        let now = yjs.read_tree(&page_id).await.unwrap();
        let before = state_vector_of(&doc).await;
        let mut relayed = doc.read().await.broadcast_tx.subscribe();
        assert!(yjs.replace_tree(&page_id, &now).await.unwrap().is_none());
        assert_eq!(state_vector_of(&doc).await, before);
        assert!(relayed.try_recv().is_err());

        // A markdown page is not replaced as a tree.
        let markdown = saved_page(&pool, "Notes.\n").await;
        assert!(matches!(
            yjs.replace_tree(&markdown, &now).await,
            Err(TextWriteError::Other(_))
        ));
        assert_eq!(yjs.read_text(&markdown).await.unwrap(), "Notes.\n");
    }

    /// Putting a version back writes the server's own tree, which no write's
    /// 4 MiB bounds: a long page cleared to one line is put back whole.
    #[sqlx::test]
    async fn a_long_page_cleared_is_put_back_whole(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "Cleared.\n").await;
        let yjs = YjsState::new(pool.clone());
        let paragraph = |i: usize| {
            Node::element(
                "paragraph",
                Default::default(),
                vec![Node::text(
                    &format!("Paragraph {i} of a long page, kept as it was written before the page was cleared, every word of it, none lost."),
                    vec![],
                )],
            )
        };
        let target: Vec<Node> = (0..40_000).map(paragraph).collect();
        assert!(virtues_document::to_html(&target, false).len() > virtues_document::MAX_INPUT_BYTES);
        let edit = yjs.replace_tree(&page_id, &target).await.unwrap().expect("the page changed");
        let written = edit.after.unwrap();
        assert_eq!(written.tree.as_deref().unwrap().len(), 40_000);
        assert!(stored(&pool, &page_id).await.0.ends_with("Paragraph 39999 of a long page, kept as it was written before the page was cleared, every word of it, none lost.\n"));
    }

    /// A page from another origin gets no socket: through the desktop's
    /// loopback splice it would otherwise read the page as the owner. The
    /// app's own pages, and clients that send no Origin, connect.
    #[sqlx::test]
    async fn a_page_from_another_origin_gets_no_socket(pool: PgPool) {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let page_id = tree_from_markdown(&pool, "Lunch.\n").await;
        let addr = serve(YjsState::new(pool.clone())).await;
        let handshake = |origin: &str| {
            let mut req = format!("ws://{addr}/ws/yjs/{page_id}?contract=1")
                .into_client_request()
                .unwrap();
            req.headers_mut().insert("origin", origin.parse().unwrap());
            req
        };
        for origin in ["https://evil.example", "http://localhost:8888"] {
            match tokio_tungstenite::connect_async(handshake(origin)).await {
                Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                    assert_eq!(response.status(), 403, "{origin}")
                }
                other => panic!("{origin} was let in: {:?}", other.map(|_| ())),
            }
        }
        let (socket, _) = tokio_tungstenite::connect_async(handshake("virtues://localhost"))
            .await
            .expect("the phone's own page connects");
        drop(socket);
        let mut editor =
            Client::connect(addr, &page_id, Some(virtues_document::contract().version)).await;
        assert_eq!(editor.hear().await, Heard::Synced, "no Origin, no browser");
    }

    /// A keystroke that reaches the page while a restore is still cutting
    /// its restore point is the owner's: the restore writes over its block,
    /// and a version of the owner's own keeps it, between the restore point
    /// and "Put back".
    #[sqlx::test]
    async fn typing_that_lands_during_a_restore_is_kept_as_a_version(pool: PgPool) {
        let page_id = tree_from_markdown(&pool, "Lunch.\n\nTea.\n").await;
        let yjs = YjsState::new(pool.clone());
        let v1 = pages::cut_version_now(&pool, &yjs, &page_id, "user", Some("Saved"))
            .await
            .unwrap();
        let then = yjs.read_tree(&page_id).await.unwrap();
        yjs.edit_tree(
            &page_id,
            Some(&then),
            &[virtues_document::Op::Replace {
                id: block_ids(&then)[1].clone(),
                html: "<p>Coffee.</p>".into(),
            }],
        )
        .await
        .unwrap();

        let addr = serve(yjs.clone()).await;
        let mut editor =
            Client::connect(addr, &page_id, Some(virtues_document::contract().version)).await;
        assert_eq!(editor.hear().await, Heard::Synced);

        // The restore point's INSERT waits on this lock while the owner types.
        let mut hold = pool.begin().await.unwrap();
        sqlx::query("LOCK TABLE app_page_versions IN SHARE MODE")
            .execute(&mut *hold)
            .await
            .unwrap();
        let restore = tokio::spawn({
            let (pool, yjs, page_id, version) =
                (pool.clone(), yjs.clone(), page_id.clone(), v1.id.clone());
            async move { pages::restore_version(&pool, &yjs, &page_id, &version).await }
        });
        pages::until_a_statement_waits(&pool, "%INSERT INTO app_page_versions%").await;
        editor.edit(|txn| type_at(txn, 0, 0, "Typed ")).await;
        assert_eq!(editor.hear().await, Heard::Update, "the server took the keystroke");
        assert_eq!(
            texts(&yjs.read_tree(&page_id).await.unwrap()),
            ["Typed Lunch.", "Coffee."]
        );
        hold.rollback().await.unwrap();
        let restored = restore.await.unwrap().expect("the restore");
        assert!(restored.changed);
        assert_eq!(texts(&yjs.read_tree(&page_id).await.unwrap()), ["Lunch.", "Tea."]);

        let rows: Vec<(String, Option<String>, Vec<u8>)> = sqlx::query_as(
            "SELECT created_by, description, yjs_snapshot FROM app_page_versions \
             WHERE page_id = $1 ORDER BY version_number",
        )
        .bind(&page_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        let history: Vec<(String, Option<String>, String)> = rows
            .into_iter()
            .map(|(by, why, state)| (by, why, page_text_of_state(&state).unwrap().markdown))
            .collect();
        let row = |by: &str, why: &str, text: &str| (by.to_string(), Some(why.to_string()), text.to_string());
        assert_eq!(
            history,
            [
                row("user", "Saved", "Lunch.\n\nTea.\n"),
                row("auto", pages::RESTORE_POINT, "Lunch.\n\nCoffee.\n"),
                row("auto", pages::TYPED_BEFORE_RESTORE, "Typed Lunch.\n\nCoffee.\n"),
                row("user", "Put back v1", "Lunch.\n\nTea.\n"),
            ]
        );
    }

    /// Every reader of a snapshot reads a tree's export, a History room
    /// diffing versions included.
    #[sqlx::test]
    async fn snapshot_readers_read_a_trees_export(pool: PgPool) {
        let o = virtues_document::parse_markdown("## Plan\n\nLunch with [@Nick](/person/person_1).\n");
        let tree = virtues_document::doc_from_nodes(o.nodes);
        let state = tree.transact().encode_state_as_update_v1(&StateVector::default());
        let export = "## Plan\n\nLunch with [@Nick](/person/person_1).\n";
        assert_eq!(extract_text_content(&state), export);
        assert_eq!(text_of_state(&state).unwrap(), export);
        let read = page_text_of_state(&state).unwrap();
        assert_eq!(read.tree.unwrap(), virtues_document::read_doc(&tree.transact()));

        sqlx::query("INSERT INTO wiki_people (id, name) VALUES ('person_1', 'Nick')")
            .execute(&pool)
            .await
            .unwrap();
        let article = crate::api::wiki_articles::create_article(&pool, "person", "person_1", "Nick", "## Plan\n")
            .await
            .unwrap();
        pages::create_version_from_snapshot(&pool, &article.page_id, &state, export, "ai", None)
            .await
            .unwrap();
        let history = crate::api::wiki_articles::get_article_history(&pool, "person", "person_1")
            .await
            .unwrap();
        assert_eq!(history.len(), 1, "{history:?}");
        assert!(
            history[0].diff.iter().any(|l| l.kind == "add" && l.text.contains("[@Nick](/person/person_1)")),
            "{:?}",
            history[0].diff
        );
    }

    #[test]
    fn presence_keeps_the_newest_state_of_each_client_within_its_limits() {
        let mut p = Presence::default();
        assert_eq!(p.apply(presence(&[(1, 1, "{\"a\":1}")])), [ClientID::new(1)]);
        // An older or equal clock does not replace it.
        assert!(p.apply(presence(&[(1, 1, "{\"a\":2}")])).is_empty());
        assert_eq!(&*p.states[&ClientID::new(1)].1, "{\"a\":1}");
        assert_eq!(p.apply(presence(&[(1, 2, "{\"a\":2}")])), [ClientID::new(1)]);

        // A state past 4 KiB is not kept.
        let big = format!("{{\"a\":\"{}\"}}", "x".repeat(5 * 1024));
        assert!(p.apply(presence(&[(2, 1, &big)])).is_empty());
        assert!(!p.states.contains_key(&ClientID::new(2)));

        // Past 64 clients a new one is not kept; one already kept still is.
        for client in 10..(10 + 63) {
            p.apply(presence(&[(client, 1, "{}")]));
        }
        assert_eq!(p.states.len(), 64);
        assert!(p.apply(presence(&[(500, 1, "{}")])).is_empty(), "the 65th client");
        assert_eq!(p.apply(presence(&[(1, 3, "{\"a\":3}")])), [ClientID::new(1)]);

        // A null state at the kept clock removes it; taking down sends null
        // one clock on.
        assert!(p.apply(presence(&[(10, 1, "null")])).is_empty());
        assert!(!p.states.contains_key(&ClientID::new(10)));
        let gone = p.remove(&[ClientID::new(1), ClientID::new(999)].into_iter().collect()).unwrap();
        assert_eq!(gone.clients.len(), 1);
        let entry = &gone.clients[&ClientID::new(1)];
        assert_eq!((entry.clock, &*entry.json), (4, "null"));
        assert!(p.remove(&[ClientID::new(1)].into_iter().collect()).is_none());
    }

    /// A client that joins late is sent the carets already on the page at
    /// once, and a closed socket's caret is taken down for everyone left.
    #[sqlx::test]
    async fn carets_reach_late_joiners_and_leave_with_their_socket(pool: PgPool) {
        let page_id = tree_page(&pool, "<p>Hello</p>").await;
        let yjs = YjsState::new(pool.clone());
        let addr = serve(yjs.clone()).await;
        let version = virtues_document::contract().version;

        let mut laptop = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(laptop.hear().await, Heard::Synced);
        laptop.announce(11, 3, r#"{"user":{"name":"Computer"}}"#).await;
        // Relayed to every socket on the page, its sender's included: once
        // heard, the server has kept it.
        assert!(laptop.hear_awareness().await.is_some());

        let mut phone = Client::connect(addr, &page_id, Some(version)).await;
        let present = phone.hear_awareness().await.expect("the carets already here");
        let laptops = &present.clients[&ClientID::new(11)];
        assert_eq!((laptops.clock, &*laptops.json), (3, r#"{"user":{"name":"Computer"}}"#));
        assert_eq!(phone.hear().await, Heard::Synced);

        laptop.socket.close(None).await.unwrap();
        let gone = phone.hear_awareness().await.expect("the closed tab's caret taken down");
        let laptops = &gone.clients[&ClientID::new(11)];
        assert_eq!((laptops.clock, &*laptops.json), (4, "null"));

        // Nobody is left to show a third client.
        let mut tablet = Client::connect(addr, &page_id, Some(version)).await;
        assert_eq!(tablet.hear().await, Heard::Synced);
        assert!(yjs
            .doc_cache
            .get_or_create(&page_id, &pool)
            .await
            .unwrap()
            .read()
            .await
            .awareness()
            .snapshot()
            .is_none());
    }
}
