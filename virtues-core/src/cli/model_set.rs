//! Keeping a box on the recommended search models.
//!
//! A box on the recommended setup (`VIRTUES_INFERENCE=bundled`) runs the
//! embedding model its release names (`inference_report::EMBED_GGUF`). When an
//! upgrade brings a different one, [`start_recommended_change`] downloads it,
//! starts it beside the current model as `virtues-embed-next` on :18183, and
//! points `VIRTUES_EMBED_NEXT_*` at it. The indexer then builds the index for it
//! in the background and swaps it in (`search::next_index`); search keeps using
//! the old model until then. Once the swap is done, [`settle_finished_change`]
//! moves the new model onto `virtues-embed` and :18181 and removes the old one.
//!
//! The reranker goes too: it made results worse on every eval we ran
//! (`search::query::rerank_gap_threshold`), so a box that hasn't opted in
//! (`VIRTUES_RERANK_GAP`) stops running it.
//!
//! A box on its own server (`manual`) chose its models and maintains them, so
//! nothing here touches it, not even when that server stops answering: it may
//! be rebooting, and a quiet switch would rebuild the index for a model the
//! owner didn't choose. Going back is the owner's call, and one command:
//! [`use_recommended`]. Nor is a Dragon touched; its NPU models are compiled
//! for the board.
//!
//! Runs as root, from `virtues upgrade`, the nightly `virtues auto-update`, and
//! `virtues configure-inference --recommended`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::ui;
use crate::box_env;
use crate::inference_report::{
    EMBED_DOC_PROMPT, EMBED_GGUF, EMBED_GGUF_SHA256, EMBED_QUERY_PROMPT, RERANK_GGUF,
};

const UNITS: &str = "/etc/systemd/system";
const EMBED: &str = "virtues-embed";
const NEXT: &str = "virtues-embed-next";
const RERANK: &str = "virtues-rerank";
const EMBED_URL: &str = "http://127.0.0.1:18181";
const NEXT_URL: &str = "http://127.0.0.1:18183";

fn unit_path(unit: &str) -> PathBuf {
    Path::new(UNITS).join(format!("{unit}.service"))
}

fn on_recommended_setup() -> bool {
    box_env::get("VIRTUES_INFERENCE").as_deref() == Some("bundled")
}

fn models_dir() -> PathBuf {
    box_env::get("VIRTUES_MODELS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/var/lib/virtues/models"))
}

fn models_base() -> String {
    box_env::get("VIRTUES_MODELS_BASE").unwrap_or_else(|| {
        format!("https://github.com/{}/releases/download/models-1", super::upgrade::RELEASE_REPO)
    })
}

/// The `-m` argument of a llama-server unit's ExecStart (`-m PATH`,
/// `--model PATH` or `--model=PATH`).
fn unit_model(unit_text: &str) -> Option<PathBuf> {
    let exec = unit_text.lines().find(|l| l.trim_start().starts_with("ExecStart="))?;
    let mut it = exec.split_whitespace();
    while let Some(tok) = it.next() {
        if tok == "-m" || tok == "--model" {
            return it.next().map(PathBuf::from);
        }
        if let Some(p) = tok.strip_prefix("--model=") {
            return Some(PathBuf::from(p));
        }
    }
    None
}

