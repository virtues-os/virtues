//! Inference mode resolution + manual-endpoint validation.
//!
//! Inference comes from exactly two places, decided once per install:
//!
//!   · **Dragon** — our own board, detected from the device tree. The
//!     installer provisions the llama-server sidecars locally
//!     (`install::install_inference`) and the runtime talks to loopback.
//!   · **Manual** — any other machine. The user runs their own
//!     OpenAI-style embedding endpoint (plus an optional reranker); the
//!     installer asks for the URLs, probes them, and pins a model
//!     fingerprint so the runtime can detect a silently-swapped model at
//!     boot (see `virtues-core/src/search/embedder.rs`).
//!
//! There is deliberately no general "managed" mode for arbitrary hardware:
//! provisioning sidecars on machines we can't test produces more broken
//! boxes than it saves keystrokes. Either it's our board and inference is
//! built in, or the user owns the endpoint and we validate it.

use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use sha2::{Digest, Sha256};

use crate::ui;

pub enum InferenceMode {
    /// Our board, detected automatically; sidecars provisioned locally.
    Dragon,
    /// User-run endpoints on any other machine. `embed_model` is the model
    /// name sent in every `/v1/embeddings` request body — llama.cpp ignores
    /// it, but Ollama (the most common BYO server) 404s unless it names a
    /// model that's actually pulled, so it must be configurable and must be
    /// recorded for the runtime to reuse (fingerprint probes have to be
    /// byte-identical between setup and boot).
    Manual { embed_url: String, embed_model: String, rerank_url: Option<String> },
}

impl InferenceMode {
    /// Decide the mode, in order: env override (image builds / CI, no TTY
    /// needed) → Dragon board detection → interactive manual prompts.
    pub fn resolve() -> Result<Self> {
        // 1. Env override — the non-interactive path for image builds and
        //    scripted installs.
        match std::env::var("VIRTUES_INFERENCE").ok().as_deref() {
            Some("dragon") => {
                ui::ok("Inference mode: dragon (VIRTUES_INFERENCE=dragon)");
                return Ok(Self::Dragon);
            }
            Some("manual") => {
                let embed_url = std::env::var("VIRTUES_EMBED_URL")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        anyhow!(
                            "VIRTUES_INFERENCE=manual requires VIRTUES_EMBED_URL \
                             (your OpenAI-style /v1/embeddings endpoint, e.g. \
                             http://localhost:11434). Optionally also set \
                             VIRTUES_RERANK_URL."
                        )
                    })?;
                let embed_url = normalize_url(&embed_url)
                    .context("VIRTUES_EMBED_URL is not a valid http(s) URL")?;
                let rerank_url = std::env::var("VIRTUES_RERANK_URL")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .map(|u| normalize_url(&u).context("VIRTUES_RERANK_URL is not a valid http(s) URL"))
                    .transpose()?;
                let embed_model = embed_model_from_env();
                ui::ok(&format!("Inference mode: manual (embed: {embed_url})"));
                return Ok(Self::Manual { embed_url, embed_model, rerank_url });
            }
            Some(other) => bail!(
                "unrecognized VIRTUES_INFERENCE={other} — expected \"dragon\" or \"manual\""
            ),
            None => {}
        }

        // 2. Board detection — our hardware means inference is built in,
        //    no questions asked.
        if is_dragon() {
            ui::ok("Dragon detected — inference is built in");
            return Ok(Self::Dragon);
        }

        // 3. Interactive manual flow. cliclack reads /dev/tty directly, so
        //    this works under `curl | sh` (stdin = pipe) — but not with no
        //    controlling terminal at all (CI, systemd), where the env
        //    override is the only path.
        if !has_tty() {
            bail!(
                "not our hardware and no terminal to ask on — set \
                 VIRTUES_INFERENCE=manual and VIRTUES_EMBED_URL=<your /v1/embeddings \
                 endpoint> (optionally VIRTUES_RERANK_URL) and re-run, or run the \
                 installer from an interactive terminal"
            );
        }
        print_recipes();

        let embed_url: String = cliclack::input("Embedding endpoint URL")
            .placeholder("http://localhost:11434")
            .validate(|s: &String| match normalize_url(s) {
                Ok(_) => Ok(()),
                Err(e) => Err(e.to_string()),
            })
            .interact()
            .context("reading embedding endpoint URL")?;
        let embed_url = normalize_url(&embed_url)?;

        let rerank_raw: String = cliclack::input(
            "Rerank endpoint URL (Enter to skip — search still works without one, slightly lower precision)",
        )
        .required(false)
        .validate(|s: &String| {
            if s.trim().is_empty() {
                Ok(())
            } else {
                match normalize_url(s) {
                    Ok(_) => Ok(()),
                    Err(e) => Err(e.to_string()),
                }
            }
        })
        .interact()
        .context("reading rerank endpoint URL")?;
        let rerank_url = if rerank_raw.trim().is_empty() {
            None
        } else {
            Some(normalize_url(&rerank_raw)?)
        };

        // Model name: llama.cpp serves whatever it loaded regardless, but
        // Ollama routes by this field and 404s on names it doesn't know.
        let model_raw: String = cliclack::input(
            "Model name your server expects (required for Ollama, e.g. \"embeddinggemma\" — Enter to skip for llama.cpp)",
        )
        .required(false)
        .interact()
        .context("reading embedding model name")?;
        let embed_model = if model_raw.trim().is_empty() {
            "default".to_string()
        } else {
            model_raw.trim().to_string()
        };

        Ok(Self::Manual { embed_url, embed_model, rerank_url })
    }
}

