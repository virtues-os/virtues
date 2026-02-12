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
use yrs::{updates::decoder::Decode, updates::encoder::Encode, Any, Doc, GetString, ReadTxn, StateVector, Text, Transact, TransactionMut, Update, Value, WriteTxn, XmlElementRef, XmlFragment, XmlFragmentRef, XmlNode};
use yrs::types::Attrs;
use yrs::types::text::YChange;

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
        let _page = pages::get_page(pool, page_id).await?;

        let doc = Doc::new();

        // Check if we have Yjs state stored
        let yjs_state: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT yjs_state FROM app_pages WHERE id = ?"
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
        } else if !_page.content.is_empty() {
            // No Yjs state yet but page has markdown content — initialize XmlFragment
            // server-side so the frontend doesn't need a TS markdown parser.
            let mut txn = doc.transact_mut();
            let fragment = txn.get_or_insert_xml_fragment("content");
            if let Err(e) = markdown_to_xml_fragment(&mut txn, fragment, &_page.content) {
                tracing::warn!("Failed to init Yjs from markdown for page {}: {}", page_id, e);
            }
        }

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
        "UPDATE app_pages SET yjs_state = ?, content = ?, updated_at = datetime('now') WHERE id = ?",
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

// =============================================================================
// Surgical Text Editing
// =============================================================================

/// Result of attempting a surgical text edit on the Yjs tree
enum SurgicalEditResult {
    /// Edit applied successfully in-place
    Applied,
    /// Text not found in the document tree
    NotFound,
}

/// Attempt surgical text replacement within a Yjs XmlFragment.
///
/// Walks the tree recursively, looking for the `find` text within individual
/// XmlText nodes. If found within a single XmlText, replaces it in-place while
/// preserving all surrounding structure (entity_links, file_cards, etc.) and
/// maintaining text formatting (bold, italic, etc.) on the replacement text.
///
/// Falls back (NeedsFallback) if the text can't be edited at the tree level —
/// e.g. it spans inline elements or the find text contains markdown syntax that
/// doesn't appear as literal text in XmlText nodes.
fn surgical_text_edit(
    txn: &mut TransactionMut,
    fragment: &XmlFragmentRef,
    find: &str,
    replace: &str,
) -> SurgicalEditResult {
    let n = fragment.len(txn);
    for i in 0..n {
        if let Some(XmlNode::Element(el)) = fragment.get(txn, i) {
            let result = search_in_element(txn, &el, find, replace);
            match result {
                SurgicalEditResult::NotFound => continue,
                other => return other,
            }
        }
    }
    SurgicalEditResult::NotFound
}

/// Recursively search for text within an element and apply replacement if found.
fn search_in_element(
    txn: &mut TransactionMut,
    element: &XmlElementRef,
    find: &str,
    replace: &str,
) -> SurgicalEditResult {
    let tag = element.tag();
    let tag = tag.as_ref();

    match tag {
        // Inline containers: search XmlText children directly
        "paragraph" | "heading" => {
            search_in_inline_container(txn, element, find, replace)
        }
        // Code blocks: text without formatting
        "code_block" => {
            search_in_code_block(txn, element, find, replace)
        }
        // Table cells might have direct inline content OR child paragraphs
        "table_cell" | "table_header" => {
            let result = search_in_inline_container(txn, element, find, replace);
            if !matches!(result, SurgicalEditResult::NotFound) {
                return result;
            }
            recurse_into_children(txn, element, find, replace)
        }
        // Structural containers: recurse into children
        "bullet_list" | "ordered_list" | "list_item" | "blockquote"
        | "table" | "table_row" => {
            recurse_into_children(txn, element, find, replace)
        }
        // Non-text nodes (media, horizontal_rule, entity_link, file_card, etc.)
        _ => SurgicalEditResult::NotFound,
    }
}

/// Recurse into an element's children looking for the text.
fn recurse_into_children(
    txn: &mut TransactionMut,
    element: &XmlElementRef,
    find: &str,
    replace: &str,
) -> SurgicalEditResult {
    let n = element.len(txn);
    for i in 0..n {
        if let Some(XmlNode::Element(child)) = element.get(txn, i) {
            let result = search_in_element(txn, &child, find, replace);
            match result {
                SurgicalEditResult::NotFound => continue,
                other => return other,
            }
        }
    }
    SurgicalEditResult::NotFound
}

