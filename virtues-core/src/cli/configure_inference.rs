//! `virtues configure-inference` — move search to another embedding server
//! (`--embed-url`, see [`switch`]), or re-validate the endpoint after a model
//! change and recover the index.
//!
//! The boot guard (`search::embedder`) refuses to serve when the endpoint's
//! model fingerprint no longer matches the one the index was built with — the
//! runtime errors point here. This command is the exit: it re-probes the current
//! endpoint (bypassing that guard), reports what changed, and — on confirmation
//! — re-embeds. Re-embedding re-pins the new fingerprint + dims, then rebuilds
//! exactly as `virtues reindex` does (`reindex::rebuild`): wipes the DERIVED
//! index (never source data), re-embeds from source with the new model, and
//! rescores every day's events. A restart then puts the new model behind search.
//!
//! Handled in `main.rs` (not `cli::run`) so it runs before the app builds the
//! guarded embedder — which would itself fail on the very mismatch we're here to
//! fix.

use crate::error::{Error, Result};

pub async fn run(reembed: bool, yes: bool) -> Result<()> {
    let database_url = crate::database::normalize_database_url()?;

    let stored_fp = std::env::var("VIRTUES_EMBED_FINGERPRINT")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let Some(stored_fp) = stored_fp else {
        println!("Search runs on the server Virtues installed, so there's no model change to recover.");
        println!("To move search to your own server (e.g. llama.cpp on your GPU):");
        println!("    virtues configure-inference --embed-url http://127.0.0.1:8080");
        return Ok(());
    };
    // The width the index was actually BUILT at, read from the database — not a
    // constant, not the env. The index is the thing that remembers. `None` means
    // it has never been built, so there is no width to disagree with.
    let db = crate::database::Database::new(&database_url)?;
    db.connect().await?;
    let stored_dim = crate::search::embedder::index_dim(db.pool())
        .await
        .map_err(|e| Error::Database(format!("{e:#}")))?;
    let dim_label = stored_dim.map(|d| d.to_string()).unwrap_or_else(|| "—".into());

    println!("→ probing the configured embedding endpoint…");
    let (new_fp, new_dim) = crate::search::embedder::probe_current_endpoint()
        .await
        .map_err(|e| Error::Other(format!("probe failed: {e}")))?;

    if new_fp.eq_ignore_ascii_case(&stored_fp) {
        println!("✓ The endpoint serves the same model your index was built with.");
        println!("  Fingerprint {}… · {dim_label} dims. Nothing to do.", short(&new_fp));
        return Ok(());
    }

    println!();
    println!("⚠  The model behind your embedding endpoint has changed:");
    println!("     fingerprint  {}…  →  {}…", short(&stored_fp), short(&new_fp));
    if Some(new_dim) != stored_dim {
        println!("     dimensions   {dim_label}  →  {new_dim}");
    }
    println!();
    println!("   Embeddings are a derived cache — your source data is safe. Recovering");
    println!("   means wiping the vector index and re-embedding from source with the new");
    println!("   model. (Prompt prefixes aren't changed; if the new model needs different");
    println!("   ones, re-run the installer or set VIRTUES_EMBED_QUERY_PROMPT / _DOC_PROMPT.)");
    if let Some(line) = super::reindex::estimate(db.pool()).await? {
        println!("   {line}");
    }
    println!();

    if !reembed && !yes {
        let ok = dialoguer::Confirm::new()
            .with_prompt("Wipe the vector index and re-embed from source now?")
            .default(false)
            .interact()
            .unwrap_or(false);
        if !ok {
            println!("Aborted — nothing changed. Search stays offline until the endpoint's model");
            println!("matches the index again, or you re-embed.");
            return Ok(());
        }
    }

    // 1. Re-pin the new fingerprint + dims: in the env file for the next boot, and
    //    in this process so the re-embed below runs exactly as the next boot will.
    //    The embedder reads both at construction and refuses a model whose
    //    fingerprint is not the pinned one. Pinned before the wipe, so a failed
    //    write leaves the index untouched.
    println!("→ pinning the new model fingerprint…");
    pin(
        &[
            ("VIRTUES_EMBED_FINGERPRINT", new_fp.clone()),
            ("VIRTUES_EMBED_DIMS", new_dim.to_string()),
        ],
        &[],
    )?;

    // 2. Wipe, re-embed with the new model, and rescore every day's events.
    let (embedded, days, scored) = super::reindex::rebuild(db.pool()).await?;

    println!();
    println!(
        "✓ Re-configured — {embedded} records embedded with the new model, {scored} events \
         rescored across {days} days."
    );
    println!("Restart the box so the server serves search with the new model:");
    println!("    sudo systemctl restart virtues");
    Ok(())
}

