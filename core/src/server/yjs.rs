//! Yjs WebSocket sync server using yrs crate
//!
//! Implements the y-websocket protocol for compatibility with the y-websocket client library.
//! Uses Y.XmlFragment for ProseMirror compatibility (via y-prosemirror).
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
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, RwLock};
use tokio::time::Instant;
use yrs::{updates::decoder::Decode, updates::encoder::Encode, Doc, GetString, ReadTxn, StateVector, Transact, Update, WriteTxn, Xml, XmlFragment};

use crate::api::pages;
use crate::markdown::{markdown_to_xml_fragment, xml_fragment_to_markdown};

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
}

/// Document cache with automatic TTL eviction
pub struct DocCache {
    pages: Cache<String, Arc<RwLock<PageDoc>>>,
}

impl DocCache {
    pub fn new() -> Self {
        Self {
            pages: Cache::builder()
                .time_to_idle(Duration::from_secs(30 * 60)) // 30 min TTL
                .max_capacity(100)
                .build(),
        }
    }

    /// Get or load document from database
    pub async fn get_or_create(
        &self,
        page_id: &str,
        pool: &SqlitePool,
    ) -> Result<Arc<RwLock<PageDoc>>, anyhow::Error> {
        // Check cache first
        if let Some(doc) = self.pages.get(page_id) {
            return Ok(doc);
        }

        // Load from database
        let page = pages::get_page(pool, page_id).await?;

        let doc = Doc::new();

        // Check if we have Yjs state stored
        let yjs_state: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT yjs_state FROM pages WHERE id = ?"
        )
        .bind(page_id)
        .fetch_optional(pool)
        .await?
        .flatten();

        if let Some(state) = yjs_state {
            // Apply existing Yjs state
            if let Ok(update) = Update::decode_v1(&state) {
                let mut txn = doc.transact_mut();
                txn.apply_update(update);
            }
        }
        // Note: For pages without Yjs state, we do NOT pre-populate the XmlFragment here.
        // y-prosemirror requires the XML structure to be created by ProseMirror, not directly.
        // The frontend will initialize content through ProseMirror's markdown parser.

        let (broadcast_tx, _) = broadcast::channel(256);
        let page_doc = Arc::new(RwLock::new(PageDoc {
            doc,
            broadcast_tx,
            last_update: Instant::now(),
        }));

        // Insert into cache
        self.pages.insert(page_id.to_string(), page_doc.clone());

        Ok(page_doc)
    }

    /// Remove a document from cache (e.g., when page is deleted)
    pub fn remove(&self, page_id: &str) {
        self.pages.invalidate(page_id);
    }
}

impl Default for DocCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Pending save entry for debounced persistence
struct PendingSave {
    yjs_state: Vec<u8>,
    queued_at: Instant,
}

/// Debounced save queue - waits for typing to stop before saving
pub struct SaveQueue {
    pending: RwLock<std::collections::HashMap<String, PendingSave>>,
}

impl SaveQueue {
    pub fn new() -> Self {
        Self {
            pending: RwLock::new(std::collections::HashMap::new()),
        }
    }

    pub async fn queue_save(&self, page_id: String, yjs_state: Vec<u8>) {
        let mut pending = self.pending.write().await;
        pending.insert(
            page_id,
            PendingSave {
                yjs_state,
                queued_at: Instant::now(),
            },
        );
    }