/// Search for text within an inline container (paragraph, heading).
///
/// These elements have XmlText and XmlElement (entity_link, file_card, hard_break)
/// children. We search each XmlText independently — if the find text spans across
/// an inline element boundary, it won't be found here and the caller falls back.
fn search_in_inline_container(
    txn: &mut TransactionMut,
    element: &XmlElementRef,
    find: &str,
    replace: &str,
) -> SurgicalEditResult {
    let n = element.len(txn);
    for i in 0..n {
        if let Some(XmlNode::Text(text_ref)) = element.get(txn, i) {
            // Build plain text from diff() chunks. IMPORTANT: get_string() on XmlTextRef
            // returns XML-formatted text (e.g. "<strong>bold</strong>") which has different
            // byte offsets than the actual text content. diff() gives us the real text.
            let mut plain_text = String::new();
            let mut chunk_info: Vec<(u32, u32, Option<Attrs>)> = Vec::new();

            for diff in text_ref.diff(txn, YChange::identity) {
                let chunk_str = match &diff.insert {
                    Value::Any(Any::String(s)) => s.to_string(),
                    _ => continue,
                };
                let char_start = plain_text.chars().count() as u32;
                let char_len = chunk_str.chars().count() as u32;
                plain_text.push_str(&chunk_str);
                chunk_info.push((char_start, char_len, diff.attributes.map(|a| *a)));
            }

            if let Some(byte_offset) = plain_text.find(find) {
                let char_start = plain_text[..byte_offset].chars().count() as u32;
                let char_len = find.chars().count() as u32;

                // Find formatting at the match start position
                let attrs = chunk_info.iter()
                    .find(|(start, len, _)| char_start >= *start && char_start < *start + *len)
                    .and_then(|(_, _, attrs)| attrs.clone());

                // Edit using Text trait methods (not inherent XmlTextRef methods)
                Text::remove_range(&text_ref, txn, char_start, char_len);
                Text::insert(&text_ref, txn, char_start, replace);

                // Re-apply formatting to the replacement text
                let replace_char_len = replace.chars().count() as u32;
                if let Some(attrs) = attrs {
                    if !attrs.is_empty() && replace_char_len > 0 {
                        Text::format(&text_ref, txn, char_start, replace_char_len, attrs);
                    }
                }

                return SurgicalEditResult::Applied;
            }
        }
    }
    SurgicalEditResult::NotFound
}