/// Two probe vectors at or above this cosine are the same model. A model served
/// by another build or backend (CPU → CUDA, Q8 → BF16) lands above 0.999; two
/// different models land far below.
const SAME_MODEL_COSINE: f32 = 0.995;

/// `virtues configure-inference --embed-url URL`: move search to another
/// embedding server — typically llama.cpp on the owner's GPU, after the
/// installer's accelerator guide. The index is kept when the new server runs
/// the same model (compared by probe-vector cosine, because a different build
/// changes the exact fingerprint but not the geometry) or when nothing has been
/// indexed yet; otherwise the derived index is rebuilt from source, after
/// confirmation.
pub async fn switch(
    embed_url: &str,
    rerank_url: Option<&str>,
    embed_model: Option<&str>,
    yes: bool,
) -> Result<()> {
    let embed_url = normalize_url(embed_url)?;
    ensure_local(&embed_url, "embedding server").await?;
    let rerank_url = match rerank_url {
        Some(u) => {
            let u = normalize_url(u)?;
            ensure_local(&u, "rerank server").await?;
            Some(u)
        }
        None => None,
    };
    let model = embed_model
        .map(str::to_string)
        .or_else(|| std::env::var("VIRTUES_EMBED_MODEL").ok().filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| "default".to_string());

    let database_url = crate::database::normalize_database_url()?;
    let db = crate::database::Database::new(&database_url)?;
    db.connect().await?;
    let stored_dim = crate::search::embedder::index_dim(db.pool())
        .await
        .map_err(|e| Error::Database(format!("{e:#}")))?;

    println!("→ probing {embed_url}…");
    let new = crate::search::embedder::probe_vectors(&embed_url, &model)
        .await
        .map_err(|e| Error::Other(format!("{e:#}")))?;
    // The server search uses today. Unreachable is fine: it only means we
    // can't prove the new server runs the same model.
    let old_model = std::env::var("VIRTUES_EMBED_MODEL").unwrap_or_else(|_| "default".to_string());
    let old = crate::search::embedder::probe_vectors(
        &crate::search::embedder::resolve_base_url(),
        &old_model,
    )
    .await
    .ok();
    let similarity = old.as_ref().and_then(|o| min_cosine(o, &new));
    let same_model = similarity.is_some_and(|c| c >= SAME_MODEL_COSINE);
    println!("  {} dims", new[0].len());
    match similarity {
        Some(c) if same_model => println!("✓ Same model as the current server (probe similarity {c:.4})."),
        Some(c) => {
            println!("  A different model from the current server (probe similarity {c:.4}).");
            println!("  If both servers load the same model file, the new one is computing it");
            println!("  wrong - some GPU backends overflow EmbeddingGemma's fp16 math. Stop here");
            println!("  and keep the current server.");
        }
        None => println!("  Couldn't compare with the current server, so treating it as a new model."),
    }

    let rebuild = stored_dim.is_some() && !same_model;
    if rebuild {
        println!();
        println!("   Search has to re-embed your record with the new model. Embeddings are a");
        println!("   derived cache - your source data is safe.");
        if let Some(line) = super::reindex::estimate(db.pool()).await? {
            println!("   {line}");
        }
        if !yes {
            let ok = dialoguer::Confirm::new()
                .with_prompt("Switch servers, then wipe the vector index and re-embed now?")
                .default(false)
                .interact()
                .unwrap_or(false);
            if !ok {
                println!("Aborted - nothing changed.");
                return Ok(());
            }
        }
    }

    // Pin the new server for the next boot and for this process (the rebuild
    // below embeds through it). The fingerprint is over NATIVE vectors, as the
    // boot guard computes it. A same-model switch keeps the stored width
    // (VIRTUES_EMBED_DIMS); a new model is stored at its own native width.
    let was_bundled = std::env::var("VIRTUES_INFERENCE").as_deref() == Ok("bundled");
    let fp = crate::search::embedder::fingerprint_vectors(&new);
    let mut set = vec![
        ("VIRTUES_INFERENCE", "manual".to_string()),
        ("VIRTUES_EMBED_URL", embed_url.clone()),
        ("VIRTUES_EMBED_MODEL", model.clone()),
        ("VIRTUES_EMBED_FINGERPRINT", fp),
    ];
    if let Some(u) = &rerank_url {
        set.push(("VIRTUES_RERANK_URL", u.clone()));
    }
    let unset: &[&str] = if same_model { &[] } else { &["VIRTUES_EMBED_DIMS"] };
    pin(&set, unset)?;

    if rebuild {
        let (embedded, days, scored) = super::reindex::rebuild(db.pool()).await?;
        println!(
            "✓ {embedded} records embedded with the new model, {scored} events rescored across {days} days."
        );
    } else {
        println!("✓ Switched. The index is kept.");
    }
    println!("Restart the box so search uses {embed_url}:");
    println!("    sudo systemctl restart virtues");
    if was_bundled {
        println!("The CPU engine Virtues installed is no longer used. To free its memory:");
        println!("    sudo systemctl disable --now virtues-embed");
    }
    Ok(())
}

