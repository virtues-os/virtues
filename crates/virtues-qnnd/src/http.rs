//! The llama-server-compatible HTTP surface.
//!
//! This is what makes the NPU daemon a drop-in implementation of the box's one
//! inference contract: the same four routes virtues-core already speaks to
//! llama-server (and to any BYO endpoint), backed by the Hexagon NPU instead of
//! a GGUF. virtues-core needs no QNN-specific code path at all — the installer
//! just points `VIRTUES_EMBED_URL`/`VIRTUES_RERANK_URL` here.
//!
//! - `GET  /health`        → 200 once the engine answers (core's liveness gate)
//! - `GET  /v1/models`     → `{"data":[{"id":"gte-small"}]}` (core's model probe;
//!                            first entry is what gets stamped on indexed rows)
//! - `POST /v1/embeddings` → OpenAI shape: `{input, model?}` →
//!                            `{"data":[{index, embedding}]}`
//! - `POST /v1/rerank`     → Jina/Cohere shape (what llama.cpp ships):
//!                            `{query, documents, top_n?}` →
//!                            `{"results":[{index, relevance_score}]}`

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::engine::{QnnClient, COLBERT_MODEL, GTE_MODEL};

pub fn router(client: Arc<QnnClient>) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(models))
        .route("/v1/embeddings", post(embeddings))
        .route("/v1/rerank", post(rerank))
        .with_state(client)
}

/// Liveness — proves the whole path (HTTP → TCP loop → NPU execute) with one
/// tiny embed, mirroring what llama-server's /health means ("model loaded and
/// able to serve"), so core's startup gate can't pass while the engine is dead.
async fn health(State(client): State<Arc<QnnClient>>) -> Response {
    match client.embed("health probe").await {
        Ok(_) => (StatusCode::OK, Json(json!({"status": "ok"}))).into_response(),
        Err(e) => err_response(StatusCode::SERVICE_UNAVAILABLE, &e.to_string()),
    }
}

/// Model identity. Core's `probe_served_model` reads `data[0].id` and stamps it
/// on every indexed vector — so this MUST be the embed model, and MUST stay
/// `gte-small`: it's the same stamp the old native-client path wrote, which is
/// what keeps a Dragon box's existing index valid across the consolidation.
async fn models() -> Json<serde_json::Value> {
    Json(json!({
        "data": [
            {"id": GTE_MODEL, "object": "model"},
            {"id": COLBERT_MODEL, "object": "model"},
        ]
    }))
}

#[derive(Deserialize)]
struct EmbeddingsRequest {
    input: EmbedInput,
    // `model` accepted-and-ignored, like llama-server: the NPU context binary
    // serves exactly one model regardless of the routing key sent.
    #[allow(dead_code)]
    model: Option<String>,
}

/// OpenAI's `input` is a string or an array of strings; core sends the array
/// form but a drop-in endpoint should take both.
#[derive(Deserialize)]
#[serde(untagged)]
enum EmbedInput {
    One(String),
    Many(Vec<String>),
}

