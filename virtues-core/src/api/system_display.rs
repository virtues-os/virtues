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
//! Four routes:
//! - `GET  /api/system/display`          — panel facts, service state, the
//!   configured face, and a redacted mirror of what the glass is showing
//!   (the Settings page's live miniature renders from it).
//! - `PUT  /api/system/display/face`     — choose the ambient face.
//! - `PUT  /api/system/display/hours`    — the sleep schedule.
//! - `POST /api/system/display/restart`  — restart `virtues-display`, the
//!   canonical remedy for a kiosk whose page died under a healthy process.
//!
//! Also home to the **sleep engine** (`spawn_sleep_engine`) — the server-side
//! task that enforces Hours. See its module docs; the design was settled by
//! the 2026-08-26 backlight audit (agents/plan/display-plan.md): sleep is a
//! precedence state below every interruption, the kiosk unit keeps running,
//! and only the connector toggles.

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
// Hours — the sleep schedule
// ============================================================================

/// The screen's hours, box-local. `None`/`None` = the screen never sleeps.
/// Both-or-neither is a DB CHECK (`display_hours_paired`).
#[derive(Debug, Clone, Copy, Serialize)]
pub struct HoursConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_start: Option<chrono::NaiveTime>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sleep_end: Option<chrono::NaiveTime>,
}

pub(crate) async fn try_hours(pool: &sqlx::PgPool) -> crate::error::Result<HoursConfig> {
    let row = sqlx::query("SELECT sleep_start, sleep_end FROM app_display WHERE id")
        .fetch_one(pool)
        .await
        .map_err(|e| crate::error::Error::Database(format!("app_display hours read failed: {e}")))?;
    Ok(HoursConfig {
        sleep_start: row.get("sleep_start"),
        sleep_end: row.get("sleep_end"),
    })
}

/// Is `now` inside the sleep window? Overnight spans (start > end) wrap
/// midnight; `start == end` is rejected at the PUT and reads as inactive here
/// so a bad row can never hold the screen dark forever.
fn window_active(now: chrono::NaiveTime, start: chrono::NaiveTime, end: chrono::NaiveTime) -> bool {
    use std::cmp::Ordering::*;
    match start.cmp(&end) {
        Equal => false,
        Less => now >= start && now < end,
        Greater => now >= start || now < end,
    }
}

#[derive(Debug, Deserialize)]
pub struct SetHoursBody {
    /// "HH:MM", box-local. Both set = schedule on; both null = off.
    pub sleep_start: Option<String>,
    pub sleep_end: Option<String>,
}

pub async fn set_display_hours_handler(
    State(state): State<AppState>,
    Json(body): Json<SetHoursBody>,
) -> Response {
    fn bad(msg: &str) -> Response {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": msg })),
        )
            .into_response()
    }

    let parsed = match (&body.sleep_start, &body.sleep_end) {
        (None, None) => (None, None),
        (Some(s), Some(e)) => {
            let s = match chrono::NaiveTime::parse_from_str(s, "%H:%M") {
                Ok(t) => t,
                Err(_) => return bad("sleep_start must be HH:MM"),
            };
            let e = match chrono::NaiveTime::parse_from_str(e, "%H:%M") {
                Ok(t) => t,
                Err(_) => return bad("sleep_end must be HH:MM"),
            };
            if s == e {
                return bad("the screen must wake at a different time than it sleeps");
            }
            (Some(s), Some(e))
        }
        _ => return bad("set both times, or neither"),
    };

    let result = sqlx::query(
        "UPDATE app_display SET sleep_start = $1, sleep_end = $2, updated_at = now() WHERE id",
    )
    .bind(parsed.0)
    .bind(parsed.1)
    .execute(state.db.pool())
    .await;

    match result {
        Ok(_) => {
            // Land the change now, not on the engine's next slow tick.
            sleep_engine::nudge();
            Json(HoursConfig { sleep_start: parsed.0, sleep_end: parsed.1 }).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("could not save hours: {e}") })),
        )
            .into_response(),
    }
}