/// Smallest pairwise cosine between two probe sets; `None` when the shapes
/// differ (different widths are different models).
fn min_cosine(a: &[Vec<f32>], b: &[Vec<f32>]) -> Option<f32> {
    if a.len() != b.len() || a.iter().zip(b).any(|(x, y)| x.len() != y.len()) {
        return None;
    }
    let cos = |x: &[f32], y: &[f32]| {
        let dot: f32 = x.iter().zip(y).map(|(p, q)| p * q).sum();
        let n = |v: &[f32]| v.iter().map(|p| p * p).sum::<f32>().sqrt();
        dot / (n(x) * n(y)).max(f32::EPSILON)
    };
    a.iter().zip(b).map(|(x, y)| cos(x, y)).reduce(f32::min)
}

fn normalize_url(raw: &str) -> Result<String> {
    let u = raw.trim().trim_end_matches('/').to_string();
    if !(u.starts_with("http://") || u.starts_with("https://")) {
        return Err(Error::Other(format!("{raw:?} is not an http(s) URL, e.g. http://127.0.0.1:8080")));
    }
    Ok(u)
}

/// Inference servers must be on this machine, the LAN, or a VPN — the same rule
/// and override as the installer's `mode::ensure_local` (two copies: the
/// installer cannot depend on this crate; change one, change both).
async fn ensure_local(url: &str, label: &str) -> Result<()> {
    if std::env::var("VIRTUES_ALLOW_REMOTE_INFERENCE").as_deref() == Ok("1") {
        tracing::warn!("VIRTUES_ALLOW_REMOTE_INFERENCE=1: not checking that {url} is local");
        return Ok(());
    }
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let authority = rest.split('/').next().unwrap_or(rest);
    let lookup = if authority.starts_with('[') || authority.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok()) {
        authority.to_string()
    } else {
        format!("{authority}:80")
    };
    let addrs: Vec<std::net::IpAddr> = tokio::net::lookup_host(&lookup)
        .await
        .map_err(|e| Error::Other(format!("can't resolve the {label} {url}: {e}")))?
        .map(|s| s.ip())
        .collect();
    let local = |ip: &std::net::IpAddr| match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback() || v4.is_private() || v4.is_link_local() || (o[0] == 100 && (64..128).contains(&o[1]))
        }
        std::net::IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00,
    };
    if addrs.is_empty() || !addrs.iter().all(local) {
        return Err(Error::Other(format!(
            "the {label} {url} isn't on this machine, your LAN, or your VPN. Inference runs \
             next to your data; set VIRTUES_ALLOW_REMOTE_INFERENCE=1 to override."
        )));
    }
    Ok(())
}

/// Write settings to the box env file for the next boot, and to this process
/// so what runs next here (a rebuild) sees them too. On a dev machine (no env
/// file) only this process changes.
fn pin(set: &[(&str, String)], unset: &[&str]) -> Result<()> {
    let path = crate::box_env::path();
    if !crate::box_env::edit(&path, set, unset)? {
        println!("  (no {} — set these in your environment for the next run)", path.display());
    }
    for (k, v) in set {
        std::env::set_var(k, v);
    }
    for k in unset {
        std::env::remove_var(k);
    }
    Ok(())
}

fn short(fp: &str) -> &str {
    &fp[..fp.len().min(12)]
}
