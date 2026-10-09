//! `virtues configure-inference` — re-validate the embedding endpoint after a
//! model change and recover the index.
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
    upsert_env(ENV_FILE, "VIRTUES_EMBED_FINGERPRINT", &new_fp)?;
    upsert_env(ENV_FILE, "VIRTUES_EMBED_DIMS", &new_dim.to_string())?;
    std::env::set_var("VIRTUES_EMBED_FINGERPRINT", &new_fp);
    std::env::set_var("VIRTUES_EMBED_DIMS", new_dim.to_string());

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

/// Upsert a `KEY=value` line in the box env file, preserving everything else.
/// Values here are a hex fingerprint and an integer — no quoting needed. On a
/// dev machine (no env file) this is a no-op.
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