/// A copy of a llama-server unit serving another model on another port. Reads
/// the ExecStart the same way [`unit_model`] does, so a unit it can read is a
/// unit it can rewrite.
fn retarget_unit(unit_text: &str, model: &Path, port: u16) -> String {
    let stem = model.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    unit_text
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("Description=") {
                return format!("Description=Virtues embedding sidecar (llama-server, {stem})");
            }
            if !line.trim_start().starts_with("ExecStart=") {
                return line.to_string();
            }
            let mut out: Vec<String> = Vec::new();
            let mut toks = line.split_whitespace();
            while let Some(tok) = toks.next() {
                if tok.starts_with("--model=") {
                    out.push(format!("--model={}", model.display()));
                } else if tok.starts_with("--port=") {
                    out.push(format!("--port={port}"));
                } else {
                    out.push(tok.to_string());
                    if tok == "-m" || tok == "--model" {
                        toks.next();
                        out.push(model.display().to_string());
                    } else if tok == "--port" {
                        toks.next();
                        out.push(port.to_string());
                    }
                }
            }
            out.join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn systemctl(args: &[&str]) -> bool {
    Command::new("systemctl").args(args).status().map(|s| s.success()).unwrap_or(false)
}

fn remove_unit(unit: &str) {
    systemctl(&["disable", "--now", unit]);
    let _ = std::fs::remove_file(unit_path(unit));
    systemctl(&["daemon-reload"]);
}

/// The reranker made results worse on every eval we ran, so search stopped
/// calling it unless the owner opts in. Stop running it too, and get its
/// memory back.
fn retire_reranker() {
    let opted_in = box_env::get("VIRTUES_RERANK_GAP")
        .and_then(|g| g.trim().parse::<f64>().ok())
        .is_some_and(|g| g > 0.0);
    if opted_in || !unit_path(RERANK).exists() {
        return;
    }
    ui::step("retiring the reranker (search no longer uses it)…");
    remove_unit(RERANK);
    let _ = std::fs::remove_file(models_dir().join(RERANK_GGUF));
    if let Err(e) = box_env::edit(&box_env::path(), &[], &["VIRTUES_RERANK_URL"]) {
        ui::warn(&format!("couldn't remove VIRTUES_RERANK_URL from the box env: {e}"));
    }
}

/// The llama-server this release ships.
fn llama_server() -> PathBuf {
    let prefix = box_env::get("INSTALL_PREFIX").unwrap_or_else(|| "/usr/local".into());
    Path::new(&prefix).join("bin/llama-server")
}

/// Download `target` (if needed), run it as `virtues-embed-next` on :18183, and
/// point `VIRTUES_EMBED_NEXT_*` at it: the start of a model change, which the
/// indexer then carries out (`search::next_index`). The unit is a copy of the
/// box's embed unit when it has one, so it keeps that unit's flags and groups.
///
/// Always a restart, never `enable --now`: a next unit already running (an
/// earlier change, overtaken by a newer recommendation) would otherwise keep
/// serving the model it started with, and the index would be built for that.
async fn stage_next(target: &Path) -> Result<(), String> {
    fetch_model(target).await.map_err(|e| format!("download {EMBED_GGUF}: {e}"))?;
    let body = match std::fs::read_to_string(unit_path(EMBED)) {
        Ok(embed_unit) => retarget_unit(&embed_unit, target, 18183),
        Err(_) => {
            let bin = llama_server();
            if !bin.exists() {
                return Err(format!(
                    "{} isn't on this server; update to the latest release first",
                    bin.display()
                ));
            }
            embed_unit(&bin, target, 18183)
        }
    };
    std::fs::write(unit_path(NEXT), body).map_err(|e| format!("write {NEXT}.service: {e}"))?;
    systemctl(&["daemon-reload"]);
    systemctl(&["enable", NEXT]);
    systemctl(&["restart", NEXT]);
    if !wait_serving(NEXT_URL, target).await {
        return Err(format!(
            "{NEXT} didn't start serving {EMBED_GGUF}; journalctl -u {NEXT} -n 50"
        ));
    }
    let set = [
        ("VIRTUES_EMBED_NEXT_URL", NEXT_URL.to_string()),
        ("VIRTUES_EMBED_NEXT_QUERY_PROMPT", EMBED_QUERY_PROMPT.to_string()),
        ("VIRTUES_EMBED_NEXT_DOC_PROMPT", EMBED_DOC_PROMPT.to_string()),
    ];
    // Stored at the model's native width: no NEXT_DIMS.
    let unset = ["VIRTUES_EMBED_NEXT_MODEL", "VIRTUES_EMBED_NEXT_FINGERPRINT", "VIRTUES_EMBED_NEXT_DIMS"];
    box_env::edit(&box_env::path(), &set, &unset).map_err(|e| e.to_string())?;
    Ok(())
}

/// After a release is running: on the recommended setup, retire the reranker,
/// and if this release recommends a different embedding model, start the move
/// to it. Search keeps working throughout; the indexer finishes the move.
/// Never fails an upgrade; a step that can't run says so and tries again on
/// the next one.
pub(crate) async fn start_recommended_change() {
    if !on_recommended_setup() || crate::inference_report::is_dragon_profile() {
        return;
    }
    retire_reranker();

    let Ok(embed_unit) = std::fs::read_to_string(unit_path(EMBED)) else {
        return;
    };
    let target = models_dir().join(EMBED_GGUF);
    if unit_model(&embed_unit).as_deref() == Some(target.as_path()) {
        return;
    }
    let under_way = std::fs::read_to_string(unit_path(NEXT))
        .ok()
        .and_then(|t| unit_model(&t))
        .is_some_and(|m| m == target)
        && box_env::get("VIRTUES_EMBED_NEXT_URL").as_deref() == Some(NEXT_URL);
    if under_way {
        return;
    }

    ui::step(&format!("moving search to {EMBED_GGUF} (search keeps working while it rebuilds)…"));
    match stage_next(&target).await {
        Ok(()) => ui::ok("search will switch to the new model once its index is built"),
        Err(e) => ui::warn(&format!("{e}. The next update tries again.")),
    }
}

/// A next sidecar nothing will use: its change was called off (the owner
/// moved search to their own server) or never armed. Remove it whatever the
/// setup, or every upgrade that restarts sidecars brings it back.
fn remove_abandoned_next() {
    if !unit_path(NEXT).exists() {
        return;
    }
    let armed = box_env::get("VIRTUES_EMBED_NEXT_URL").as_deref() == Some(NEXT_URL);
    let serving = box_env::get("VIRTUES_EMBED_URL").as_deref() == Some(NEXT_URL);
    if !armed && !serving {
        ui::step(&format!("removing {NEXT} (no model change uses it)…"));
        remove_unit(NEXT);
    }
}

/// Before the server starts, or with `restart_server` from the nightly pass:
/// if a model change has finished (the indexer promoted the next endpoint),
/// move the new model onto the usual unit and port and remove the old one.
pub(crate) async fn settle_finished_change(restart_server: bool) {
    remove_abandoned_next();
    if !on_recommended_setup() || !unit_path(NEXT).exists() {
        return;
    }
    let finished = box_env::get("VIRTUES_EMBED_NEXT_URL").is_none()
        && box_env::get("VIRTUES_EMBED_URL").as_deref() == Some(NEXT_URL);
    if !finished {
        return;
    }
    let Some(new_model) = std::fs::read_to_string(unit_path(NEXT)).ok().and_then(|t| unit_model(&t))
    else {
        return;
    };
    // The index was built for what :18183 actually serves, which is what moves.
    // If that isn't the model its unit names, moving the unit would put a model
    // the index wasn't built for behind search.
    if !wait_serving(NEXT_URL, &new_model).await {
        ui::warn(&format!(
            "{NEXT} isn't serving the model its unit names; search stays on it until it does"
        ));
        return;
    }
    let embed_unit = std::fs::read_to_string(unit_path(EMBED)).ok();
    let old_model = embed_unit.as_deref().and_then(unit_model);

    ui::step("moving the new search model onto its usual port…");
    let body = match &embed_unit {
        Some(text) => retarget_unit(text, &new_model, 18181),
        None => embed_unit_for(&new_model),
    };
    if let Err(e) = std::fs::write(unit_path(EMBED), body) {
        ui::warn(&format!("couldn't rewrite {EMBED}.service: {e}"));
        return;
    }
    systemctl(&["daemon-reload"]);
    systemctl(&["enable", EMBED]);
    systemctl(&["restart", EMBED]);
    if !wait_serving(EMBED_URL, &new_model).await {
        ui::warn(&format!("{EMBED} didn't come up with the new model; search stays on {NEXT}"));
        return;
    }
    if let Err(e) = box_env::edit(&box_env::path(), &[("VIRTUES_EMBED_URL", EMBED_URL.to_string())], &[]) {
        ui::warn(&format!("couldn't point search back at {EMBED_URL}: {e}"));
        return;
    }
    // Restart before stopping next: the running server may hold an embedder
    // pointed at :18183 for a few minutes.
    if restart_server {
        systemctl(&["restart", "virtues"]);
    }
    remove_unit(NEXT);
    if let Some(old) = old_model.filter(|o| *o != new_model) {
        let _ = std::fs::remove_file(old);
    }
    ui::ok("search runs on the new model");
}

/// Wait for the llama-server at `base_url` to load `model` and serve it. Asks
/// `/v1/models` rather than `/health`, because something else may already be
/// answering on that port: an owner's own server, before they go back to the
/// recommended setup.
async fn wait_serving(base_url: &str, model: &Path) -> bool {
    let Ok(client) = crate::http_client::base_builder().timeout(Duration::from_secs(3)).build() else {
        return false;
    };
    let name = model.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    for attempt in 0..90 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        if let Ok(r) = client.get(format!("{base_url}/v1/models")).send().await {
            if r.status().is_success() && r.text().await.is_ok_and(|b| b.contains(&name)) {
                return true;
            }
        }
    }
    false
}