/// `VIRTUES_EMBED_MODEL`, defaulting to `"default"` (ignored by llama.cpp;
/// meaningful for Ollama-style servers that route requests by model name).
fn embed_model_from_env() -> String {
    std::env::var("VIRTUES_EMBED_MODEL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "default".to_string())
}

/// Are we on our own board? The Dragon device tree carries a
/// "radxa,dragon…" compatible string (covers the q6a/q8b variants).
/// `VIRTUES_BOARD=dragon` is the image-build escape hatch — the image is
/// assembled off-board, where there is no device tree to read.
fn is_dragon() -> bool {
    if std::env::var("VIRTUES_BOARD").as_deref() == Ok("dragon") {
        return true;
    }
    // /proc/device-tree/compatible is a NUL-separated list of byte strings
    // ("radxa,dragon-q6a\0qcom,…"); lossy UTF-8 is fine for a substring match.
    for path in ["/proc/device-tree/compatible", "/proc/device-tree/model"] {
        if let Ok(bytes) = std::fs::read(path) {
            let text = String::from_utf8_lossy(&bytes).to_lowercase();
            if text.contains("radxa,dragon") || text.contains("radxa dragon") {
                return true;
            }
        }
    }
    false
}

/// Can we prompt? cliclack reads /dev/tty (not stdin), so this — not
/// `isatty(stdin)` — is the right existence check under `curl | sh`.
fn has_tty() -> bool {
    std::fs::File::open("/dev/tty").is_ok()
}

/// The short "what do I point this at" block shown before the URL prompts.
fn print_recipes() {
    use console::style;
    println!();
    println!("  Virtues needs an embedding endpoint you run (OpenAI-style /v1/embeddings).");
    println!("  A reranker (/v1/rerank) is optional.");
    println!();
    println!("  {}", style("Quick recipes:").bold());
    println!("    Ollama:     ollama pull embeddinggemma  →  http://localhost:11434");
    println!("    llama.cpp:  llama-server --embedding -m embeddinggemma-300m-qat-Q8_0.gguf --port 18181");
    println!("  {}", style("Docs: https://virtues.com/docs/inference").dim());
    println!();
}

/// Minimal http(s) URL validation without pulling in the `url` crate —
/// scheme + non-empty host is all we need before the real probe talks to it.
fn normalize_url(raw: &str) -> Result<String> {
    let s = raw.trim().trim_end_matches('/');
    let rest = s
        .strip_prefix("http://")
        .or_else(|| s.strip_prefix("https://"))
        .ok_or_else(|| anyhow!("URL must start with http:// or https://"))?;
    if rest.is_empty() || rest.starts_with('/') || rest.starts_with(':') {
        bail!("URL has no host");
    }
    Ok(s.to_string())
}

// ────────────────────────────────────────────────────────────────────────
// Manual-endpoint validation
// ────────────────────────────────────────────────────────────────────────

/// The two fixed probe strings + the quantize-and-hash fingerprint below
/// MUST match `virtues-core/src/search/embedder.rs` exactly — the installer
/// pins the fingerprint at setup time and the core recomputes it at boot to
/// detect a swapped model. Duplicated (the installer doesn't depend on the
/// core crate); keep both copies in lockstep.
const PROBES: [&str; 2] = ["virtues fingerprint probe 0", "virtues fingerprint probe 1"];

/// SHA256 over the probe vectors with each component quantized to
/// `(x * 10000).round() as i32` (LE bytes). The quantization makes the hash
/// stable across float formatting / minor backend jitter while still
/// changing on any real model swap. Must match
/// `virtues-core/src/search/embedder.rs::fingerprint_vectors`.
fn fingerprint_vectors(vectors: &[Vec<f32>]) -> String {
    let mut h = Sha256::new();
    for v in vectors {
        for &x in v {
            let q = (x as f64 * 10000.0).round() as i32;
            h.update(q.to_le_bytes());
        }
    }
    hex::encode(h.finalize())
}

/// What the probe learned about the user's endpoints — written into the env
/// file so the runtime can re-check the fingerprint at every boot.
pub struct ValidationReport {
    pub dims: usize,
    pub fingerprint: String,
    /// Recorded for the verdict printed by `validate_manual`; only dims +
    /// fingerprint end up in the env file today.
    #[allow(dead_code)]
    pub p50_ms: u128,
    #[allow(dead_code)]
    pub rerank_ok: bool,
}

/// Probe the user's endpoints: shape check + dims + latency + fingerprint
/// on the embedder (fatal if broken — search can't exist without it), and a
/// best-effort probe of the reranker (absent/broken → warn, search still
/// works on bi-encoder ranking alone).
pub async fn validate_manual(
    embed_url: &str,
    embed_model: &str,
    rerank_url: Option<&str>,
) -> Result<ValidationReport> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    // Both probes in one call — this is the fingerprint input.
    let vecs = embed(&client, embed_url, embed_model, &PROBES)
        .await
        .with_context(|| format!("probing embedding endpoint {embed_url}"))?;
    if vecs.len() != 2 {
        bail!(
            "embedding endpoint returned {} vectors for 2 inputs — not an \
             OpenAI-style /v1/embeddings endpoint?",
            vecs.len()
        );
    }
    let dims = vecs[0].len();
    if dims == 0 || vecs[1].len() != dims {
        bail!("embedding endpoint returned inconsistent/empty vectors ({} and {} dims)", dims, vecs[1].len());
    }
    let fingerprint = fingerprint_vectors(&vecs);

    // p50 over 5 single-input calls — enough to smooth out a cold first
    // request without turning validation into a benchmark.
    let mut lat_ms: Vec<u128> = Vec::with_capacity(5);
    for _ in 0..5 {
        let t = Instant::now();
        embed(&client, embed_url, embed_model, &PROBES[..1])
            .await
            .with_context(|| format!("latency probe against {embed_url}"))?;
        lat_ms.push(t.elapsed().as_millis());
    }
    lat_ms.sort_unstable();
    let p50_ms = lat_ms[2];

    // Reranker probe — never fatal: a broken reranker degrades precision,
    // it doesn't break search.
    let rerank_ok = match rerank_url {
        None => false,
        Some(url) => match probe_rerank(&client, url).await {
            Ok(()) => true,
            Err(e) => {
                ui::warn(&format!(
                    "rerank endpoint {url} failed validation ({e:#}) — continuing without \
                     a reranker (search still works, slightly lower precision)"
                ));
                false
            }
        },
    };

    // Verdict.
    ui::ok(&format!("Embedding endpoint OK — {dims}-dim vectors"));
    if p50_ms < 100 {
        ui::ok(&format!("embeds at {p50_ms} ms — searches will feel instant"));
    } else if p50_ms <= 400 {
        ui::warn(&format!("embeds at {p50_ms} ms — searches will feel a bit slow"));
    } else {
        ui::warn(&format!(
            "embeds at {p50_ms} ms — that's slow; every search waits on this endpoint. \
             Consider running the model on faster hardware or closer to the box."
        ));
    }
    if rerank_ok {
        ui::ok("Rerank endpoint OK");
    } else {
        ui::skip("No reranker — search still works without one, slightly lower precision");
    }

    Ok(ValidationReport { dims, p50_ms, fingerprint, rerank_ok })
}

