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
    Manual {
        embed_url: String,
        embed_model: String,
        rerank_url: Option<String>,
        /// Optional HuggingFace repo id for the embedding model (e.g.
        /// `google/embeddinggemma-300m`). Used at validation time to pull the
        /// model's official query/document prompt prefixes from its
        /// `config_sentence_transformers.json`. `None` → fall back to a
        /// known-family table, then no prefix.
        hf_repo: Option<String>,
    },
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
                ensure_local(&embed_url, "Embedding endpoint")?;
                if let Some(u) = &rerank_url {
                    ensure_local(u, "Rerank endpoint")?;
                }
                let embed_model = embed_model_from_env();
                let hf_repo = std::env::var("VIRTUES_EMBED_HF_REPO")
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().to_string());
                ui::ok(&format!("Inference mode: manual (embed: {embed_url})"));
                return Ok(Self::Manual { embed_url, embed_model, rerank_url, hf_repo });
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
        ensure_local(&embed_url, "Embedding endpoint")?;

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
            let u = normalize_url(&rerank_raw)?;
            ensure_local(&u, "Rerank endpoint")?;
            Some(u)
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

        // Optional HF repo id → we pull the model's official query/doc prompt
        // prefixes from its config_sentence_transformers.json at validation time
        // (better recall for models that want asymmetric prompts, e.g. e5/bge).
        let hf_repo_raw: String = cliclack::input(
            "HuggingFace repo for your model (optional, improves search quality — e.g. google/embeddinggemma-300m; Enter to skip)",
        )
        .required(false)
        .interact()
        .context("reading HuggingFace repo id")?;
        let hf_repo = if hf_repo_raw.trim().is_empty() {
            None
        } else {
            Some(hf_repo_raw.trim().to_string())
        };

        Ok(Self::Manual { embed_url, embed_model, rerank_url, hf_repo })
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

/// The "what do I point this at" block shown before the URL prompts. Two
/// recipes per endpoint, all speaking the contracts we pin: embeddings is the
/// universal OpenAI `/v1/embeddings` (llama.cpp or Ollama both work); rerank is
/// pinned to llama.cpp's `/v1/rerank` shape (our own Dragon sidecar), shown in
/// GPU and CPU flavors. Cloud APIs are intentionally absent — see `ensure_local`.
fn print_recipes() {
    use console::style;
    println!();
    println!("  Virtues runs inference on a service YOU host — this box, a machine on your");
    println!("  LAN, or one over your VPN. It needs an OpenAI-style /v1/embeddings endpoint;");
    println!("  a /v1/rerank endpoint is optional (search still works without one).");
    println!("  Cloud APIs (OpenAI, Cohere, …) are not supported — your data stays yours.");
    println!();
    println!("  {}", style("Embedding endpoint — pick one:").bold());
    println!("    llama.cpp:  llama-server --embeddings --pooling mean \\");
    println!("                  -m embeddinggemma-300m-qat-Q8_0.gguf --port 18181");
    println!("                → URL http://localhost:18181   (model name: any)");
    println!("    Ollama:     ollama pull embeddinggemma");
    println!("                → URL http://localhost:11434   (model name: embeddinggemma)");
    println!();
    println!("  {}", style("Rerank endpoint (optional) — pick one:").bold());
    println!("    llama.cpp (GPU):  llama-server --reranking --pooling rank -ngl 99 \\");
    println!("                        -m gte-reranker-modernbert-base-Q8_0.gguf --port 18182");
    println!("    llama.cpp (CPU):  llama-server --reranking --pooling rank \\");
    println!("                        -m gte-reranker-modernbert-base-Q8_0.gguf --port 18182");
    println!("                      → URL http://localhost:18182");
    println!();
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

/// The host portion of a normalized `http(s)://host[:port][/path]` URL, with any
/// port and IPv6 brackets stripped. Best-effort string surgery (we avoid the
/// `url` crate) — enough to hand to `parse::<IpAddr>()` or a DNS lookup.
fn host_of(normalized_url: &str) -> String {
    let after_scheme = normalized_url
        .strip_prefix("http://")
        .or_else(|| normalized_url.strip_prefix("https://"))
        .unwrap_or(normalized_url);
    let authority = after_scheme.split('/').next().unwrap_or(after_scheme);
    // IPv6 literal: [::1]:8080 → ::1
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest).to_string();
    }
    // host[:port] → host
    authority.split(':').next().unwrap_or(authority).to_string()
}

/// Is this IP on the user's own machine, LAN, or VPN — i.e. traffic to it never
/// leaves their network? Loopback, RFC1918 private, link-local, CGNAT/100.64
/// (Tailscale et al.), and IPv6 unique-local (fc00::/7, incl. Tailscale's
/// fd7a::/8) all count. A global address (the box's own public IPv6, a cloud
/// API) does not. Classified by hand to avoid std's unstable `ip` feature.
fn is_local_ip(ip: std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()        // 127.0.0.0/8
                || v4.is_private()  // 10/8, 172.16/12, 192.168/16
                || v4.is_link_local() // 169.254.0.0/16
                || {                // 100.64.0.0/10 (carrier-grade NAT / Tailscale)
                    let o = v4.octets();
                    o[0] == 100 && (o[1] & 0xc0) == 0x40
                }
        }
        IpAddr::V6(v6) => {
            let head = v6.segments()[0];
            v6.is_loopback()               // ::1
                || (head & 0xffc0) == 0xfe80 // fe80::/10 link-local
                || (head & 0xfe00) == 0xfc00 // fc00::/7 unique-local
        }
    }
}