fn embed_unit_for(model: &Path) -> String {
    embed_unit(&llama_server(), model, 18181)
}

/// The embedding sidecar's unit, for a box that never had one (installed on
/// its own server). Must match the installer's `EMBED_UNIT_TEMPLATE`
/// (tools/virtues-installer/src/install.rs), which documents the flags; a box
/// that has the unit keeps its own and only the model and port change.
fn embed_unit(llama_server: &Path, model: &Path, port: u16) -> String {
    format!(
        "[Unit]\n\
         Description=Virtues embedding sidecar (llama-server, {stem})\n\
         Documentation=https://virtues.com/docs\n\
         After=network.target\n\
         StartLimitIntervalSec=300\n\
         StartLimitBurst=5\n\
         \n\
         [Service]\n\
         Type=simple\n\
         User=virtues\n\
         Group=virtues\n\
         ExecStart={bin} --embedding --pooling mean -m {model} --host 127.0.0.1 --port {port} -c 2048 -b 2048 -ub 2048 -np 1 --cache-ram 0 -ngl 0\n\
         Restart=on-failure\n\
         RestartSec=5\n\
         \n\
         NoNewPrivileges=true\n\
         ProtectSystem=strict\n\
         ProtectHome=true\n\
         PrivateTmp=true\n\
         ProtectKernelTunables=true\n\
         ProtectControlGroups=true\n\
         RestrictSUIDSGID=true\n\
         LockPersonality=true\n\
         SystemCallArchitectures=native\n\
         CapabilityBoundingSet=\n\
         \n\
         [Install]\n\
         WantedBy=multi-user.target\n",
        stem = model.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
        bin = llama_server.display(),
        model = model.display(),
    )
}