    /// Background task: process saves after 2s of inactivity
    pub async fn process_loop(self: Arc<Self>, pool: SqlitePool) {
        const DEBOUNCE_DURATION: Duration = Duration::from_secs(2);

        loop {
            tokio::time::sleep(Duration::from_millis(500)).await;

            let now = Instant::now();
            let mut to_save = vec![];

            {
                let mut pending = self.pending.write().await;
                pending.retain(|page_id, save| {
                    if now.duration_since(save.queued_at) >= DEBOUNCE_DURATION {
                        to_save.push((page_id.clone(), save.yjs_state.clone()));
                        false
                    } else {
                        true
                    }
                });
            }

            for (page_id, yjs_state) in to_save {
                if let Err(e) = save_and_materialize(&pool, &page_id, &yjs_state).await {
                    tracing::error!("Failed to save page {}: {}", page_id, e);
                }
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
fn apply_yjs_update(doc: &mut PageDoc, data: &[u8]) -> Option<Vec<u8>> {
    if let Ok(update) = Update::decode_v1(data) {
        {
            let mut txn = doc.doc.transact_mut();
            txn.apply_update(update);
        }
        doc.last_update = Instant::now();

        // Broadcast to other clients (wrapped as y-websocket update message)
        let broadcast_msg = encode_sync_update(data);
        let _ = doc.broadcast_tx.send(broadcast_msg);

        // Get current state for debounced save
        let txn = doc.doc.transact();
        Some(txn.encode_state_as_update_v1(&yrs::StateVector::default()))
    } else {
        None
    }
}

// ============================================================================
// lib0 VarInt Encoding (used by y-websocket protocol)
// ============================================================================

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

/// Extract text content from Yjs state bytes (XmlFragment)
/// Uses the built-in GetString trait which recursively extracts text
fn extract_text_content(yjs_state: &[u8]) -> String {
    let doc = Doc::new();
    if let Ok(update) = Update::decode_v1(yjs_state) {
        let mut txn = doc.transact_mut();
        txn.apply_update(update);
    }

    let txn = doc.transact();
    if let Some(fragment) = txn.get_xml_fragment("content") {
        // GetString::get_string recursively extracts all text content
        fragment.get_string(&txn)
    } else {
        String::new()
    }
}

/// Save Yjs state and materialize content for search
async fn save_and_materialize(
    pool: &SqlitePool,
    page_id: &str,
    yjs_state: &[u8],
) -> Result<(), anyhow::Error> {
    // Extract text content from yrs
    let content = extract_text_content(yjs_state);

    // Save both yjs_state and materialized content
    sqlx::query(
        "UPDATE pages SET yjs_state = ?, content = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(yjs_state)
    .bind(&content)
    .bind(page_id)
    .execute(pool)
    .await?;

    // Future: trigger embedding update
    // This is a placeholder - embeddings not implemented yet
    on_content_updated(page_id, &content);

    tracing::debug!("Saved page {} ({} chars)", page_id, content.len());
    Ok(())
}

/// Placeholder for future embedding integration
fn on_content_updated(page_id: &str, content: &str) {
    // TODO: When embeddings are ready:
    // - Queue embedding job with longer debounce (30-60s)
    // - embedding_queue.send(EmbeddingJob { page_id, content, delay: 30s });
    let _ = (page_id, content); // Silence unused warnings
}

/// Shared state for Yjs WebSocket connections
#[derive(Clone)]
pub struct YjsState {
    pub doc_cache: Arc<DocCache>,
    pub save_queue: Arc<SaveQueue>,
    pub pool: SqlitePool,
}

impl YjsState {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            doc_cache: Arc::new(DocCache::new()),
            save_queue: Arc::new(SaveQueue::new()),
            pool,
        }
    }

    /// Start the background save queue processor
    pub fn start_save_processor(&self) {
        let save_queue = self.save_queue.clone();
        let pool = self.pool.clone();
        tokio::spawn(async move {
            save_queue.process_loop(pool).await;
        });
    }

    /// Apply a markdown text edit to a page through Yjs (XmlFragment)
    ///
    /// This method:
    /// 1. Gets or creates the Yjs document for the page
    /// 2. Parses the markdown content into ProseMirror-compatible XmlFragment structure
    /// 3. Broadcasts the update to connected clients
    /// 4. Queues the save for persistence
    ///
    /// For find/replace edits (non-empty `find`):
    /// - Gets current content as markdown
    /// - Performs the replacement in markdown space
    /// - Parses the result back to XmlFragment
    ///
    /// For full document replacement (empty `find`):
    /// - Clears the document and parses the new markdown content
    ///
    /// Returns the new markdown content after the edit, or an error message
    pub async fn apply_text_edit(
        &self,
        page_id: &str,
        find: &str,
        replace: &str,
    ) -> Result<String, String> {
        // Get or create the document
        let page_doc = self.doc_cache.get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| format!("Failed to get page document: {}", e))?;

        let (new_markdown, update_bytes) = {
            let mut doc = page_doc.write().await;

            // Determine new content
            let new_markdown = if find.is_empty() {
                // Full document replacement
                replace.to_string()
            } else {
                // Find/replace: get current markdown, perform replacement
                let txn = doc.doc.transact();
                let current_markdown = if let Some(frag) = txn.get_xml_fragment("content") {
                    xml_fragment_to_markdown(&txn, frag)
                } else {
                    String::new()
                };
                drop(txn);

                // Check that the find text exists
                if !current_markdown.contains(find) {
                    return Err(format!(
                        "Text not found in page: '{}'",
                        if find.len() > 50 {
                            format!("{}...", &find[..50])
                        } else {
                            find.to_string()
                        }
                    ));
                }

                // Perform the replacement
                current_markdown.replacen(find, replace, 1)
            };

            // Parse markdown into XmlFragment
            {
                let mut txn = doc.doc.transact_mut();
                let fragment = txn.get_or_insert_xml_fragment("content");

                // Remove all existing children
                while fragment.len(&txn) > 0 {
                    fragment.remove_range(&mut txn, 0, 1);
                }

                // Parse markdown into XmlFragment
                if let Err(e) = markdown_to_xml_fragment(&mut txn, fragment, &new_markdown) {
                    return Err(format!("Failed to parse markdown: {}", e));
                }
            }

            doc.last_update = Instant::now();

            // Encode update for broadcast and persistence
            let txn = doc.doc.transact();
            let update_bytes = txn.encode_state_as_update_v1(&StateVector::default());

            // Broadcast to connected clients
            let broadcast_msg = encode_sync_update(&update_bytes);
            let _ = doc.broadcast_tx.send(broadcast_msg);

            (new_markdown, update_bytes)
        };

        // Queue save for persistence
        self.save_queue.queue_save(page_id.to_string(), update_bytes).await;

        Ok(new_markdown)
    }

    /// Get the current content of a page from Yjs as markdown
    ///
    /// This reads from the Yjs document if it's loaded and converts XmlFragment to markdown.
    /// Falls back to database content if document is not loaded.
    pub async fn get_page_content(&self, page_id: &str) -> Result<String, String> {
        // Get or create the document
        let page_doc = self.doc_cache.get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| format!("Failed to get page document: {}", e))?;

        let doc = page_doc.read().await;
        let txn = doc.doc.transact();

        if let Some(fragment) = txn.get_xml_fragment("content") {
            // Convert XmlFragment to markdown for AI consumption
            Ok(xml_fragment_to_markdown(&txn, fragment))
        } else {
            Ok(String::new())
        }
    }
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
                                        let mut doc = page_doc.write().await;
                                        if let Some(full_state) = apply_yjs_update(&mut doc, update_bytes) {
                                            drop(doc);
                                            state.save_queue.queue_save(page_id.clone(), full_state).await;
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
                                        let mut doc = page_doc.write().await;
                                        if let Some(full_state) = apply_yjs_update(&mut doc, update_bytes) {
                                            drop(doc);
                                            state.save_queue.queue_save(page_id.clone(), full_state).await;
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
            else => break,
        }
    }

    tracing::debug!("WebSocket connection closed for page {}", page_id);
}