/// The sleep engine — Hours, enforced.
///
/// Sleep is a **precedence state below every interruption**, never a cron
/// toggle: a sleeping screen during a held case button would swallow the
/// countdown that `button_held_secs` exists to show, so the engine wakes the
/// glass for a button, an upgrade, or a storage fault, and never sleeps an
/// unclaimed box (setup must show). The duty list on the Settings page is a
/// promise; this task is where it is kept at 3am.
///
/// Mechanism (backlight audit, agents/plan/display-plan.md): the kiosk unit KEEPS
/// RUNNING; only the connector toggles. Sleep = write the
/// `/run/virtues-display-asleep` marker (the unit's ExecStartPre accepts it in
/// lieu of a connected connector, so a mid-sleep `restart_display()` after an
/// upgrade cannot park the unit) then force the connector down — the panel's
/// backlight goes off with the signal. Wake = remove the marker, force
/// `detect`, and self-heal a unit that failed anyway (`reset-failed` +
/// `start`). Re-`detect` is only mode-safe under the pinned-EDID firmware
/// override; without it the flaky DDC wire serves VESA fallbacks and the
/// glass wakes stretched.
///
/// Two lanes: a 1s lane reading only in-memory/cheap state (the clock, the
/// button atomic, the mount stat), and a 30s lane for what costs a process or
/// a query (upgrade unit, claimed). A `nudge()` (config PUT) fires the slow
/// refresh early. Everything privileged runs `sudo -n` with a fixed argv —
/// the connector name is enumerated from sysfs and character-validated, never
/// caller input.
pub mod sleep_engine {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};

    static ASLEEP: AtomicBool = AtomicBool::new(false);
    /// Connector we forced down, so wake re-detects the same one. The MARKER
    /// file carries the same name as the durable copy: a virtues.service
    /// restart mid-sleep loses this memory but not the file, and `wake_up`
    /// falls back to reading it. A reboot clears the sysfs force and the
    /// tmpfs marker together, so all state resets as one.
    static SLEPT_CONNECTOR: Mutex<Option<String>> = Mutex::new(None);

    fn nudge_notify() -> &'static tokio::sync::Notify {
        static N: OnceLock<tokio::sync::Notify> = OnceLock::new();
        N.get_or_init(tokio::sync::Notify::new)
    }

    /// Ask the engine to re-read config/state now (config PUT).
    pub fn nudge() {
        nudge_notify().notify_one();
    }

    /// Is the screen currently held dark by Hours? Read by the GET handler
    /// and the redacted mirror.
    pub fn asleep() -> bool {
        ASLEEP.load(Ordering::Relaxed)
    }

    const MARKER: &str = "/run/virtues-display-asleep";

    fn sudo_sh(script: &str) -> bool {
        std::process::Command::new("sudo")
            .args(["-n", "sh", "-c", script])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// A connector name is a path segment about to ride inside a root shell
    /// string; it came from our own sysfs enumeration, but validate anyway.
    fn connector_ok(name: &str) -> bool {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    }

    fn go_to_sleep() {
        let Some(panel) = super::connected_panel() else {
            // Nothing connected — nothing to darken, and forcing a random
            // connector off on a headless box would be pure mischief.
            return;
        };
        if !connector_ok(&panel.connector) {
            tracing::warn!(connector = %panel.connector, "sleep: refusing odd connector name");
            return;
        }
        // Marker BEFORE the force: from the moment the connector reads
        // `disconnected`, a unit (re)start must already have its escape. The
        // marker CARRIES the connector name so a successor process — an
        // upgrade restarts virtues.service while the display unit keeps
        // running — can adopt the sleep and still know what to wake.
        if !sudo_sh(&format!("echo {} > {MARKER}", panel.connector)) {
            tracing::warn!("sleep: could not write marker; staying awake");
            return;
        }
        if sudo_sh(&format!(
            "echo off > /sys/class/drm/{}/status",
            panel.connector
        )) {
            *SLEPT_CONNECTOR.lock().unwrap_or_else(|e| e.into_inner()) =
                Some(panel.connector.clone());
            ASLEEP.store(true, Ordering::Relaxed);
            tracing::info!(connector = %panel.connector, "display: asleep");
        } else {
            let _ = sudo_sh(&format!("rm -f {MARKER}"));
            tracing::warn!("sleep: connector force-off failed; staying awake");
        }
    }

    /// RETRYABLE: nothing is forgotten until the glass is provably waking.
    /// A transient sudo refusal keeps `ASLEEP` true and the marker in place,
    /// so the next 1s tick simply tries again — clearing state on a failed
    /// wake would strand the connector forced-off with an engine that
    /// believes the screen is on (and, with the marker gone, an ExecStartPre
    /// that parks the unit on the next restart).
    fn wake_up(reason: &str) {
        // The in-memory name, or the one the marker carries — a successor
        // process (upgrade restarts virtues.service; the display unit keeps
        // running) adopts the sleep and reads the connector from the file.
        let connector = SLEPT_CONNECTOR
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .or_else(|| {
                std::fs::read_to_string(MARKER)
                    .ok()
                    .map(|s| s.trim().to_string())
            })
            .filter(|c| connector_ok(c));
        match connector {
            Some(c) => {
                if !sudo_sh(&format!("echo detect > /sys/class/drm/{c}/status")) {
                    tracing::warn!("wake: detect write failed; will retry");
                    return;
                }
            }
            None => {
                // A legacy or empty marker: re-detect every connector reading
                // `disconnected`. Connected ones are left alone — a re-probe
                // on a box WITHOUT the pinned EDID is what loses the mode.
                if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
                    for e in entries.flatten() {
                        let name = e.file_name().to_string_lossy().into_owned();
                        if !name.contains('-') || !connector_ok(&name) {
                            continue;
                        }
                        let status = std::fs::read_to_string(e.path().join("status"))
                            .unwrap_or_default();
                        if status.trim() == "disconnected" {
                            let _ = sudo_sh(&format!(
                                "echo detect > /sys/class/drm/{name}/status"
                            ));
                        }
                    }
                }
            }
        }
        if !sudo_sh(&format!("rm -f {MARKER}")) {
            tracing::warn!("wake: marker removal failed; will retry");
            return;
        }
        // Self-heal: an upgrade's restart_display against the forced-off
        // connector may have parked the unit failed before the marker landed
        // (or on an old unit file without the escape). Wake fixes it.
        let active = std::process::Command::new("systemctl")
            .args(["is-active", "--quiet", "virtues-display"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !active {
            let _ = sudo_sh("systemctl reset-failed virtues-display 2>/dev/null; systemctl start virtues-display");
        }
        *SLEPT_CONNECTOR.lock().unwrap_or_else(|e| e.into_inner()) = None;
        ASLEEP.store(false, Ordering::Relaxed);
        tracing::info!(reason, "display: awake");
    }

    /// Slow-lane snapshot, refreshed every ~30s (or on nudge).
    #[derive(Clone, Copy, Default)]
    struct Slow {
        claimed: bool,
        updating: bool,
        disk_fault: bool,
        window: Option<(chrono::NaiveTime, chrono::NaiveTime)>,
        /// The owner's home timezone. The APPLIANCE'S SYSTEM CLOCK IS UTC —
        /// `Local::now()` there would run "sleeps at 22:00" at 5pm in Austin.
        /// The profile's home_timezone is the same source the face bridge
        /// sets on its SQL sessions; `None` (unset profile, unparseable name)
        /// falls back to the process-local zone, which is at least right on
        /// a dev machine.
        tz: Option<chrono_tz::Tz>,
    }

    pub fn spawn(pool: sqlx::PgPool) {
        // Appliance-only: no kiosk unit, no screen to keep hours for. A dev
        // checkout never reaches the sudo calls.
        if !std::path::Path::new(super::DISPLAY_UNIT_FILE).exists() {
            return;
        }
        // Adopt a sleep this process didn't start: the marker survives a
        // virtues.service restart (an upgrade IS one), and without adoption
        // the new process believes the glass is awake, never runs the wake
        // transition, and the connector stays forced off past the window —
        // a permanently dark panel until someone SSHes in.
        if std::path::Path::new(MARKER).exists() {
            ASLEEP.store(true, Ordering::Relaxed);
            tracing::info!("display: adopted an in-progress sleep from a previous process");
        }
        tokio::spawn(async move {
            let mut slow = Slow::default();
            let mut last_slow = std::time::Instant::now() - std::time::Duration::from_secs(60);
            loop {
                if last_slow.elapsed() >= std::time::Duration::from_secs(30) {
                    slow = refresh_slow(&pool).await;
                    last_slow = std::time::Instant::now();
                }

                let now = match slow.tz {
                    Some(tz) => chrono::Utc::now().with_timezone(&tz).time(),
                    None => chrono::Local::now().time(),
                };
                let in_window = slow
                    .window
                    .map(|(s, e)| super::window_active(now, s, e))
                    .unwrap_or(false);
                // The precedence chain, as one expression: the window only
                // wins when nothing above it is happening.
                let desired = in_window
                    && slow.claimed
                    && !slow.updating
                    && !slow.disk_fault
                    && crate::maintenance::reset_button::hold_secs().is_none();

                match (asleep(), desired) {
                    (false, true) => go_to_sleep(),
                    (true, false) => wake_up(if in_window { "interruption" } else { "morning" }),
                    _ => {}
                }

                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {}
                    _ = nudge_notify().notified() => {
                        // Config changed — refresh the slow lane immediately.
                        last_slow = std::time::Instant::now() - std::time::Duration::from_secs(60);
                    }
                }
            }
        });
    }

    async fn refresh_slow(pool: &sqlx::PgPool) -> Slow {
        let claimed = !crate::api::pair::is_unclaimed(pool).await;
        let updating = crate::api::display::upgrade_unit_active();
        let disk_fault = crate::data_disk::status().message().is_some();
        // Absent config here means "no schedule", the pre-Hours behavior —
        // same degradation contract as the face read: the engine must not
        // take a broken query as permission to darken someone's screen.
        let window = match super::try_hours(pool).await {
            Ok(h) => h.sleep_start.zip(h.sleep_end),
            Err(e) => {
                tracing::warn!(error = %e, "sleep: hours unreadable, treating as none");
                None
            }
        };
        let tz = sqlx::query_scalar::<_, Option<String>>(
            "SELECT home_timezone FROM app_user_profile LIMIT 1",
        )
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .flatten()
        .and_then(|name| name.parse::<chrono_tz::Tz>().ok());
        Slow { claimed, updating, disk_fault, window, tz }
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
    pub hours: HoursConfig,
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
    /// Hours is holding the glass dark right now. The miniature says so
    /// rather than mirroring blackness that would read as a fault.
    pub asleep: bool,
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
    let hours = match try_hours(pool).await {
        Ok(h) => h,
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
        hours,
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
            asleep: sleep_engine::asleep(),
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

#[cfg(test)]
mod tests {
    use super::window_active;
    use chrono::NaiveTime;

    fn t(s: &str) -> NaiveTime {
        NaiveTime::parse_from_str(s, "%H:%M").unwrap()
    }

    #[test]
    fn same_day_window() {
        // 13:00–15:00: a nap schedule, not that anyone would.
        assert!(!window_active(t("12:59"), t("13:00"), t("15:00")));
        assert!(window_active(t("13:00"), t("13:00"), t("15:00")));
        assert!(window_active(t("14:59"), t("13:00"), t("15:00")));
        assert!(!window_active(t("15:00"), t("13:00"), t("15:00")));
    }

    #[test]
    fn overnight_window_wraps_midnight() {
        // 22:00–07:00 — the actual product case.
        assert!(window_active(t("22:00"), t("22:00"), t("07:00")));
        assert!(window_active(t("23:59"), t("22:00"), t("07:00")));
        assert!(window_active(t("00:00"), t("22:00"), t("07:00")));
        assert!(window_active(t("06:59"), t("22:00"), t("07:00")));
        assert!(!window_active(t("07:00"), t("22:00"), t("07:00")));
        assert!(!window_active(t("12:00"), t("22:00"), t("07:00")));
        assert!(!window_active(t("21:59"), t("22:00"), t("07:00")));
    }

    #[test]
    fn degenerate_equal_times_never_sleep() {
        // Rejected at the PUT; if a row carries it anyway, the screen must
        // not be held dark forever by a schedule with no wake.
        assert!(!window_active(t("12:00"), t("08:00"), t("08:00")));
        assert!(!window_active(t("08:00"), t("08:00"), t("08:00")));
    }
}