/// `virtues configure-inference --recommended`: go back from the owner's own
/// server to the recommended setup. Starts the recommended model on this
/// machine's CPU as the next endpoint, the way an update does, so search keeps
/// using the owner's server while the index is rebuilt for it (or switches on
/// the indexer's next run when the index was already built with it). Always on
/// :18183 first: the owner's server may be the old CPU sidecar on :18181, and
/// replacing it in place would put a model the index wasn't built for behind
/// search for the length of the rebuild. The next update moves it to :18181.
pub async fn use_recommended() -> Result<(), crate::Error> {
    if !super::upgrade::running_as_root() {
        return Err(crate::Error::Other(
            "going back to the recommended setup starts a system service; run it with sudo: \
             sudo virtues configure-inference --recommended"
                .into(),
        ));
    }
    if crate::inference_report::is_dragon_profile() {
        return Err(crate::Error::Other(
            "this server's search runs on its NPU, which the installer sets up. To go back to \
             it, re-run the installer."
                .into(),
        ));
    }
    let _lock = super::upgrade::acquire_lock()?;
    let target = models_dir().join(EMBED_GGUF);
    let current = box_env::get("VIRTUES_EMBED_URL").unwrap_or_else(|| EMBED_URL.to_string());
    let embed_model = std::fs::read_to_string(unit_path(EMBED)).ok().and_then(|t| unit_model(&t));
    if on_recommended_setup() && current == EMBED_URL && embed_model.as_deref() == Some(target.as_path()) {
        ui::ok("search already runs on the recommended setup");
        return Ok(());
    }

    ui::step(&format!("starting {EMBED_GGUF} on this machine's CPU…"));
    stage_next(&target).await.map_err(crate::Error::Other)?;
    box_env::edit(&box_env::path(), &[("VIRTUES_INFERENCE", "bundled".to_string())], &[])?;

    println!();
    ui::ok(&format!("{EMBED_GGUF} is running on this machine's CPU"));
    println!("     Search keeps using {current} while your index is rebuilt for it,");
    println!("     then switches. Settings → Search shows how far along it is. After the");
    println!("     switch, you can stop your own server; updates keep this model current.");
    Ok(())
}