async fn embeddings(
    State(client): State<Arc<QnnClient>>,
    Json(req): Json<EmbeddingsRequest>,
) -> Response {
    let inputs = match req.input {
        EmbedInput::One(s) => vec![s],
        EmbedInput::Many(v) => v,
    };
    match client.embed_batch(&inputs).await {
        Ok(vecs) => {
            let data: Vec<_> = vecs
                .into_iter()
                .enumerate()
                .map(|(index, embedding)| {
                    json!({"object": "embedding", "index": index, "embedding": embedding})
                })
                .collect();
            (
                StatusCode::OK,
                Json(json!({"object": "list", "model": GTE_MODEL, "data": data})),
            )
                .into_response()
        }
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

#[derive(Deserialize)]
struct RerankRequest {
    query: String,
    documents: Vec<String>,
    // Accepted for wire compatibility; we always score and return every
    // document — core sends top_n = documents.len() and sorts by index anyway.
    #[allow(dead_code)]
    top_n: Option<usize>,
    #[allow(dead_code)]
    model: Option<String>,
}

async fn rerank(
    State(client): State<Arc<QnnClient>>,
    Json(req): Json<RerankRequest>,
) -> Response {
    match client.rerank(&req.query, &req.documents).await {
        Ok(scores) => {
            let results: Vec<_> = scores
                .into_iter()
                .enumerate()
                .map(|(index, score)| json!({"index": index, "relevance_score": score}))
                .collect();
            (
                StatusCode::OK,
                Json(json!({"model": COLBERT_MODEL, "results": results})),
            )
                .into_response()
        }
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

fn err_response(code: StatusCode, msg: &str) -> Response {
    (code, Json(json!({"error": {"message": msg}}))).into_response()
}

// ── end-to-end test: HTTP → packing → mock TCP daemon → response ────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tower::util::ServiceExt; // Router::oneshot

    /// A minimal-but-valid `tokenizer.json` (WordPiece + whitespace pre-tok) so
    /// the client constructs without the shipped model tokenizers. "hello" → 7,
    /// "world" → 8.
    const TINY_TOKENIZER: &str = r###"{
        "version": "1.0",
        "truncation": null, "padding": null, "added_tokens": [],
        "normalizer": null,
        "pre_tokenizer": {"type": "Whitespace"},
        "post_processor": null, "decoder": null,
        "model": {
            "type": "WordPiece", "unk_token": "[UNK]",
            "continuing_subword_prefix": "##", "max_input_chars_per_word": 100,
            "vocab": {"[UNK]": 3, "hello": 7, "world": 8}
        }
    }"###;

    /// Mock the C++ daemon's binary loop: embed (idx 0) answers each 128-token
    /// input with 384 floats [2, 0, 0, …] (norm 2 → L2-normalizes to [1, 0, …]);
    /// rerank (idx 1) answers 256×96 floats all 0.5.
    async fn spawn_mock_daemon() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                tokio::spawn(async move {
                    let mut hdr = [0u8; 8];
                    if sock.read_exact(&mut hdr).await.is_err() {
                        return;
                    }
                    let model_idx = u32::from_le_bytes(hdr[0..4].try_into().unwrap());
                    let nbytes = u32::from_le_bytes(hdr[4..8].try_into().unwrap()) as usize;
                    let mut payload = vec![0u8; nbytes];
                    if sock.read_exact(&mut payload).await.is_err() {
                        return;
                    }
                    let out: Vec<f32> = match model_idx {
                        0 => {
                            let batch = nbytes / 4 / 128;
                            let mut v = Vec::with_capacity(batch * 384);
                            for _ in 0..batch {
                                v.push(2.0);
                                v.extend(std::iter::repeat(0.0).take(383));
                            }
                            v
                        }
                        _ => vec![0.5; 256 * 96],
                    };
                    let mut resp = Vec::with_capacity(8 + out.len() * 4);
                    resp.extend_from_slice(&0u32.to_le_bytes());
                    resp.extend_from_slice(&((out.len() * 4) as u32).to_le_bytes());
                    for f in out {
                        resp.extend_from_slice(&f.to_le_bytes());
                    }
                    let _ = sock.write_all(&resp).await;
                });
            }
        });
        addr
    }

    /// The tokenizer fixture, written exactly once per test binary.
    ///
    /// Keying the directory on `process::id()` is not enough: `cargo test` runs
    /// the tests of one binary as parallel *threads* of one process, so all four
    /// `test_router()` callers used to race on the same `tokenizer.json`.
    /// `fs::write` truncates before it writes, so a reader that arrived mid-write
    /// saw an empty file and failed with "EOF while parsing a value at line 1
    /// column 0" — a flake that blocked unrelated PRs. `OnceLock` makes the
    /// write happen once and every later caller wait for it.
    fn write_tokenizers() -> std::path::PathBuf {
        static FIXTURE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
        FIXTURE
            .get_or_init(|| {
                let dir = std::env::temp_dir().join(format!("qnnd-test-{}", std::process::id()));
                for sub in ["tok_gte", "tok_colbert"] {
                    let d = dir.join(sub);
                    std::fs::create_dir_all(&d).unwrap();
                    // Write-then-rename so a torn file is never observable, even
                    // if a stale directory survives from an earlier run.
                    let tmp = d.join("tokenizer.json.tmp");
                    std::fs::write(&tmp, TINY_TOKENIZER).unwrap();
                    std::fs::rename(&tmp, d.join("tokenizer.json")).unwrap();
                }
                dir
            })
            .clone()
    }

    async fn test_router() -> Router {
        let addr = spawn_mock_daemon().await;
        let models_dir = write_tokenizers();
        let client = QnnClient::new(addr, &models_dir).expect("client construct");
        router(Arc::new(client))
    }

    async fn body_json(resp: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn embeddings_end_to_end() {
        let app = test_router().await;
        let req = axum::http::Request::post("/v1/embeddings")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                r#"{"input": ["hello world", "world"], "model": "default"}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        assert_eq!(v["model"], "gte-small");
        let data = v["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);
        // Mock emits [2,0,…]; the client must L2-normalize to [1,0,…].
        assert_eq!(data[0]["index"], 0);
        let e = data[0]["embedding"].as_array().unwrap();
        assert_eq!(e.len(), 384);
        assert!((e[0].as_f64().unwrap() - 1.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn rerank_end_to_end() {
        let app = test_router().await;
        let req = axum::http::Request::post("/v1/rerank")
            .header("content-type", "application/json")
            .body(axum::body::Body::from(
                r#"{"query": "hello", "documents": ["world", "hello world"], "top_n": 2}"#,
            ))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        let results = v["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["index"], 0);
        // All token embeddings are 0.5-vectors (dim 96): every dot = 96·0.25 = 24;
        // MaxSim sums over the 32 valid query rows → 32 · 24 = 768.
        let score = results[0]["relevance_score"].as_f64().unwrap();
        assert!((score - 768.0).abs() < 1e-3);
    }

    #[tokio::test]
    async fn models_and_health() {
        let app = test_router().await;
        let resp = app
            .clone()
            .oneshot(axum::http::Request::get("/v1/models").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        let v = body_json(resp).await;
        // First data entry is what core stamps on indexed rows.
        assert_eq!(v["data"][0]["id"], "gte-small");

        let resp = app
            .oneshot(axum::http::Request::get("/health").body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
