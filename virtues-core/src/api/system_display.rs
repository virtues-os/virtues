//! Settings → Display — the authenticated control surface for the box's
//! attached screen.
//!
//! Deliberately a separate module from `api::display`. That one is the
//! loopback-only feed the kiosk itself polls — it carries the setup phrase,
//! and its uniform box-local rule is the security argument ("a single
//! exception is how the next one gets argued for"). This module is the
//! opposite side of the glass: what a *paired device* may know and change
//! about the screen, over the ordinary authenticated surface. Nothing here
//! ever includes the phrase or the live setup session.
//!
//! Three routes:
//! - `GET  /api/system/display`          — panel facts, service state, the
//!   configured face, and a redacted mirror of what the glass is showing
//!   (the Settings page's live miniature renders from it).
//! - `PUT  /api/system/display/face`     — choose the ambient face.
//! - `POST /api/system/display/restart`  — restart `virtues-display`, the
//!   canonical remedy for a kiosk whose page died under a healthy process.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::server::AppState;

/// The kiosk unit the installer writes on an appliance. Its existence is the
/// direct truth of "this box has a screen stack" — `install_manifest`'s
/// appliance flag falls back to exactly this path.
const DISPLAY_UNIT_FILE: &str = "/etc/systemd/system/virtues-display.service";

/// The panel's design canvas width in CSS px — the number the kiosk's shim
/// divides the DRM mode width by to derive its zoom (see the installer's
/// `display.py`). Duplicated here only to *report* the derived zoom; the shim
/// remains the authority that applies it.
const DESIGN_WIDTH_PX: f64 = 585.0;

// ============================================================================
// The face — what the ambient slot shows
// ============================================================================

/// The face the panel wears in its ambient slot (claimed, nothing
/// interrupting). Stored in the `app_display` singleton.
#[derive(Debug, Clone, Serialize)]
pub struct FaceConfig {
    /// `builtin` | `applet`.
    pub kind: String,
    /// Which built-in, when `kind` is `builtin`: `record` (the census
    /// ambient screen) or `matte` (black glass on purpose).
    pub builtin: String,
    /// Which applet's `face/index.html`, when `kind` is `applet`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applet_id: Option<String>,
}

impl Default for FaceConfig {
    fn default() -> Self {
        FaceConfig {
            kind: "builtin".into(),
            builtin: "record".into(),
            applet_id: None,
        }
    }
}

/// Read the configured face.
pub(crate) async fn try_face_config(
    pool: &sqlx::PgPool,
) -> crate::error::Result<FaceConfig> {
    let row = sqlx::query(
        "SELECT face_kind, face_builtin, face_applet_id FROM app_display WHERE id",
    )
    .fetch_one(pool)
    .await
    .map_err(|e| crate::error::Error::Database(format!("app_display read failed: {e}")))?;
    Ok(FaceConfig {
        kind: row.get("face_kind"),
        builtin: row.get("face_builtin"),
        applet_id: row.get("face_applet_id"),
    })
}

/// The face, for the kiosk's own state feed. Absence here means "behave as
/// before this table existed": the panel must always render *something*, and
/// the record screen is what every box showed before faces were choosable —
/// so a read failure degrades to it rather than taking the glass down with
/// the query. The settings surface uses [`try_face_config`] and reports the
/// error instead.
pub(crate) async fn face_config_or_default(pool: &sqlx::PgPool) -> FaceConfig {
    match try_face_config(pool).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "display: face config unreadable, showing the record");
            FaceConfig::default()
        }
    }
}

// ============================================================================
// Panel hardware — the first time the server can see the screen
// ============================================================================

/// What `/sys/class/drm` says is plugged in. Mirrors the kiosk shim's
/// `_mode_width()`: first `connected` connector, first (preferred) mode.
///
/// No EDID beyond this, deliberately: the 7" panel claims 53×30 cm when it is
/// physically 15.5×8.7, so nothing derived from claimed physical size may be
/// trusted or shown. The pixel mode is real; the inches are not.
#[derive(Debug, Serialize)]
pub struct PanelInfo {
    /// The DRM connector name, e.g. `card0-HDMI-A-1`.
    pub connector: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode_height: Option<u32>,
}

fn connected_panel() -> Option<PanelInfo> {
    let entries = std::fs::read_dir("/sys/class/drm").ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        // Connectors are `cardN-<name>`; the bare `cardN` and `version`
        // entries have no status file worth reading.
        if !name.contains('-') {
            continue;
        }
        let status = std::fs::read_to_string(entry.path().join("status")).unwrap_or_default();
        if status.trim() != "connected" {
            continue;
        }
        let modes = std::fs::read_to_string(entry.path().join("modes")).unwrap_or_default();
        let (mode_width, mode_height) = modes
            .lines()
            .next()
            .and_then(|m| m.trim().split_once('x'))
            .map(|(w, h)| (w.parse().ok(), h.parse().ok()))
            .unwrap_or((None, None));
        return Some(PanelInfo { connector: name, mode_width, mode_height });
    }
    None
}

/// `virtues-display`'s state, in the doctor's vocabulary — a headless box
/// legitimately has no display, so "not installed" is a fact, not a fault.
fn unit_state() -> &'static str {
    if !std::path::Path::new(DISPLAY_UNIT_FILE).exists() {
        return "not installed";
    }
    match std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", "virtues-display"])
        .status()
    {
        Ok(s) if s.success() => "active",
        _ => "installed but not running",
    }
}

// ============================================================================
// GET /api/system/display
// ============================================================================

