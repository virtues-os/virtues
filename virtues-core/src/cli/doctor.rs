//! `virtues doctor` — the box examined.
//!
//! Two ledgers (Inference, Reach) and one verdict. The editorial rules:
//!
//! 1. **Say each fact once.** The old report printed the global IPv6 three
//!    times (class line, headline, network line); here every fact has one row.
//! 2. **End with a verdict.** The last block is `✓ healthy` or the issue
//!    list — the user shouldn't have to diff ten rows against a mental model
//!    of "good" to learn that `relay: LAN-only` is the problem.
//! 3. **Every issue names its remedy** as a runnable command. Doctor is a
//!    to-do list, not a readout.
//! 4. **Exit code = diagnosis.** Errors exit 1 so `virtues doctor && …`
//!    composes; warnings (unclaimed box) stay exit 0.
//!
//! Doctor never binds the iroh endpoint and treats the DB as optional: an
//! unreadable database is itself a finding ("unknown", with the
//! run-as-the-box-user remedy), never silently-authoritative zeros.

use console::style;

use super::ui;
use crate::inference_report::{ModelSource, ResolutionReport};

/// Run the full report. Returns the process exit code.
pub async fn run() -> i32 {
    let mut issues = ui::Issues::new();
    ui::section("Doctor");
    print_inference(&crate::inference_report::resolution_report(), &mut issues);
    print_reach(&mut issues).await;
    issues.verdict()
}

/// The Inference ledger. Shared with `virtues warm-models`, which prints it
/// before pulling anything so the user sees what's about to be exercised.
pub fn print_inference(r: &ResolutionReport, issues: &mut ui::Issues) {
    ui::subsection("Inference");
    ui::kv(
        "accelerator",
        &format!("{} (GPU or CPU per sidecar build)", r.accelerator),
    );
    ui::kv("precision", &r.precision);
    match &r.models_dir {
        Some(d) => ui::kv("models dir", &d.display().to_string()),
        None => ui::kv("models dir", "unset"),
    }
    for m in &r.models {
        // The on-disk path is models dir + gguf file — both already on
        // screen — so the row carries status, not the path again.
        let value = match &m.source {
            ModelSource::Baked(_) => {
                format!("{}  {}  {}", m.repo, m.gguf_file, style("✓").green())
            }
            ModelSource::Download => {
                issues.error(
                    format!("model not on disk: {}", m.gguf_file),
                    Some("re-run the installer to fetch it: curl -fsSL https://virtues.com/sh | sudo sh"),
                );
                format!(
                    "{}  {}  {}",
                    m.repo,
                    m.gguf_file,
                    style("✖ missing").red()
                )
            }
        };
        ui::kv(m.name, &value);
    }
}

/// The Reach ledger: local network facts, then the iroh legs read from the DB.
async fn print_reach(issues: &mut ui::Issues) {
    let net = crate::net_check::compute_net_status();
    ui::subsection("Reach");
    match net.ipv4_source {
        Some(a) => ui::kv("lan", &a.to_string()),
        None => ui::kv("lan", "none"),
    }
    match net.ipv6_global {
        Some(a) => ui::kv("ipv6", &format!("{a} (global)")),
        None => ui::kv("ipv6", "none"),
    }
    if let Some(b) = &net.byo {
        let addr = b.addr.map(|a| format!(" ({a})")).unwrap_or_default();
        ui::kv("byo", &format!("{}{addr} — devices can dial this address", b.ifname));
    }
    if matches!(net.class, crate::net_check::NetClass::Unknown) {
        issues.error(
            "no internet connection detected",
            Some("check the box's network link, then re-run: virtues doctor"),
        );
    }

    // The iroh legs (endpoint id, relay home, allowlist) live in the DB;
    // doctor runs in a separate process that never binds the endpoint.
    let report = match crate::setup::recommended_config()
        .ok()
        .and_then(|cfg| crate::database::Database::new(&cfg.database_url).ok())
    {
        Some(db) => crate::relay::reach_report(db.pool()).await,
        None => crate::relay::ReachReport {
            db_reachable: false,
            endpoint_id: None,
            relay_url: None,
            allowlisted_devices: 0,
        },
    };

    if !report.db_reachable {
        ui::kv("iroh node", "unknown");
        ui::kv("relay", "unknown");
        ui::kv("devices", "unknown");
        issues.error(
            "couldn't read the box database — reach state unknown",
            Some("run as the box user: sudo -u virtues virtues doctor"),
        );
        return;
    }

    match &report.endpoint_id {
        Some(eid) => {
            // Middle-ellipsize on a TTY (head+tail is how humans compare
            // keys); piped output keeps the full id for support pastes.
            let shown = if ui::tty() {
                ui::ellipsize_middle(eid, 24)
            } else {
                eid.clone()
            };
            ui::kv("iroh node", &shown);
        }
        None => {
            ui::kv("iroh node", "not provisioned");
            issues.warn(
                "iroh identity not provisioned yet (first boot mints it)",
                Some("start the service: sudo systemctl start virtues"),
            );
        }
    }
    match &report.relay_url {
        Some(u) => ui::kv("relay", u),
        None => {
            ui::kv("relay", "LAN-only");
            issues.warn(
                "no relay configured — the box is reachable on this network only (unclaimed, or atlas unreachable)",
                Some("claim the box: virtues pair"),
            );
        }
    }
    ui::kv("devices", &format!("{} paired", report.allowlisted_devices));
    // 0 devices while unclaimed shares a root cause with the relay warning
    // above, so it only becomes its own finding once the relay leg is fine.
    if report.allowlisted_devices == 0 && report.relay_url.is_some() {
        issues.warn(
            "no devices on the allowlist",
            Some("pair one: virtues device add"),
        );
    }
}