/// Download the recommended model, verified against the checksum this binary
/// pins. A copy already on disk is kept if it verifies. Callers hold the
/// upgrade lock; the part file is still this process's own, and is checked
/// again on disk before it takes the model's name.
async fn fetch_model(dest: &Path) -> Result<(), String> {
    if sha256_file(dest).is_ok_and(|h| h == EMBED_GGUF_SHA256) {
        return Ok(());
    }
    let dir = dest.parent().ok_or("the models dir has no parent")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let part = dest.with_extension(format!("gguf.{}.part", std::process::id()));
    let result = download_to(&part, dir).await.and_then(|()| {
        let got = sha256_file(&part).map_err(|e| format!("read back: {e}"))?;
        if got != EMBED_GGUF_SHA256 {
            return Err(format!("checksum mismatch (expected {EMBED_GGUF_SHA256}, got {got})"));
        }
        std::fs::rename(&part, dest).map_err(|e| format!("rename: {e}"))
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
        return result;
    }
    let _ = Command::new("chown").args(["virtues:virtues"]).arg(dest).status();
    Ok(())
}

async fn download_to(part: &Path, dir: &Path) -> Result<(), String> {
    let url = format!("{}/{EMBED_GGUF}", models_base());
    let mut resp = super::upgrade::send_get(&url).await.map_err(|e| e.to_string())?;
    if let Some(len) = resp.content_length() {
        super::upgrade::ensure_space(dir, len, "download the new search model")
            .map_err(|e| e.to_string())?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(part)
        .map_err(|e| format!("create {}: {e}", part.display()))?;
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("download: {e}"))? {
        file.write_all(&chunk).map_err(|e| format!("write: {e}"))?;
    }
    file.sync_all().map_err(|e| format!("flush: {e}"))
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    std::io::copy(&mut f, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: &str = "[Unit]\nDescription=Virtues embedding sidecar (llama-server, embeddinggemma-300m)\n\n[Service]\nSupplementaryGroups=render video\nExecStart=/usr/local/lib/virtues/current/llama-server --embedding --pooling mean -m /var/lib/virtues/models/embeddinggemma-300m-qat-Q8_0.gguf --host 127.0.0.1 --port 18181 -c 2048 -ngl 0\nRestart=on-failure\n";

    #[test]
    fn a_retargeted_unit_serves_the_new_model_on_the_new_port() {
        let model = Path::new("/var/lib/virtues/models/embeddinggemma-2-Q8_0.gguf");
        let next = retarget_unit(UNIT, model, 18183);
        assert_eq!(unit_model(&next).as_deref(), Some(model));
        assert!(next.contains("--port 18183 -c 2048 -ngl 0\n"), "{next}");
        assert!(next.contains("--pooling mean -m /var/lib/virtues/models/embeddinggemma-2-Q8_0.gguf --host"));
        assert!(next.contains("Description=Virtues embedding sidecar (llama-server, embeddinggemma-2-Q8_0)"));
        assert!(next.contains("SupplementaryGroups=render video\n") && next.contains("Restart=on-failure"));
        assert_eq!(
            unit_model(UNIT).as_deref(),
            Some(Path::new("/var/lib/virtues/models/embeddinggemma-300m-qat-Q8_0.gguf"))
        );
    }

    /// A hand-edited unit: `=` forms and doubled spaces read and rewrite alike.
    #[test]
    fn equals_forms_and_extra_spaces_are_rewritten_too() {
        let unit = "ExecStart=/bin/llama-server  --embedding --model=/m/old.gguf  --port=18181\n";
        let model = Path::new("/m/new.gguf");
        let next = retarget_unit(unit, model, 18183);
        assert_eq!(unit_model(unit).as_deref(), Some(Path::new("/m/old.gguf")));
        assert_eq!(unit_model(&next).as_deref(), Some(model));
        assert!(next.contains("--port=18183"), "{next}");
    }

    #[test]
    fn a_fresh_unit_serves_the_model_on_the_port_asked_for() {
        let model = Path::new("/var/lib/virtues/models/embeddinggemma-2-Q8_0.gguf");
        let unit = embed_unit(Path::new("/usr/local/bin/llama-server"), model, 18183);
        assert_eq!(unit_model(&unit).as_deref(), Some(model));
        assert!(unit.contains("ExecStart=/usr/local/bin/llama-server --embedding --pooling mean -m "));
        assert!(unit.contains("--port 18183 "));
        assert!(unit.contains("\nUser=virtues\n") && unit.contains("\nWantedBy=multi-user.target\n"));
    }
}