#[derive(Debug, Serialize)]
pub struct DisplaySettings {
    /// This box has the kiosk stack installed. False on every DIY box and in
    /// every dev checkout; the section renders its honest empty banner.
    pub attached: bool,
    pub unit_state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub panel: Option<PanelInfo>,
    /// The zoom the shim would derive (mode width / design width). `None`
    /// when no mode is readable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom_derived: Option<f64>,
    /// A `VIRTUES_DISPLAY_ZOOM` override, when one is set in the box env.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom_override: Option<f64>,
    pub face: FaceConfig,
    pub state: GlassState,
}

/// What the glass is showing, redacted for the LAN.
///
/// The Settings page's miniature renders from this. It is `DisplayState`
/// minus everything proximity-gated: no setup phrase, no live setup-session
/// name — a remote mirror may say *that* the panel is on its setup screen,
/// never what the words are.
#[derive(Debug, Serialize)]
pub struct GlassState {
    pub claimed: bool,
    pub online: bool,
    pub connectivity: String,
    pub devices: i64,
    pub box_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_disk_fault: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub record: Vec<crate::api::display::RecordLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record_since: Option<chrono::DateTime<chrono::Utc>>,
    /// An upgrade is running right now — the mirror shows the same
    /// "Updating" card the glass does.
    pub updating: bool,
}

pub async fn get_display_settings_handler(State(state): State<AppState>) -> Response {
    let pool = state.db.pool();

    let face = match try_face_config(pool).await {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let devices = crate::api::pair::paired_device_count(pool).await;
    let unclaimed = crate::api::pair::is_unclaimed(pool).await;
    let connectivity = crate::cli::link::connectivity();
    let online = crate::cli::link::verdict_means_online(&connectivity);
    let (record, record_since) = if devices > 0 {
        crate::api::display::record_lines(pool)
    } else {
        (Vec::new(), None)
    };

    let panel = connected_panel();
    let zoom_derived = panel
        .as_ref()
        .and_then(|p| p.mode_width)
        .map(|w| f64::from(w) / DESIGN_WIDTH_PX);
    // The box env (main.rs dotenvs virtues.env) is where the kiosk unit's
    // override also lives, so the server sees the same value the shim does.
    let zoom_override = std::env::var("VIRTUES_DISPLAY_ZOOM")
        .ok()
        .and_then(|v| v.parse::<f64>().ok());

    Json(DisplaySettings {
        attached: std::path::Path::new(DISPLAY_UNIT_FILE).exists(),
        unit_state: unit_state(),
        panel,
        zoom_derived,
        zoom_override,
        face,
        state: GlassState {
            claimed: !unclaimed,
            online,
            connectivity,
            devices,
            box_name: crate::codename::pretty(&crate::codename::box_codename()),
            data_disk_fault: crate::data_disk::status().message(),
            record,
            record_since,
            updating: crate::api::display::upgrade_unit_active(),
        },
    })
    .into_response()
}

// ============================================================================
// PUT /api/system/display/face
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SetFaceBody {
    pub kind: String,
    pub builtin: Option<String>,
    pub applet_id: Option<String>,
}

const BUILTIN_FACES: &[&str] = &["record", "matte"];

pub async fn set_display_face_handler(
    State(state): State<AppState>,
    Json(body): Json<SetFaceBody>,
) -> Response {
    fn bad(msg: &str) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response()
    }

    let (builtin, applet_id) = match body.kind.as_str() {
        "builtin" => {
            let Some(b) = body.builtin.as_deref() else {
                return bad("builtin face not named");
            };
            if !BUILTIN_FACES.contains(&b) {
                return bad("no such built-in face");
            }
            (b.to_string(), None)
        }
        "applet" => {
            let Some(id) = body.applet_id.clone() else {
                return bad("applet_id required for an applet face");
            };
            // The choice is only meaningful if the applet ships a face —
            // hanging a faceless applet would put a 404 on the glass.
            if crate::server::faces::face_dir_for(&id).is_none() {
                return bad("that applet has no face");
            }
            // Unused for applet faces; keep the column's default so a later
            // switch back to `builtin` lands somewhere sensible.
            ("record".to_string(), Some(id))
        }
        _ => return bad("kind must be builtin or applet"),
    };

    let result = sqlx::query(
        "UPDATE app_display
         SET face_kind = $1, face_builtin = $2, face_applet_id = $3, updated_at = now()
         WHERE id",
    )
    .bind(&body.kind)
    .bind(&builtin)
    .bind(&applet_id)
    .execute(state.db.pool())
    .await;

    match result {
        Ok(_) => Json(FaceConfig { kind: body.kind, builtin, applet_id }).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("could not save the face: {e}") })),
        )
            .into_response(),
    }
}

// ============================================================================
// POST /api/system/display/restart
// ============================================================================

/// Restart the kiosk. The known failure this remedies: cage and WebKit are
/// perfectly healthy while the page underneath them has died (an upgrade took
/// the server away, or the SPA went stale), so `Restart=always` never fires —
/// see `cli::upgrade::restart_display`, the same verb run automatically after
/// an upgrade. Uses the `virtues` account's existing sudo grant, exactly as
/// `api::updates::apply` does — a use of a standing grant, not a new one. No
/// user input reaches the command line; the argv is fixed.
pub async fn restart_display_handler() -> Response {
    if !std::path::Path::new(DISPLAY_UNIT_FILE).exists() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "no display service is installed on this box"
            })),
        )
            .into_response();
    }

    let output = std::process::Command::new("sudo")
        .args(["-n", "systemctl", "restart", "virtues-display"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            Json(serde_json::json!({ "restarted": true })).into_response()
        }
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let detail = stderr.trim();
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": if detail.is_empty() {
                        format!("systemctl exited {}", o.status)
                    } else {
                        detail.to_string()
                    }
                })),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("could not invoke systemctl: {e}") })),
        )
            .into_response(),
    }
}
