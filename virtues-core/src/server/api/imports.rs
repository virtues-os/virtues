//! One-time imports: a file the owner downloaded from a service, uploaded
//! here and read by that service's import applet.
//!
//!   POST /api/imports/:source/upload — `spotify`, `amazon` or `discord`
//!
//! The upload streams into a staging folder, so a large download never sits
//! in memory, and the source's applet starts in the background with
//! `payload = { "file_path": ... }`. The answer is the run's status, the same
//! shape a manual run gets; the app follows the run for its summary or error.
//! The applet reads only files in its staging folder and removes the file
//! when it is done.
//!
//! Past chats has its own route (`/api/chat-import/upload` in `applets.rs`)
//! that the app and older setups call, and it stages through the same code
//! here.

use std::collections::HashSet;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Mutex;

use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, State},
    http::StatusCode,
    response::Response,
    routing::post,
    Router,
};

use super::error_response;
use crate::error::Error;
use crate::server::AppState;

/// A download larger than this is not one of these services' exports.
const MAX_BYTES: usize = 2 * 1024 * 1024 * 1024;

/// The folder the three imports stage into. Their applets read only from a
/// folder of this name (or `virtues-imports` in a dev checkout).
const IMPORTS_FOLDER: &str = "imports";

/// The applet that reads each source's download.
fn applet_for(source: &str) -> Option<&'static str> {
    match source {
        "spotify" => Some("applet_spotify_import"),
        "amazon" => Some("applet_amazon_orders_import"),
        "discord" => Some("applet_discord_import"),
        _ => None,
    }
}

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/imports/:source/upload",
        post(import_upload_handler).layer(DefaultBodyLimit::max(MAX_BYTES)),
    )
}

/// POST /api/imports/:source/upload — stage the file and start its import.
pub async fn import_upload_handler(
    State(state): State<AppState>,
    Path(source): Path<String>,
    mut multipart: Multipart,
) -> Response {
    let Some(applet_id) = applet_for(&source) else {
        return error_response(Error::NotFound(format!("no import for {source}")));
    };
    let staged = match stage_upload(&mut multipart, IMPORTS_FOLDER, "This file is larger than 2 GB.").await {
        Ok(s) => s,
        Err(e) => return error_response(e),
    };
    start_import(&state, applet_id, staged).await
}

/// Start `applet_id` on a staged file and answer with the run's status.
pub(crate) async fn start_import(state: &AppState, applet_id: &str, staged: StagedUpload) -> Response {
    let deps = crate::applet_runner::RunnerDeps {
        db: state.db.pool().clone(),
        yjs: state.yjs_state.clone(),
    };
    let payload = serde_json::json!({ "file_path": staged.path.to_string_lossy() });
    match crate::applet_runner::run_applet_detached(&deps, applet_id, "manual", Some(&payload)).await {
        Ok(result) => {
            // A run that started reads the file and removes it; any other
            // outcome leaves it to `staged`'s drop.
            if matches!(result.status, crate::applet_runner::AppletRunStatus::Running) {
                staged.keep();
            }
            super::applets::run_status_response(result, applet_id)
        }
        Err(e) => error_response(e),
    }
}

/// An upload in a staging folder, removed when dropped unless kept for a run
/// that has started. Covers every early return and a request the client
/// drops mid-upload.
pub(crate) struct StagedUpload {
    pub path: PathBuf,
    kept: bool,
}

impl StagedUpload {
    pub fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for StagedUpload {
    fn drop(&mut self) {
        if !self.kept {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// Where uploads named `folder` wait: under the state root on a box, or
/// `virtues-<folder>` in the temp folder in a dev checkout with no state root.
/// Each import applet reads only from a folder of one of its two names.
pub(crate) fn staging_dir(folder: &str) -> PathBuf {
    let root = crate::data_disk::data_dir();
    if root.is_dir() {
        root.join(folder)
    } else {
        std::env::temp_dir().join(format!("virtues-{folder}"))
    }
}

/// Remove uploads an earlier server process left behind (killed mid-upload,
/// or its import killed mid-read), once per folder per process and before its
/// first upload. Only these routes write the folders, and a run from an
/// earlier process is not running any more.
fn sweep_once(dir: &FsPath) {
    static SWEPT: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);
    let mut swept = SWEPT.lock().unwrap_or_else(|e| e.into_inner());
    if !swept.get_or_insert_with(HashSet::new).insert(dir.to_path_buf()) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let removed = entries.flatten().filter(|e| std::fs::remove_file(e.path()).is_ok()).count();
    if removed > 0 {
        tracing::info!("removed {removed} upload(s) an earlier server left in {}", dir.display());
    }
}

/// Stream the upload's `file` field into the staging folder `folder`, mode
/// 0600. `too_large` is the sentence for an upload over the route's limit.
pub(crate) async fn stage_upload(multipart: &mut Multipart, folder: &str, too_large: &str) -> Result<StagedUpload, Error> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    use tokio::io::AsyncWriteExt;

    let dir = staging_dir(folder);
    sweep_once(&dir);
    let stage_failed = |e: std::io::Error| Error::Other(format!("could not save the upload in {}: {e}", dir.display()));
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir).map_err(stage_failed)?;

    let cut_off = |e: axum::extract::multipart::MultipartError| {
        if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
            Error::InvalidInput(too_large.to_string())
        } else {
            Error::InvalidInput(format!("The upload was cut off. Try it again. ({e})"))
        }
    };

    while let Some(mut field) = multipart.next_field().await.map_err(cut_off)? {
        if field.name() != Some("file") {
            continue;
        }
        let path = dir.join(format!("{}.upload", uuid::Uuid::new_v4()));
        let std_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(stage_failed)?;
        // Owned from here, so any failure below removes the file.
        let staged = StagedUpload { path, kept: false };
        let mut out = tokio::fs::File::from_std(std_file);
        while let Some(chunk) = field.chunk().await.map_err(cut_off)? {
            out.write_all(&chunk).await.map_err(stage_failed)?;
        }
        out.flush().await.map_err(stage_failed)?;
        return Ok(staged);
    }
    Err(Error::InvalidInput("No file was sent. Choose the file and try again.".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_three_sources_have_an_import() {
        assert_eq!(applet_for("spotify"), Some("applet_spotify_import"));
        assert_eq!(applet_for("amazon"), Some("applet_amazon_orders_import"));
        assert_eq!(applet_for("discord"), Some("applet_discord_import"));
        assert_eq!(applet_for("chat_import"), None);
        assert_eq!(applet_for("../etc"), None);
    }

    #[test]
    fn a_staged_upload_is_removed_unless_kept() {
        let dir = std::env::temp_dir().join(format!("virtues-staging-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let dropped = dir.join("a.upload");
        std::fs::write(&dropped, b"x").unwrap();
        drop(StagedUpload { path: dropped.clone(), kept: false });
        assert!(!dropped.exists());

        let kept = dir.join("b.upload");
        std::fs::write(&kept, b"x").unwrap();
        StagedUpload { path: kept.clone(), kept: false }.keep();
        assert!(kept.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