/// Search for text within a code block's text content.
/// Code blocks have no formatting, so we just do a plain find/replace.
fn search_in_code_block(
    txn: &mut TransactionMut,
    element: &XmlElementRef,
    find: &str,
    replace: &str,
) -> SurgicalEditResult {
    let n = element.len(txn);
    for i in 0..n {
        if let Some(XmlNode::Text(text_ref)) = element.get(txn, i) {
            // Build plain text from diff() for consistency (code blocks have no formatting
            // so get_string would also work, but diff is safer)
            let mut plain_text = String::new();
            for diff in text_ref.diff(txn, YChange::identity) {
                if let Value::Any(Any::String(s)) = &diff.insert {
                    plain_text.push_str(s);
                }
            }

            if let Some(byte_offset) = plain_text.find(find) {
                let char_start = plain_text[..byte_offset].chars().count() as u32;
                let char_len = find.chars().count() as u32;

                Text::remove_range(&text_ref, txn, char_start, char_len);
                Text::insert(&text_ref, txn, char_start, replace);

                return SurgicalEditResult::Applied;
            }
        }
    }
    SurgicalEditResult::NotFound
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
    /// For find/replace edits (non-empty `find`):
    /// 1. **Surgical path** (preferred): searches the Yjs tree for the find text and
    ///    replaces it in-place, preserving entity_links, file_cards, and formatting.
    /// 2. **Fallback path**: if surgical edit can't match (e.g. find text contains
    ///    markdown syntax), falls back to full markdown roundtrip.
    ///
    /// For full document replacement (empty `find`):
    /// - Clears the document and parses the new markdown content.
    ///
    /// Returns the new markdown content after the edit, or an error message.
    /// Get the current Yjs document state as bytes (for snapshots/versioning).
    pub async fn get_document_snapshot(&self, page_id: &str) -> Result<Vec<u8>, String> {
        let page_doc = self.doc_cache.get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| format!("Failed to get page document: {}", e))?;
        let doc = page_doc.read().await;
        let txn = doc.doc.transact();
        Ok(txn.encode_state_as_update_v1(&StateVector::default()))
    }

    /// - `find`: plain text to locate (empty = full document replacement)
    /// - `replace`: plain text for surgical in-place edit (text within existing nodes)
    /// - `markdown_replace`: markdown for full replacement and fallback roundtrip
    pub async fn apply_text_edit(
        &self,
        page_id: &str,
        find: &str,
        replace: &str,
        markdown_replace: &str,
    ) -> Result<String, String> {
        let page_doc = self.doc_cache.get_or_create(page_id, &self.pool)
            .await
            .map_err(|e| format!("Failed to get page document: {}", e))?;

        let (new_markdown, update_bytes) = {
            let mut doc = page_doc.write().await;

            if find.is_empty() {
                // Full document replacement — clear and rebuild with markdown formatting
                let mut txn = doc.doc.transact_mut();
                let fragment = txn.get_or_insert_xml_fragment("content");

                while fragment.len(&txn) > 0 {
                    fragment.remove_range(&mut txn, 0, 1);
                }

                if let Err(e) = markdown_to_xml_fragment(&mut txn, fragment, markdown_replace) {
                    return Err(format!("Failed to parse markdown: {}", e));
                }
                drop(txn);
            } else {
                // Find/replace: try surgical edit first, fall back to markdown roundtrip
                let surgical_applied = {
                    let mut txn = doc.doc.transact_mut();
                    let fragment = txn.get_or_insert_xml_fragment("content");
                    matches!(
                        surgical_text_edit(&mut txn, &fragment, find, replace),
                        SurgicalEditResult::Applied
                    )
                    // txn commits on drop
                };

                if !surgical_applied {
                    // Fallback: markdown roundtrip (preserves formatting in replacement)
                    let txn = doc.doc.transact();
                    let current_markdown = if let Some(frag) = txn.get_xml_fragment("content") {
                        xml_fragment_to_markdown(&txn, frag)
                    } else {
                        String::new()
                    };
                    drop(txn);

                    if !current_markdown.contains(find) {
                        return Err(format!(
                            "Text not found in page: '{}'",
                            if find.chars().count() > 50 {
                                format!("{}...", find.chars().take(50).collect::<String>())
                            } else {
                                find.to_string()
                            }
                        ));
                    }

                    let new_markdown = current_markdown.replacen(find, markdown_replace, 1);

                    let mut txn = doc.doc.transact_mut();
                    let fragment = txn.get_or_insert_xml_fragment("content");

                    while fragment.len(&txn) > 0 {
                        fragment.remove_range(&mut txn, 0, 1);
                    }

                    if let Err(e) = markdown_to_xml_fragment(&mut txn, fragment, &new_markdown) {
                        return Err(format!("Failed to parse markdown: {}", e));
                    }
                    // txn commits on drop
                }
            }

            doc.last_update = Instant::now();

            // Serialize final state and encode for broadcast + persistence
            let txn = doc.doc.transact();
            let new_markdown = if let Some(frag) = txn.get_xml_fragment("content") {
                xml_fragment_to_markdown(&txn, frag)
            } else {
                String::new()
            };
            let update_bytes = txn.encode_state_as_update_v1(&StateVector::default());

            let broadcast_msg = encode_sync_update(&update_bytes);
            let _ = doc.broadcast_tx.send(broadcast_msg);

            (new_markdown, update_bytes)
        };

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

#[cfg(test)]
mod tests {
    use super::*;
    use yrs::{Doc, Transact, WriteTxn};
    use crate::markdown::{markdown_to_xml_fragment, xml_fragment_to_markdown};

    /// Helper: create a doc with content and return (doc, fragment name)
    fn setup_doc(markdown: &str) -> Doc {
        let doc = Doc::new();
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");
        markdown_to_xml_fragment(&mut txn, fragment, markdown).unwrap();
        drop(txn);
        doc
    }

    #[test]
    fn test_surgical_edit_simple_text() {
        let doc = setup_doc("Hello world, this is a test.");
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");

        let result = surgical_text_edit(&mut txn, &fragment, "world", "universe");
        assert!(matches!(result, SurgicalEditResult::Applied));
        drop(txn);

        let txn = doc.transact();
        let frag = txn.get_xml_fragment("content").unwrap();
        let output = xml_fragment_to_markdown(&txn, frag);
        assert!(output.contains("Hello universe, this is a test."), "Got: {}", output);
    }

    #[test]
    fn test_surgical_edit_preserves_entity_link() {
        let doc = setup_doc("Hello [John](/person/123) how are you?");
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");

        // Edit text near entity link — link should survive
        let result = surgical_text_edit(&mut txn, &fragment, "how are you?", "what's up?");
        assert!(matches!(result, SurgicalEditResult::Applied));
        drop(txn);

        let txn = doc.transact();
        let frag = txn.get_xml_fragment("content").unwrap();
        let output = xml_fragment_to_markdown(&txn, frag);
        assert!(output.contains("[John](/person/123)"), "Entity link should survive. Got: {}", output);
        assert!(output.contains("what's up?"), "Replacement should be present. Got: {}", output);
    }

    #[test]
    fn test_surgical_edit_preserves_formatting() {
        let doc = setup_doc("This is **bold text** here.");
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");

        // Edit within the bold range — bold should be preserved
        let result = surgical_text_edit(&mut txn, &fragment, "bold text", "strong words");
        assert!(matches!(result, SurgicalEditResult::Applied));
        drop(txn);

        let txn = doc.transact();
        let frag = txn.get_xml_fragment("content").unwrap();
        let output = xml_fragment_to_markdown(&txn, frag);
        assert!(output.contains("**strong words**"), "Bold should be preserved. Got: {}", output);
    }

    #[test]
    fn test_surgical_edit_not_found() {
        let doc = setup_doc("Hello world.");
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");

        let result = surgical_text_edit(&mut txn, &fragment, "nonexistent text", "replacement");
        assert!(matches!(result, SurgicalEditResult::NotFound));
    }

    #[test]
    fn test_surgical_edit_in_heading() {
        let doc = setup_doc("# My Title\n\nSome body text.");
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");

        let result = surgical_text_edit(&mut txn, &fragment, "My Title", "New Title");
        assert!(matches!(result, SurgicalEditResult::Applied));
        drop(txn);

        let txn = doc.transact();
        let frag = txn.get_xml_fragment("content").unwrap();
        let output = xml_fragment_to_markdown(&txn, frag);
        assert!(output.contains("# New Title"), "Heading should be updated. Got: {}", output);
        assert!(output.contains("Some body text."), "Body should be unchanged. Got: {}", output);
    }

    #[test]
    fn test_surgical_edit_in_list_item() {
        let doc = setup_doc("- First item\n- Second item\n- Third item");
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");

        let result = surgical_text_edit(&mut txn, &fragment, "Second item", "Updated item");
        assert!(matches!(result, SurgicalEditResult::Applied));
        drop(txn);

        let txn = doc.transact();
        let frag = txn.get_xml_fragment("content").unwrap();
        let output = xml_fragment_to_markdown(&txn, frag);
        assert!(output.contains("Updated item"), "List item should be updated. Got: {}", output);
        assert!(output.contains("First item"), "Other items should be unchanged. Got: {}", output);
        assert!(output.contains("Third item"), "Other items should be unchanged. Got: {}", output);
    }

    #[test]
    fn test_surgical_edit_in_code_block() {
        let doc = setup_doc("```rust\nfn main() {\n    println!(\"hello\");\n}\n```");
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");

        let result = surgical_text_edit(&mut txn, &fragment, "hello", "goodbye");
        assert!(matches!(result, SurgicalEditResult::Applied));
        drop(txn);

        let txn = doc.transact();
        let frag = txn.get_xml_fragment("content").unwrap();
        let output = xml_fragment_to_markdown(&txn, frag);
        assert!(output.contains("goodbye"), "Code block should be updated. Got: {}", output);
        assert!(!output.contains("hello"), "Old text should be gone. Got: {}", output);
    }

    #[test]
    fn test_surgical_edit_preserves_file_card() {
        let doc = setup_doc("See [report.pdf](/drive/abc) for details about the project.");
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");

        let result = surgical_text_edit(&mut txn, &fragment, "details about the project", "more info");
        assert!(matches!(result, SurgicalEditResult::Applied));
        drop(txn);

        let txn = doc.transact();
        let frag = txn.get_xml_fragment("content").unwrap();
        let output = xml_fragment_to_markdown(&txn, frag);
        assert!(output.contains("[report.pdf](/drive/abc)"), "File card should survive. Got: {}", output);
        assert!(output.contains("more info"), "Replacement should be present. Got: {}", output);
    }

    #[test]
    fn test_surgical_edit_in_blockquote() {
        let doc = setup_doc("> This is a quoted passage.");
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");

        let result = surgical_text_edit(&mut txn, &fragment, "quoted passage", "famous quote");
        assert!(matches!(result, SurgicalEditResult::Applied));
        drop(txn);

        let txn = doc.transact();
        let frag = txn.get_xml_fragment("content").unwrap();
        let output = xml_fragment_to_markdown(&txn, frag);
        assert!(output.contains("famous quote"), "Blockquote should be updated. Got: {}", output);
    }

    #[test]
    fn test_surgical_edit_markdown_syntax_falls_back() {
        // Find text containing markdown syntax won't match XmlText content
        let doc = setup_doc("This is **bold** text.");
        let mut txn = doc.transact_mut();
        let fragment = txn.get_or_insert_xml_fragment("content");

        // "**bold**" contains markdown syntax — won't be found in XmlText (which has "bold" without **)
        let result = surgical_text_edit(&mut txn, &fragment, "**bold**", "**strong**");
        assert!(matches!(result, SurgicalEditResult::NotFound),
            "Markdown syntax in find text should not match XmlText content");
    }
}
