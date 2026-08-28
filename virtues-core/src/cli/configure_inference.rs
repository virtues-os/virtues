//! `virtues configure-inference` — re-validate the embedding endpoint after a
//! model change and recover the index.
//!
//! The boot guard (`search::embedder`) refuses to serve when the endpoint's
//! model fingerprint no longer matches the one the index was built with — the
//! runtime errors point here. This command is the exit: it re-probes the current
//! endpoint (bypassing that guard), reports what changed, and — on confirmation
//! — re-embeds. Re-embedding wipes the DERIVED vector index (never source data),
//! re-pins the new fingerprint + dims, and resizes the vector columns, so the
//! background indexer rebuilds from source with the new model.
//!
//! Handled in `main.rs` (not `cli::run`) so it runs before the app builds the
//! guarded embedder — which would itself fail on the very mismatch we're here to
//! fix.

use sqlx::PgPool;

use crate::error::{Error, Result};

const ENV_FILE: &str = "/var/lib/virtues/virtues.env";

pub async fn run(reembed: bool, yes: bool) -> Result<()> {
    let database_url = crate::database::normalize_database_url()?;

    let stored_fp = std::env::var("VIRTUES_EMBED_FINGERPRINT")
        .ok()
        .filter(|s| !s.trim().is_empty());
    let Some(stored_fp) = stored_fp else {
        println!("This box uses managed (Dragon) inference — there's nothing to configure.");
        println!("`configure-inference` is for manual endpoints (VIRTUES_INFERENCE=manual).");
        return Ok(());
    };
    // The width the index was actually BUILT at, read from the database — not a
    // constant, not the env. The index is the thing that remembers. `None` means
    // it has never been built, so there is no width to disagree with.
    let pool = PgPool::connect(&database_url)
        .await
        .map_err(|e| Error::Database(format!("connecting: {e}")))?;
    let stored_dim = crate::search::embedder::index_dim(&pool).await;
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

    let db = crate::database::Database::new(&database_url)?;
    let chunks: i64 = sqlx::query_scalar("SELECT count(*) FROM search_embeddings")
        .fetch_one(db.pool())
        .await
        .unwrap_or(0);
    if chunks > 0 {
        // Rough estimate at the ingest floor (≥50 windows/s from the R&D bench).
        let secs = (chunks as f64 / 50.0).ceil() as i64;
        println!("   ~{chunks} chunks to re-embed (rough estimate: {}).", human_dur(secs));
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

    // 1. Wipe the derived vectors (mirror migration 0017's model-swap reset).
    println!("→ wiping the derived vector index (source data untouched)…");
    wipe_derived(db.pool()).await?;

    // 2. Re-pin the new fingerprint + dims: the env file for the next boot, and
    //    this process so the resize below sees the new width.
    println!("→ pinning the new model fingerprint…");
    upsert_env(ENV_FILE, "VIRTUES_EMBED_FINGERPRINT", &new_fp)?;
    upsert_env(ENV_FILE, "VIRTUES_EMBED_DIMS", &new_dim.to_string())?;
    std::env::set_var("VIRTUES_EMBED_FINGERPRINT", &new_fp);
    std::env::set_var("VIRTUES_EMBED_DIMS", new_dim.to_string());

    // 3. Resize the (now-empty) vector columns to the new width + rebuild index.
    //    initialize() re-runs migrations (idempotent) then ensure_embedding_dims,
    //    which resizes because the tables are empty after the wipe.
    if Some(new_dim) != stored_dim {
        println!("→ sizing the vector index to {new_dim} dims…");
    }
    crate::database::Database::new(&database_url)?
        .initialize()
        .await
        .map_err(|e| Error::Other(format!("resize: {e}")))?;

    println!();
    println!("✓ Re-configured. Restart the box so the new model takes over and re-indexing begins:");
    println!("    sudo systemctl restart virtues");
    Ok(())
}

/// Truncate the derived embedding tables and reset every embedding-derived
/// score, exactly as migration 0017 does on a model swap. Source rows are never
/// touched — embeddings rebuild from them.
async fn wipe_derived(pool: &PgPool) -> Result<()> {
    for stmt in [
        // CASCADE truncates search_vectors too (it FK-references search_embeddings).
        "TRUNCATE search_embeddings CASCADE",
        "TRUNCATE search_topic_cache",
        // The geometry goes with the vectors. The indexer refuses to write vectors
        // from a model the index was not built with — and adopting the new model is
        // the entire point of this command, so the old geometry must not survive it.
        "UPDATE search_index_meta SET n_docs = 0, sum_len = 0, \
             model = NULL, dim = NULL, fingerprint = NULL, built_at = NULL",
        // wiki_events carries its own embedding blob + derived novelty/autonomic
        // scores; null them so each scoring pass recomputes with the new model.
        "UPDATE wiki_events SET \
             embedding = NULL, novelty_z = NULL, local_novelty_z = NULL, \
             hr_z = NULL, hrv_z = NULL, autonomic_z = NULL, topic_novelty = NULL, \
             entity_novelty = NULL",
    ] {
        sqlx::query(stmt)
            .execute(pool)
            .await
            .map_err(|e| Error::Database(format!("wiping derived embeddings: {e}")))?;
    }
    Ok(())
}

/// Upsert a `KEY=value` line in the box env file, preserving everything else.
/// Values here are a hex fingerprint and an integer — no quoting needed. On a
/// dev machine (no env file) this is a no-op; the caller has already set the
/// process env for the in-process resize.
fn upsert_env(path: &str, key: &str, value: &str) -> Result<()> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        println!("  (no {path} — set {key} in your environment for the next run)");
        return Ok(());
    }
    let contents =
        std::fs::read_to_string(p).map_err(|e| Error::Other(format!("read {path}: {e}")))?;
    let prefix = format!("{key}=");
    let line = format!("{key}={value}");
    let mut found = false;
    let mut out: Vec<String> = contents
        .lines()
        .map(|l| {
            if l.trim_start().starts_with(&prefix) {
                found = true;
                line.clone()
            } else {
                l.to_string()
            }
        })
        .collect();
    if !found {
        out.push(line);
    }
    let mut body = out.join("\n");
    body.push('\n');
    std::fs::write(p, body).map_err(|e| Error::Other(format!("write {path}: {e}")))?;
    Ok(())
}

fn short(fp: &str) -> &str {
    &fp[..fp.len().min(12)]
}

fn human_dur(secs: i64) -> String {
    if secs < 90 {
        format!("~{secs}s")
    } else if secs < 5400 {
        format!("~{}m", (secs + 59) / 60)
    } else {
        format!("~{:.1}h", secs as f64 / 3600.0)
    }
}