/// POST `{base}/v1/embeddings`, returning vectors in input order. Accepts
/// both OpenAI-style response shapes: `{"data": [{"embedding": […]}, …]}`
/// (rows may carry an `index`) and a bare top-level row array.
async fn embed(
    client: &reqwest::Client,
    base: &str,
    model: &str,
    input: &[&str],
) -> Result<Vec<Vec<f32>>> {
    let url = format!("{base}/v1/embeddings");
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "input": input, "model": model }))
        .send()
        .await
        .with_context(|| format!("POST {url}"))?
        .error_for_status()
        .with_context(|| format!("POST {url}"))?;
    let body: serde_json::Value = resp.json().await.context("parsing /v1/embeddings response")?;

    let rows = body
        .get("data")
        .and_then(|d| d.as_array())
        .or_else(|| body.as_array())
        .ok_or_else(|| {
            anyhow!("response has no `data` array — not an OpenAI-style /v1/embeddings endpoint?")
        })?;

    // Index-tagged rows may arrive out of order; sort when the tag exists.
    let mut rows: Vec<&serde_json::Value> = rows.iter().collect();
    rows.sort_by_key(|r| r.get("index").and_then(|i| i.as_u64()).unwrap_or(0));

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let emb = row
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| anyhow!("embedding row missing an `embedding` array"))?;
        let v: Result<Vec<f32>> = emb
            .iter()
            .map(|x| {
                x.as_f64()
                    .map(|f| f as f32)
                    .ok_or_else(|| anyhow!("non-numeric value in embedding vector"))
            })
            .collect();
        out.push(v?);
    }
    Ok(out)
}

/// POST `{base}/v1/rerank` with a two-document probe; accepts
/// `results[].relevance_score` (Jina/llama-server) or `results[].score`
/// (Cohere-style).
async fn probe_rerank(client: &reqwest::Client, base: &str) -> Result<()> {
    let url = format!("{base}/v1/rerank");
    let resp = client
        .post(&url)
        .json(&serde_json::json!({
            "model": "default",
            "query": "probe",
            "documents": ["a", "b"],
        }))
        .send()
        .await
        .with_context(|| format!("POST {url}"))?
        .error_for_status()
        .with_context(|| format!("POST {url}"))?;
    let body: serde_json::Value = resp.json().await.context("parsing /v1/rerank response")?;
    let results = body
        .get("results")
        .and_then(|r| r.as_array())
        .ok_or_else(|| anyhow!("response has no `results` array"))?;
    if results.is_empty() {
        bail!("rerank returned no results for a 2-document probe");
    }
    for r in results {
        let score = r
            .get("relevance_score")
            .or_else(|| r.get("score"))
            .and_then(|s| s.as_f64());
        if score.is_none() {
            bail!("rerank result rows carry neither `relevance_score` nor `score`");
        }
    }
    Ok(())
}
