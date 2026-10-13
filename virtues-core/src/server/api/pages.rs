//! Pages: the documents, sharing, versions, raw records and local search.

use axum::{
    extract::{FromRef, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json,
    Router,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use super::{api_response, error_response, success_message};
use crate::server::yjs::YjsState;
use crate::server::AppState;

/// This area's authenticated routes. Merged into the protected router, whose
/// `route_layer` requires a resolved `AuthUser`.
pub fn routes() -> Router<AppState> {
    Router::new()
        // Local content search — the ⌘K palette. Never leaves the box.
        .route("/api/search/local", post(search_local_handler))
        // Raw record viewer — one life-graph row by (ontology, id)
        .route(
            "/api/records/:ontology/:record_id",
            get(get_record_handler),
        )
        .merge(page_routes())
}

/// The pages routes. They read only the pool and the page docs, so any state
/// that holds both serves them: the app's, or a test's.
fn page_routes<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    PgPool: FromRef<S>,
    YjsState: FromRef<S>,
{
    Router::new()
        .route(
            "/api/pages",
            get(list_pages_handler).post(create_page_handler),
        )
        .route(
            "/api/pages/search/refs",
            get(search_refs_handler),
        )
        .route(
            "/api/pages/:id",
            get(get_page_handler)
                .put(update_page_handler)
                .delete(delete_page_handler),
        )
        // Page References (backlinks) API
        .route(
            "/api/pages/:id/backlinks",
            get(get_page_backlinks_handler),
        )
        // Append a markdown block through Yjs (safe with an open editor) — the
        // synthesis bridge's write path.
        .route("/api/pages/:id/append", post(append_page_handler))
        // The page's markdown from its live document, either format.
        .route("/api/pages/:id/markdown", get(get_page_markdown_handler))
        // Page Versions API
        .route(
            "/api/pages/:id/versions",
            get(list_page_versions_handler).post(create_page_version_handler),
        )
        .route(
            "/api/pages/:id/versions/:version_id/restore",
            post(restore_page_version_handler),
        )
        .route(
            "/api/pages/versions/:version_id",
            get(get_page_version_handler),
        )
        // A device's copy of a page from before it moved to blocks.
        .route("/api/pages/:id/device-copy", post(keep_device_copy_handler))
        // Yjs WebSocket (real-time collaborative editing)
        .route("/ws/yjs/:page_id", get(crate::server::yjs::yjs_websocket_handler))
}


// ============================================================================
// Pages Handlers
// ============================================================================

/// Query params for pages list
#[derive(Debug, Deserialize)]
pub struct ListPagesQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub workspace_id: Option<String>,
}

/// GET /api/pages - List all pages
pub async fn list_pages_handler(
    State(pool): State<PgPool>,
    Query(query): Query<ListPagesQuery>,
) -> Response {
    // Note: workspace_id filter removed - views handle filtering now
    api_response(crate::api::pages::list_pages(&pool, query.limit, query.offset).await)
}

/// A page, with the document contract it is written under and the one this
/// server reads.
#[derive(Serialize)]
struct PageWithContract {
    #[serde(flatten)]
    page: crate::api::pages::Page,
    /// What the page's socket binds a client at (`YjsState::page_contract`):
    /// the editor shows the copy of the page it kept only when it reads this
    /// contract. Null when the page's document could not be read.
    contract: Option<u32>,
    /// The contract this server reads and checks writes against. An app
    /// whose contract is newer opens a block page read-only and says the
    /// server needs an update: the server could not check what it writes.
    box_contract: u32,
    /// `page` or `article`: an article's markdown reads as an article in the
    /// editor (its footnotes, figures and veils).
    kind: String,
    /// Which copy of a block page's document this is (`tree_lineage`): a
    /// device keeps its copy of the page under it, and sends the server any
    /// copy it kept under another. Null for a markdown page, and for a block
    /// page from before lineages.
    lineage: Option<String>,
}

/// GET /api/pages/:id - Get a single page.
///
/// The page's document is built before its row is read: building it moves
/// an article nobody had opened to blocks (`block_move::first_open`), so the
/// row read after says `format: "tree"` and the contract and lineage the
/// app binds at once.
pub async fn get_page_handler(
    State(pool): State<PgPool>,
    State(yjs): State<YjsState>,
    Path(id): Path<String>,
) -> Response {
    let contract = yjs.page_contract(&id).await;
    let page = match crate::api::pages::get_page(&pool, &id).await {
        Ok(page) => page,
        Err(e) => return error_response(e),
    };
    let contract = match contract {
        Ok(contract) => Some(contract),
        Err(error) => {
            tracing::warn!(page_id = %id, %error, "could not read the page's document contract");
            None
        }
    };
    let (kind, lineage) = match crate::api::pages::kind_and_lineage(&pool, &id).await {
        Ok(read) => read,
        Err(e) => return error_response(e),
    };
    Json(PageWithContract {
        page,
        contract,
        box_contract: virtues_document::contract().version,
        kind,
        lineage,
    })
    .into_response()
}

/// GET /api/records/:ontology/:record_id - fetch one raw life-graph record.
pub async fn get_record_handler(
    State(state): State<AppState>,
    Path((ontology, record_id)): Path<(String, String)>,
) -> Response {
    api_response(crate::api::records::get_record(state.db.pool(), &ontology, &record_id).await)
}

/// POST /api/pages - Create a new page, in the format the request names or
/// the server's default. A block page's body carries `notes` when converting
/// its markdown changed something.
pub async fn create_page_handler(
    State(pool): State<PgPool>,
    Json(request): Json<crate::api::pages::CreatePageRequest>,
) -> Response {
    match crate::api::pages::create_page_reporting(&pool, request).await {
        Ok(created) => (StatusCode::CREATED, Json(created)).into_response(),
        Err(e) => error_response(e),
    }
}

/// PUT /api/pages/:id - Update a page. A block page refuses `content`.
pub async fn update_page_handler(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::pages::UpdatePageRequest>,
) -> Response {
    api_response(crate::api::pages::update_page(&pool, &id, request).await)
}

/// DELETE /api/pages/:id - Delete a page
pub async fn delete_page_handler(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Response {
    match crate::api::pages::delete_page(&pool, &id).await {
        Ok(_) => success_message("Page deleted successfully"),
        Err(e) => error_response(e),
    }
}

/// POST /api/pages/:id/append — append a markdown block to a page THROUGH Yjs.
///
/// The synthesis-bridge write path (researcher-plan D4): sending a highlight
/// into a page must not clobber an editor that's currently open on it, so this
/// goes through the authoritative Yjs doc (transaction + broadcast) rather than
/// a REST content replace.
#[derive(Debug, Deserialize)]
pub struct AppendPageRequest {
    pub markdown: String,
}

pub async fn append_page_handler(
    State(yjs): State<YjsState>,
    Path(id): Path<String>,
    Json(req): Json<AppendPageRequest>,
) -> Response {
    if req.markdown.trim().is_empty() {
        return error_response(crate::error::Error::InvalidInput(
            "markdown cannot be empty".into(),
        ));
    }
    match yjs.append_markdown(&id, &req.markdown).await {
        Ok(appended) => {
            let mut body = serde_json::json!({ "content": appended.text });
            // What converting the markdown into a block page's blocks changed.
            if !appended.notes.is_empty() {
                body["notes"] = serde_json::json!(appended.notes);
            }
            api_response(Ok(body))
        }
        Err(e) => error_response(e),
    }
}

/// A page's markdown as its live document holds it.
#[derive(Debug, Serialize)]
struct PageMarkdown {
    format: crate::api::pages::PageFormat,
    markdown: String,
}

/// GET /api/pages/:id/markdown - the page's markdown from its live document:
/// a markdown page's text, or a block page's export, including edits not
/// saved yet. What Copy markdown and View as markdown show. The text is read
/// first: reading it can move an article nobody had opened to blocks, and
/// the format read after says what the text is.
pub async fn get_page_markdown_handler(
    State(pool): State<PgPool>,
    State(yjs): State<YjsState>,
    Path(id): Path<String>,
) -> Response {
    let markdown = match yjs.read_text(&id).await {
        Ok(markdown) => markdown,
        Err(e) => {
            // A missing or trashed page is `NotFound`, as the format says.
            if let Err(missing) = crate::api::pages::page_format(&pool, &id).await {
                return error_response(missing);
            }
            return error_response(crate::error::Error::Other(e));
        }
    };
    match crate::api::pages::page_format(&pool, &id).await {
        Ok(format) => Json(PageMarkdown { format, markdown }).into_response(),
        Err(e) => error_response(e),
    }
}

/// GET /api/pages/:id/backlinks - Get inbound references (pages linking here)
pub async fn get_page_backlinks_handler(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Response {
    api_response(crate::api::pages::get_page_backlinks(&pool, &id).await)
}

/// Query params for entity search
#[derive(Debug, Deserialize)]
pub struct EntitySearchQuery {
    pub q: String,
}

/// GET /api/pages/search/refs - Search entities for autocomplete
pub async fn search_refs_handler(
    State(pool): State<PgPool>,
    Query(query): Query<EntitySearchQuery>,
) -> Response {
    api_response(crate::api::pages::search_refs(&pool, &query.q).await)
}

// ============================================================================
// Page Versions Handlers
// ============================================================================

/// Query params for versions list
#[derive(Debug, Deserialize)]
pub struct ListVersionsQuery {
    pub limit: Option<i64>,
}

/// GET /api/pages/:id/versions - List versions for a page
pub async fn list_page_versions_handler(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Query(query): Query<ListVersionsQuery>,
) -> Response {
    api_response(crate::api::pages::list_versions(&pool, &id, query.limit).await)
}

/// POST /api/pages/:id/versions - Cut a version: from the page as it is now
/// when the request sends no snapshot, or a markdown editor's own copy.
pub async fn create_page_version_handler(
    State(pool): State<PgPool>,
    State(yjs): State<YjsState>,
    Path(id): Path<String>,
    Json(request): Json<crate::api::pages::NewVersionRequest>,
) -> Response {
    match crate::api::pages::post_version(&pool, &yjs, &id, request).await {
        Ok(version) => (StatusCode::CREATED, Json(version)).into_response(),
        Err(e) => error_response(e),
    }
}

/// POST /api/pages/:id/versions/:version_id/restore - put a block page back
/// as one of its versions, on the server. The body is ignored.
pub async fn restore_page_version_handler(
    State(pool): State<PgPool>,
    State(yjs): State<YjsState>,
    Path((id, version_id)): Path<(String, String)>,
) -> Response {
    api_response(crate::api::pages::restore_version(&pool, &yjs, &id, &version_id).await)
}

/// A device's copy of a page, as `POST /api/pages/:id/device-copy` takes it.
#[derive(Debug, Deserialize)]
pub struct DeviceCopyRequest {
    /// The copy's Yjs state, base64.
    pub state: String,
    /// The lineage of the block page's document the copy was kept under;
    /// absent for a copy of the page in markdown, or of a block page from
    /// before lineages.
    #[serde(default)]
    pub lineage: Option<String>,
}

/// POST /api/pages/:id/device-copy - a copy a device kept of a block page
/// that is not the page's document: one from before it moved to blocks, or
/// one of an earlier document of it. Sent the first time the device opens
/// the page: `{ "kept": <version> }` when it held typing the page never had,
/// kept as a version (`pages::keep_device_copy_of`), else `{ "kept": null }`.
pub async fn keep_device_copy_handler(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Json(request): Json<DeviceCopyRequest>,
) -> Response {
    use base64::Engine as _;
    let Ok(state) = base64::engine::general_purpose::STANDARD.decode(&request.state) else {
        return error_response(crate::error::Error::InvalidInput("The copy isn't base64.".into()));
    };
    api_response(
        crate::api::pages::keep_device_copy_of(&pool, &id, &state, request.lineage.as_deref())
            .await
            .map(|kept| serde_json::json!({ "kept": kept })),
    )
}

/// GET /api/pages/versions/:version_id - A version and what it says: a
/// markdown version's snapshot, or a block version's export and preview HTML.
pub async fn get_page_version_handler(
    State(pool): State<PgPool>,
    Path(version_id): Path<String>,
) -> Response {
    api_response(crate::api::pages::get_version(&pool, &version_id).await)
}

// ============================================================================
// Local content search (⌘K)
// ============================================================================

/// POST /api/search/local — content hits for the command palette.
///
/// POST rather than GET because the query is user text: a GET would put what
/// someone is searching their own life for into the URL, where it lands in
/// history and any access log. Same reason the rest of the search surface
/// posts.
pub async fn search_local_handler(
    State(state): State<AppState>,
    Json(request): Json<crate::api::search_local::LocalSearchRequest>,
) -> Response {
    api_response(crate::api::search_local::search_local(state.db.pool(), request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::pages::{create_page_as, CreatePageRequest, PageFormat};
    use axum::body::Body;
    use axum::http::{Method, Request};
    use serde_json::{json, Value};
    use tower::Service;

    /// The two things the pages routes read, as a state of their own.
    #[derive(Clone)]
    struct TestState {
        pool: PgPool,
        yjs: YjsState,
    }

    impl FromRef<TestState> for PgPool {
        fn from_ref(state: &TestState) -> Self {
            state.pool.clone()
        }
    }

    impl FromRef<TestState> for YjsState {
        fn from_ref(state: &TestState) -> Self {
            state.yjs.clone()
        }
    }

    /// The pages routes as the app serves them, with the app's body limit:
    /// axum's own default would refuse a large paste before its handler
    /// could say why.
    fn app(pool: &PgPool) -> (Router, YjsState) {
        app_with(pool, YjsState::new(pool.clone()))
    }

    /// [`app`] on `yjs`.
    fn app_with(pool: &PgPool, yjs: YjsState) -> (Router, YjsState) {
        let router = page_routes()
            .with_state(TestState {
                pool: pool.clone(),
                yjs: yjs.clone(),
            })
            .layer(axum::extract::DefaultBodyLimit::max(260 * 1024 * 1024));
        (router, yjs)
    }

    async fn call(app: &mut Router, method: Method, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        let request = Request::builder().method(method).uri(uri);
        let request = match body {
            Some(body) => request
                .header("content-type", "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();
        let response = app.call(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
        (status, body)
    }

    async fn page_as(pool: &PgPool, content: &str, format: PageFormat) -> String {
        let request = CreatePageRequest {
            title: "Trip".into(),
            content: content.into(),
            project_id: None,
            icon: None,
            icon_color: None,
            cover_url: None,
            tags: None,
            format: None,
        };
        create_page_as(pool, request, format).await.unwrap().page.id
    }

    async fn stored(pool: &PgPool, page_id: &str) -> (String, Option<Vec<u8>>) {
        sqlx::query_as("SELECT content, yjs_state FROM app_pages WHERE id = $1")
            .bind(page_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    fn error_of(body: &Value) -> &str {
        body["error"].as_str().expect("the body names an error")
    }

    #[sqlx::test]
    async fn a_page_says_its_format_and_the_contracts_on_both_sides(pool: PgPool) {
        let (mut app, _) = app(&pool);
        let version = virtues_document::contract().version;

        let tree = page_as(&pool, "## Plan\n\nLunch.\n", PageFormat::Tree).await;
        let (status, body) = call(&mut app, Method::GET, &format!("/api/pages/{tree}"), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["format"], "tree");
        assert_eq!(body["contract"], json!(version));
        assert_eq!(body["box_contract"], json!(version));
        assert_eq!(body["content"], "## Plan\n\nLunch.\n");

        let markdown = page_as(&pool, "Notes.\n", PageFormat::Markdown).await;
        let (_, body) = call(&mut app, Method::GET, &format!("/api/pages/{markdown}"), None).await;
        assert_eq!(body["format"], "markdown");
        assert_eq!(body["contract"], json!(0));
        assert_eq!(body["box_contract"], json!(version));
    }

    /// Tests never set the flag, so it is off here as on every real box.
    #[sqlx::test]
    async fn creating_a_block_page_is_refused_while_the_flag_is_off(pool: PgPool) {
        if crate::api::pages::tree_pages_enabled() {
            eprintln!("VIRTUES_TREE_PAGES is set; the flag-off case cannot run here");
            return;
        }
        let (mut app, _) = app(&pool);
        let (status, body) = call(
            &mut app,
            Method::POST,
            "/api/pages",
            Some(json!({ "title": "Trip", "content": "Lunch.\n", "format": "tree" })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(error_of(&body).contains("Block pages aren't turned on"), "{body}");

        let (status, body) = call(
            &mut app,
            Method::POST,
            "/api/pages",
            Some(json!({ "title": "Trip", "content": "Lunch.\n" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert_eq!(body["format"], "markdown");
        assert!(body.get("notes").is_none(), "{body}");
    }

    #[sqlx::test]
    async fn text_put_on_a_block_page_is_refused_and_nothing_written(pool: PgPool) {
        let (mut app, _) = app(&pool);
        let tree = page_as(&pool, "Lunch.\n", PageFormat::Tree).await;
        let before = stored(&pool, &tree).await;
        let (status, body) = call(
            &mut app,
            Method::PUT,
            &format!("/api/pages/{tree}"),
            Some(json!({ "content": "Overwritten.\n" })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(error_of(&body).contains(crate::api::pages::TREE_CONTENT_REFUSED), "{body}");
        assert_eq!(stored(&pool, &tree).await, before);
    }

    #[sqlx::test]
    async fn appending_to_a_block_page_says_what_the_conversion_changed(pool: PgPool) {
        let (mut app, _) = app(&pool);
        let tree = page_as(&pool, "## Plan\n", PageFormat::Tree).await;
        let (status, body) = call(
            &mut app,
            Method::POST,
            &format!("/api/pages/{tree}/append"),
            Some(json!({ "markdown": "---\nsource: pdf\n---\n\n> A highlight.\n" })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["content"], "## Plan\n\n> A highlight.\n");
        let notes = body["notes"].as_array().expect("notes");
        assert!(notes.iter().any(|n| n["message"].as_str().unwrap().contains("front matter")), "{body}");

        // Markdown the converter refuses is the sender's to fix: a 400
        // naming where and why, and nothing written.
        let before = stored(&pool, &tree).await;
        let deep = format!("{}Deep.\n", "> ".repeat(150));
        let (status, body) = call(
            &mut app,
            Method::POST,
            &format!("/api/pages/{tree}/append"),
            Some(json!({ "markdown": deep })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        let error = error_of(&body);
        assert!(error.contains("couldn't turn this markdown into blocks"), "{body}");
        assert!(error.contains("nest"), "the problem is named: {body}");
        assert_eq!(stored(&pool, &tree).await, before);
    }

    /// lib0's var-uint, as y-websocket frames an update.
    fn sync_update(update: &[u8]) -> Vec<u8> {
        let mut message = vec![0, 2];
        let mut n = update.len();
        while n >= 0x80 {
            message.push((n as u8 & 0x7f) | 0x80);
            n >>= 7;
        }
        message.push(n as u8);
        message.extend_from_slice(update);
        message
    }

    /// The markdown route reads the live document: an editor's keystroke
    /// shows before the debounced save writes it to the database.
    #[sqlx::test]
    async fn markdown_reads_an_editors_edit_before_it_is_saved(pool: PgPool) {
        use futures::SinkExt;
        use yrs::{updates::decoder::Decode, ReadTxn, Text, Transact, WriteTxn, XmlFragment, XmlOut};
        let (mut app, yjs) = app(&pool);
        let tree = page_as(&pool, "Lunch.\n", PageFormat::Tree).await;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let served = app.clone();
        tokio::spawn(async move { axum::serve(listener, served).await.unwrap() });
        let version = virtues_document::contract().version;
        let (mut socket, _) =
            tokio_tungstenite::connect_async(format!("ws://{addr}/ws/yjs/{tree}?contract={version}"))
                .await
                .unwrap();

        let editor = virtues_document::new_doc();
        let state = yjs.text_and_state(&tree).await.unwrap().state;
        editor
            .transact_mut()
            .apply_update(yrs::Update::decode_v1(&state).unwrap())
            .unwrap();
        let sv = editor.transact().state_vector();
        {
            let mut txn = editor.transact_mut();
            let frag = txn.get_or_insert_xml_fragment("doc");
            let Some(XmlOut::Element(p)) = frag.get(&txn, 0) else { panic!("a paragraph") };
            let Some(XmlOut::Text(t)) = p.get(&txn, 0) else { panic!("its text") };
            t.insert(&mut txn, 0, "Late ");
        }
        let typed = editor.transact().encode_diff_v1(&sv);
        socket
            .send(tokio_tungstenite::tungstenite::Message::Binary(sync_update(&typed)))
            .await
            .unwrap();

        let uri = format!("/api/pages/{tree}/markdown");
        let mut read = Value::Null;
        for _ in 0..200 {
            read = call(&mut app, Method::GET, &uri, None).await.1;
            if read["markdown"] == "Late Lunch.\n" {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(read, json!({ "format": "tree", "markdown": "Late Lunch.\n" }));
        assert_eq!(stored(&pool, &tree).await.0, "Lunch.\n", "not saved yet");

        let (status, _) = call(&mut app, Method::GET, "/api/pages/page_nosuch/markdown", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[sqlx::test]
    async fn versions_are_cut_by_the_server_and_read_back_by_format(pool: PgPool) {
        let (mut app, yjs) = app(&pool);
        let tree = page_as(&pool, "## Plan\n\nLunch.\n", PageFormat::Tree).await;
        let uri = format!("/api/pages/{tree}/versions");

        let (status, version) =
            call(&mut app, Method::POST, &uri, Some(json!({ "created_by": "user" }))).await;
        assert_eq!(status, StatusCode::CREATED, "{version}");
        assert_eq!(version["version_number"], 1);

        let copy = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            yjs.text_and_state(&tree).await.unwrap().state,
        );
        let (status, body) = call(
            &mut app,
            Method::POST,
            &uri,
            Some(json!({ "snapshot": copy, "content_preview": "x", "created_by": "auto" })),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(error_of(&body).contains(crate::api::pages::TREE_SNAPSHOT_REFUSED), "{body}");

        let id = version["id"].as_str().unwrap();
        let (status, detail) =
            call(&mut app, Method::GET, &format!("/api/pages/versions/{id}"), None).await;
        assert_eq!(status, StatusCode::OK, "{detail}");
        assert_eq!(detail["format"], "tree");
        assert_eq!(detail["markdown"], "## Plan\n\nLunch.\n");
        assert_eq!(detail["html"], "<h2>Plan</h2><p>Lunch.</p>");
        assert!(detail["snapshot"].is_null(), "{detail}");

        // A markdown page's editor sends its copy, as before.
        let markdown = page_as(&pool, "Notes.\n", PageFormat::Markdown).await;
        let copy = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            crate::server::yjs::state_from_text("Notes.\n"),
        );
        let (status, version) = call(
            &mut app,
            Method::POST,
            &format!("/api/pages/{markdown}/versions"),
            Some(json!({ "snapshot": copy, "content_preview": "Notes.\n", "created_by": "auto" })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{version}");
        let id = version["id"].as_str().unwrap();
        let (_, detail) = call(&mut app, Method::GET, &format!("/api/pages/versions/{id}"), None).await;
        assert_eq!(detail["format"], "markdown");
        assert_eq!(detail["snapshot"], json!(copy));
        assert!(detail.get("html").is_none(), "{detail}");
    }

    #[sqlx::test]
    async fn a_restore_answers_by_route(pool: PgPool) {
        let (mut app, yjs) = app(&pool);
        let tree = page_as(&pool, "Lunch.\n", PageFormat::Tree).await;
        let (_, version) = call(
            &mut app,
            Method::POST,
            &format!("/api/pages/{tree}/versions"),
            Some(json!({ "created_by": "user" })),
        )
        .await;
        let v1 = version["id"].as_str().unwrap().to_string();
        let base = yjs.read_tree(&tree).await.unwrap();
        yjs.edit_tree(
            &tree,
            Some(&virtues_document::Seen::of_page(&base)),
            &[virtues_document::Op::Append {
                html: "<p>Tea.</p>".into(),
            }],
        )
        .await
        .unwrap();

        let restore = format!("/api/pages/{tree}/versions/{v1}/restore");
        let (status, body) = call(&mut app, Method::POST, &restore, Some(json!({}))).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["changed"], true);
        assert_eq!(body["notes"], json!([]));
        assert!(body["version_number"].as_i64().is_some(), "{body}");
        assert_eq!(yjs.read_text(&tree).await.unwrap(), "Lunch.\n");

        // Again: the page already reads as the version.
        let (status, body) = call(&mut app, Method::POST, &restore, None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!((body["changed"].as_bool(), body["version_number"].is_null()), (Some(false), true));

        let markdown = page_as(&pool, "Notes.\n", PageFormat::Markdown).await;
        let (_, version) = call(
            &mut app,
            Method::POST,
            &format!("/api/pages/{markdown}/versions"),
            Some(json!({ "created_by": "user" })),
        )
        .await;
        let mv = version["id"].as_str().unwrap();
        let (status, body) = call(
            &mut app,
            Method::POST,
            &format!("/api/pages/{markdown}/versions/{mv}/restore"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(error_of(&body).contains(crate::api::pages::RESTORE_IN_EDITOR), "{body}");

        let (status, _) = call(
            &mut app,
            Method::POST,
            &format!("/api/pages/{tree}/versions/{mv}/restore"),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "another page's version");
    }

    /// The page's row is read after its document is built, so the GET that
    /// first builds an article's document answers with the blocks it moved
    /// to: their format, contract and lineage, and that it is an article.
    #[sqlx::test]
    async fn get_moves_an_article_and_says_tree_lineage_and_kind(pool: PgPool) {
        let (mut app, _) = app_with(&pool, YjsState::with_tree_articles(pool.clone(), true));
        let article = crate::api::wiki_articles::create_article(
            &pool,
            "person",
            "person_1",
            "Nick",
            "You met Nick at the R&B night.\n",
        )
        .await
        .unwrap();
        let (status, body) = call(&mut app, Method::GET, &format!("/api/pages/{}", article.page_id), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["format"], "tree");
        assert_eq!(body["kind"], "article");
        assert_eq!(body["contract"], json!(virtues_document::contract().version));
        assert_eq!(body["content"], "You met Nick at the R\\&B night.\n");
        let lineage = body["lineage"].as_str().expect("a lineage");
        assert!(lineage.len() == 16 && lineage.bytes().all(|b| b.is_ascii_hexdigit()), "{lineage}");

        let markdown = page_as(&pool, "Notes.\n", PageFormat::Markdown).await;
        let (_, body) = call(&mut app, Method::GET, &format!("/api/pages/{markdown}"), None).await;
        assert_eq!((body["kind"].as_str(), body["lineage"].is_null()), (Some("page"), true));
        // A page a wiki article owns is an article whatever its kind, as the
        // server reads and moves it.
        sqlx::query("INSERT INTO wiki_articles (id, subject_type, subject_id, page_id) VALUES ('wa_owned', 'person', 'person_2', $1)")
            .bind(&markdown)
            .execute(&pool)
            .await
            .unwrap();
        let (_, body) = call(&mut app, Method::GET, &format!("/api/pages/{markdown}"), None).await;
        assert_eq!(body["kind"], "article");
        let tree = page_as(&pool, "Notes.\n", PageFormat::Tree).await;
        let (_, body) = call(&mut app, Method::GET, &format!("/api/pages/{tree}"), None).await;
        assert!(body["lineage"].as_str().is_some(), "a block page made today has its lineage: {body}");
        let (status, _) = call(&mut app, Method::GET, "/api/pages/page_nosuch", None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// The markdown door reads the text before the format, so an article
    /// its read moved to blocks answers as blocks.
    #[sqlx::test]
    async fn markdown_door_reads_the_text_first(pool: PgPool) {
        let (mut app, _) = app_with(&pool, YjsState::with_tree_articles(pool.clone(), true));
        let article =
            crate::api::wiki_articles::create_article(&pool, "person", "person_1", "Nick", "R&B night.\n")
                .await
                .unwrap();
        let (status, body) =
            call(&mut app, Method::GET, &format!("/api/pages/{}/markdown", article.page_id), None).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body, json!({ "format": "tree", "markdown": "R\\&B night.\n" }));
    }
}