fn public_endpoint_msg(label: &str, host: &str) -> String {
    format!(
        "{label} ({host}) looks like a public address. Virtues runs inference on a \
         service you host — this box, a machine on your LAN, or one over your VPN — so \
         your data never leaves your network. Cloud embedding APIs (OpenAI, Cohere, …) \
         are deliberately not supported. Point this at a local endpoint (see the recipes \
         above). Expert override (traffic may leave your network): \
         VIRTUES_ALLOW_REMOTE_INFERENCE=1."
    )
}

/// Refuse an inference endpoint that isn't on the user's own machine/LAN/VPN.
/// This is the enforcement behind "no cloud APIs": a public host (or a name that
/// resolves to one) is rejected. `VIRTUES_ALLOW_REMOTE_INFERENCE=1` is the
/// logged escape hatch for the expert running their own model off-network.
fn ensure_local(url: &str, label: &str) -> Result<()> {
    use std::net::ToSocketAddrs;

    if std::env::var("VIRTUES_ALLOW_REMOTE_INFERENCE").as_deref() == Ok("1") {
        ui::warn(&format!(
            "{label} locality check bypassed (VIRTUES_ALLOW_REMOTE_INFERENCE=1) — \
             inference traffic may leave your network"
        ));
        return Ok(());
    }

    let host = host_of(url);
    // localhost + mDNS names are LAN by definition (and .local may not resolve
    // yet — avahi is installed after this check runs).
    if host.eq_ignore_ascii_case("localhost") || host.to_lowercase().ends_with(".local") {
        return Ok(());
    }
    // Literal IP → classify directly.
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if is_local_ip(ip) {
            return Ok(());
        }
        bail!(public_endpoint_msg(label, &host));
    }
    // Hostname → resolve; require every resolved address to be local. If it
    // can't be resolved right now (private DNS not reachable at install time),
    // don't hard-block a plausibly-local name — a cloud API always resolves.
    match (host.as_str(), 0u16).to_socket_addrs() {
        Ok(addrs) => {
            let ips: Vec<std::net::IpAddr> = addrs.map(|s| s.ip()).collect();
            if ips.is_empty() || ips.iter().all(|ip| is_local_ip(*ip)) {
                if ips.is_empty() {
                    ui::warn(&format!(
                        "{label}: couldn't resolve {host} to verify it's local — proceeding"
                    ));
                }
                Ok(())
            } else {
                bail!(public_endpoint_msg(label, &host))
            }
        }
        Err(_) => {
            ui::warn(&format!(
                "{label}: couldn't resolve {host} to verify it's local — proceeding"
            ));
            Ok(())
        }
    }
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
    /// Asymmetric prompt prefixes resolved for this model (HF card → known
    /// family → none). Empty string = no prefix. Written to the env as
    /// `VIRTUES_EMBED_QUERY_PROMPT` / `_DOC_PROMPT` and applied by the runtime
    /// embedder. Never affects the fingerprint (probes are embedded raw).
    pub query_prompt: String,
    pub doc_prompt: String,
    /// Recorded for the verdict printed by `validate_manual`; only dims +
    /// fingerprint + prompts end up in the env file today.
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
    hf_repo: Option<&str>,
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

    let (query_prompt, doc_prompt) = resolve_prompts(&client, embed_model, hf_repo).await;

    Ok(ValidationReport { dims, p50_ms, fingerprint, query_prompt, doc_prompt, rerank_ok })
}

/// Resolve the asymmetric (query, document) prompt prefixes for a manual model.
/// Ladder, most authoritative first:
///   1. explicit env (`VIRTUES_EMBED_QUERY_PROMPT` / `_DOC_PROMPT`) — power user
///   2. the model's own HuggingFace `config_sentence_transformers.json` (if a
///      repo id was given) — self-updating, we maintain nothing
///   3. a small known-family table keyed on the model name — prefix conventions
///      are sticky even as weights churn
///   4. none — a wrong prefix hurts more than a missing one, so default empty
async fn resolve_prompts(
    client: &reqwest::Client,
    embed_model: &str,
    hf_repo: Option<&str>,
) -> (String, String) {
    // 1. Explicit env wins (and honors an intentional empty = "no prefix").
    let env_q = std::env::var("VIRTUES_EMBED_QUERY_PROMPT").ok();
    let env_d = std::env::var("VIRTUES_EMBED_DOC_PROMPT").ok();
    if env_q.is_some() || env_d.is_some() {
        ui::ok("Prompt prefixes: from environment");
        return (env_q.unwrap_or_default(), env_d.unwrap_or_default());
    }

    // 2. Authoritative: the model's published sentence-transformers config.
    if let Some(repo) = hf_repo {
        match fetch_hf_prompts(client, repo).await {
            Ok(Some((q, d))) => {
                ui::ok(&format!("Prompt prefixes: from HuggingFace ({repo})"));
                return (q, d);
            }
            Ok(None) => ui::skip(&format!(
                "{repo} publishes no usable query/document prompts — trying model name"
            )),
            Err(e) => ui::warn(&format!("couldn't read prompts from {repo} ({e:#}) — trying model name")),
        }
    }

    // 3. Known-family table (conventions move far slower than model weights).
    if let Some((q, d)) = family_prompts(embed_model) {
        ui::ok(&format!("Prompt prefixes: recognized model family from \"{embed_model}\""));
        return (q, d);
    }

    // 4. None — safe default.
    ui::skip(
        "No prompt prefixes — search still works. If your model expects them, set \
         VIRTUES_EMBED_QUERY_PROMPT / VIRTUES_EMBED_DOC_PROMPT (see its HuggingFace card's \
         config_sentence_transformers.json).",
    );
    (String::new(), String::new())
}

