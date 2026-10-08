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
//! nothing here touches it. Nor a Dragon, whose NPU models are compiled for
//! the board.
//!
//! Runs as root, from `virtues upgrade` and the nightly `virtues auto-update`.

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

/// The `-m` argument of a llama-server unit's ExecStart.
fn unit_model(unit_text: &str) -> Option<PathBuf> {
    let exec = unit_text.lines().find(|l| l.trim_start().starts_with("ExecStart="))?;
    let mut it = exec.split_whitespace();
    while let Some(tok) = it.next() {
        if tok == "-m" || tok == "--model" {
            return it.next().map(PathBuf::from);
        }
    }
    None
}

/// A copy of a llama-server unit serving another model on another port.
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
            let mut toks = line.split(' ').peekable();
            while let Some(tok) = toks.next() {
                out.push(tok.to_string());
                if tok == "-m" || tok == "--model" {
                    toks.next();
                    out.push(model.display().to_string());
                } else if tok == "--port" {
                    toks.next();
                    out.push(port.to_string());
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
    systemctl(&["disable", "--now", RERANK]);
    let _ = std::fs::remove_file(unit_path(RERANK));
    systemctl(&["daemon-reload"]);
    let _ = std::fs::remove_file(models_dir().join(RERANK_GGUF));
    if let Err(e) = box_env::edit(&box_env::path(), &[], &["VIRTUES_RERANK_URL"]) {
        ui::warn(&format!("couldn't remove VIRTUES_RERANK_URL from the box env: {e}"));
    }
}

/// After a release is running: on the recommended setup, retire the reranker,
/// and if this release recommends a different embedding model, start the move
/// to it. Search keeps working throughout; the indexer finishes the move.
/// Never fails an upgrade; a step that can't run says so and tries again on
/// the next one.
pub(crate) async fn start_recommended_change() {
    if !on_recommended_setup() {
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
        && box_env::get("VIRTUES_EMBED_NEXT_URL").is_some();
    if under_way {
        return;
    }

    ui::step(&format!("moving search to {EMBED_GGUF} (search keeps working while it rebuilds)…"));
    if let Err(e) = fetch_model(&target).await {
        ui::warn(&format!("couldn't download {EMBED_GGUF}: {e}. The next update tries again."));
        return;
    }
    if let Err(e) = std::fs::write(unit_path(NEXT), retarget_unit(&embed_unit, &target, 18183)) {
        ui::warn(&format!("couldn't write {NEXT}.service: {e}"));
        return;
    }
    systemctl(&["daemon-reload"]);
    if !systemctl(&["enable", "--now", NEXT]) {
        ui::warn(&format!("{NEXT} did not start; check `systemctl status {NEXT}`"));
    }
    let set = [
        ("VIRTUES_EMBED_NEXT_URL", NEXT_URL.to_string()),
        ("VIRTUES_EMBED_NEXT_QUERY_PROMPT", EMBED_QUERY_PROMPT.to_string()),
        ("VIRTUES_EMBED_NEXT_DOC_PROMPT", EMBED_DOC_PROMPT.to_string()),
    ];
    // Stored at the model's native width: no NEXT_DIMS.
    let unset = ["VIRTUES_EMBED_NEXT_MODEL", "VIRTUES_EMBED_NEXT_FINGERPRINT", "VIRTUES_EMBED_NEXT_DIMS"];
    match box_env::edit(&box_env::path(), &set, &unset) {
        Ok(_) => ui::ok("search will switch to the new model once its index is built"),
        Err(e) => ui::warn(&format!("couldn't point the indexer at the new model: {e}")),
    }
}

/// Before the server starts, or with `restart_server` from the nightly pass:
/// if a model change has finished (the indexer promoted the next endpoint),
/// move the new model onto the usual unit and port and remove the old one.
pub(crate) async fn settle_finished_change(restart_server: bool) {
    if !on_recommended_setup() || !unit_path(NEXT).exists() {
        return;
    }
    let finished = box_env::get("VIRTUES_EMBED_NEXT_URL").is_none()
        && box_env::get("VIRTUES_EMBED_URL").as_deref() == Some(NEXT_URL);
    if !finished {
        return;
    }
    let (Ok(next_unit), Ok(embed_unit)) = (
        std::fs::read_to_string(unit_path(NEXT)),
        std::fs::read_to_string(unit_path(EMBED)),
    ) else {
        return;
    };
    let (Some(new_model), old_model) = (unit_model(&next_unit), unit_model(&embed_unit)) else {
        return;
    };

    ui::step("moving the new search model onto its usual port…");
    if let Err(e) = std::fs::write(unit_path(EMBED), retarget_unit(&embed_unit, &new_model, 18181)) {
        ui::warn(&format!("couldn't rewrite {EMBED}.service: {e}"));
        return;
    }
    systemctl(&["daemon-reload"]);
    systemctl(&["restart", EMBED]);
    if !wait_serving(EMBED_URL).await {
        ui::warn(&format!("{EMBED} didn't come up with the new model; search stays on {NEXT}"));
        return;
    }
    if let Err(e) = box_env::edit(&box_env::path(), &[("VIRTUES_EMBED_URL", EMBED_URL.to_string())], &[]) {
        ui::warn(&format!("couldn't point search back at {EMBED_URL}: {e}"));
        return;
    }
    if restart_server {
        systemctl(&["restart", "virtues"]);
    }
    systemctl(&["disable", "--now", NEXT]);
    let _ = std::fs::remove_file(unit_path(NEXT));
    systemctl(&["daemon-reload"]);
    if let Some(old) = old_model.filter(|o| *o != new_model) {
        let _ = std::fs::remove_file(old);
    }
    ui::ok("search runs on the new model");
}

/// Wait for a llama-server to load its model and answer `/health`.
async fn wait_serving(base_url: &str) -> bool {
    let Ok(client) = crate::http_client::base_builder().timeout(Duration::from_secs(3)).build() else {
        return false;
    };
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        if let Ok(r) = client.get(format!("{base_url}/health")).send().await {
            if r.status().is_success() {
                return true;
            }
        }
    }
    false
}

/// Download the recommended model, verified against the checksum this binary
/// pins. A copy already on disk is kept if it verifies.
async fn fetch_model(dest: &Path) -> Result<(), String> {
    if sha256_file(dest).is_ok_and(|h| h == EMBED_GGUF_SHA256) {
        return Ok(());
    }
    let dir = dest.parent().ok_or("the models dir has no parent")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let url = format!("{}/{EMBED_GGUF}", models_base());
    let mut resp = super::upgrade::send_get(&url).await.map_err(|e| e.to_string())?;
    if let Some(len) = resp.content_length() {
        super::upgrade::ensure_space(dir, len, "download the new search model")
            .map_err(|e| e.to_string())?;
    }
    let part = dest.with_extension("gguf.part");
    let mut file = std::fs::File::create(&part).map_err(|e| format!("create {}: {e}", part.display()))?;
    let mut hash = Sha256::new();
    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("download: {e}"))? {
        hash.update(&chunk);
        file.write_all(&chunk).map_err(|e| format!("write: {e}"))?;
    }
    file.flush().map_err(|e| format!("flush: {e}"))?;
    let got = format!("{:x}", hash.finalize());
    if got != EMBED_GGUF_SHA256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!("checksum mismatch (expected {EMBED_GGUF_SHA256}, got {got})"));
    }
    std::fs::rename(&part, dest).map_err(|e| format!("rename: {e}"))?;
    let _ = Command::new("chown").args(["virtues:virtues"]).arg(dest).status();
    Ok(())
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

    const UNIT: &str = "[Unit]\nDescription=Virtues embedding sidecar (llama-server, embeddinggemma-300m)\n\n[Service]\nExecStart=/usr/local/lib/virtues/current/llama-server --embedding --pooling mean -m /var/lib/virtues/models/embeddinggemma-300m-qat-Q8_0.gguf --host 127.0.0.1 --port 18181 -c 2048 -ngl 0\nRestart=on-failure\n";

    #[test]
    fn a_retargeted_unit_serves_the_new_model_on_the_new_port() {
        let model = Path::new("/var/lib/virtues/models/embeddinggemma-2-Q8_0.gguf");
        let next = retarget_unit(UNIT, model, 18183);
        assert_eq!(unit_model(&next).as_deref(), Some(model));
        assert!(next.contains("--port 18183 -c 2048 -ngl 0\n"), "{next}");
        assert!(next.contains("--pooling mean -m /var/lib/virtues/models/embeddinggemma-2-Q8_0.gguf --host"));
        assert!(next.contains("Description=Virtues embedding sidecar (llama-server, embeddinggemma-2-Q8_0)"));
        assert!(next.contains("Restart=on-failure"));
        assert_eq!(
            unit_model(UNIT).as_deref(),
            Some(Path::new("/var/lib/virtues/models/embeddinggemma-300m-qat-Q8_0.gguf"))
        );
    }
}