/// Pull query/document prompt prefixes from a model's
/// `config_sentence_transformers.json` on HuggingFace. Returns `Ok(None)` when
/// the file has no `prompts` or we can't confidently map one to "query" and one
/// to "document" — the key names aren't standardized, so we match by substring.
async fn fetch_hf_prompts(
    client: &reqwest::Client,
    repo: &str,
) -> Result<Option<(String, String)>> {
    let repo = repo.trim().trim_matches('/');
    let url = format!("https://huggingface.co/{repo}/resolve/main/config_sentence_transformers.json");
    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;
    let body: serde_json::Value = resp.json().await.context("parsing config_sentence_transformers.json")?;
    let Some(prompts) = body.get("prompts").and_then(|p| p.as_object()) else {
        return Ok(None);
    };

    // Key names vary by model (query / Retrieval-query / search_query / …). Match
    // the query side and the document side by substring, most-specific first.
    let find = |needles: &[&str]| -> Option<String> {
        for (k, v) in prompts {
            let kl = k.to_lowercase();
            if needles.iter().any(|n| kl.contains(n)) {
                if let Some(s) = v.as_str() {
                    return Some(s.to_string());
                }
            }
        }
        None
    };
    let query = find(&["query", "search_query", "question"]);
    let doc = find(&["passage", "document", "corpus", "search_document", "text"]);

    match (query, doc) {
        (Some(q), Some(d)) => Ok(Some((q, d))),
        // A query-only convention (e.g. bge) is legitimate: prefix queries, not docs.
        (Some(q), None) => Ok(Some((q, String::new()))),
        _ => Ok(None),
    }
}

/// Known-family prompt conventions, keyed on a substring of the model name.
/// Deliberately tiny: these conventions are stable across model versions, so
/// this is a near-zero-maintenance fallback for when no HF repo id was given.
fn family_prompts(model: &str) -> Option<(String, String)> {
    let m = model.to_lowercase();
    let pair = |q: &str, d: &str| Some((q.to_string(), d.to_string()));
    if m.contains("embeddinggemma") || m.contains("embedding-gemma") {
        pair("task: search result | query: ", "title: none | text: ")
    } else if m.contains("e5") {
        pair("query: ", "passage: ")
    } else if m.contains("nomic") {
        pair("search_query: ", "search_document: ")
    } else if m.contains("bge") {
        pair("Represent this sentence for searching relevant passages: ", "")
    } else if m.contains("gte") {
        // gte models use no prefix — recognized so we don't fall through to the
        // "unknown, set it yourself" nudge.
        pair("", "")
    } else {
        None
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn local_ips_are_allowed() {
        for s in [
            "127.0.0.1",     // loopback
            "10.0.0.5",      // RFC1918
            "172.16.3.4",    // RFC1918
            "192.168.1.233", // RFC1918
            "169.254.1.1",   // link-local
            "100.104.55.76", // CGNAT / Tailscale
            "::1",           // v6 loopback
            "fe80::1",       // v6 link-local
            "fd7a:115c:a1e0::1", // v6 unique-local (Tailscale)
        ] {
            assert!(is_local_ip(ip(s)), "{s} should be local");
        }
    }

    #[test]
    fn public_ips_are_rejected() {
        for s in [
            "1.1.1.1",
            "104.18.0.1",                       // cloud
            "2603:8080:1500:1d00::1",           // the box's own global IPv6 class
            "2606:4700::1",                      // global v6
        ] {
            assert!(!is_local_ip(ip(s)), "{s} should be public");
        }
    }

    #[test]
    fn host_extraction() {
        assert_eq!(host_of("http://localhost:11434"), "localhost");
        assert_eq!(host_of("http://192.168.1.5:18181/v1"), "192.168.1.5");
        assert_eq!(host_of("https://api.openai.com"), "api.openai.com");
        assert_eq!(host_of("http://[::1]:8080"), "::1");
        assert_eq!(host_of("http://[fd7a:115c::1]:18181/x"), "fd7a:115c::1");
    }
}
