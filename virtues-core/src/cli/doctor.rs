//! `virtues doctor` — the box examined.
//!
//! Three ledgers (Inference, Reach, Appliance) and one verdict. The last is
//! printed only on hardware we shipped. The editorial rules:
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
    probe_inference(&mut issues).await;
    print_reach(&mut issues).await;
    print_appliance(&mut issues);
    issues.verdict()
}

/// The Appliance ledger — the physical box, as opposed to the software on it.
///
/// Doctor answered two questions (Inference, Reach) and none of the ones an
/// appliance owner actually gets stuck on. When someone says "my box is
/// broken", this is the tool they are told to run, and until now it could not
/// say whether the data disk was mounted, whether the display was running,
/// whether the button did anything, or where the database physically lived.
///
/// Prints NOTHING on a DIY box. Every row here describes hardware we shipped,
/// and on somebody's own Linux server each one would be either meaningless or
/// a false alarm — a state root that is a plain directory is correct there, and
/// there is no panel and no button to report on.
fn print_appliance(issues: &mut ui::Issues) {
    let Some(m) = crate::install_manifest::get().as_ref() else {
        return;
    };
    if !m.appliance {
        return;
    }
    ui::subsection("Appliance");
    ui::kv("profile", &m.profile);

    // THE DATA DISK. First row because it is the one failure that looks like
    // health: a box whose NVMe never mounted runs, serves, and quietly writes
    // the owner's record to the boot card. See `crate::data_disk`.
    match crate::data_disk::status() {
        crate::data_disk::DataDisk::Mounted => ui::kv("data disk", "mounted"),
        crate::data_disk::DataDisk::OnRoot => ui::kv("data disk", "on the root filesystem"),
        crate::data_disk::DataDisk::Missing => {
            ui::kv("data disk", "MISSING");
            issues.error(
                "the data disk is not mounted — the record belongs on it, and                  Postgres is configured to refuse to start without it",
                Some("reseat or replace the NVMe, then: sudo systemctl reboot"),
            );
        }
    }

    // Where the database actually is. A relocated cluster is a symlink into the
    // data disk; an appliance without one is writing every transaction to the
    // boot card, which is the thing the layout exists to prevent and is
    // invisible from every other surface.
    match crate::cli::deprovision::relocated_cluster_dir() {
        Some(d) => ui::kv("postgres", &format!("{} (on the data disk)", d.display())),
        None => {
            ui::kv("postgres", "on the boot medium");
            issues.warn(
                "the Postgres cluster was never moved to the data disk — every                  write lands on the boot card, which wears out under database load",
                Some("re-run the installer to relocate it"),
            );
        }
    }

    // The panel and the button: the only two interfaces an owner has before a
    // device is paired, and both fail silently by nature — a dead kiosk is a
    // black screen, and a button nobody wired is indistinguishable from one
    // nobody pressed.
    ui::kv("display", unit_state("virtues-display"));
    ui::kv(
        "case button",
        if std::path::Path::new("/etc/systemd/logind.conf.d/10-virtues-power-key.conf").exists() {
            "armed (hold 3s to forget devices)"
        } else {
            "not armed — logind still owns the power key"
        },
    );

    // Only interesting before imaging, but this is where someone looks.
    let applets = crate::cli::deprovision::authored_applets_dir();
    if let Ok(n) = std::fs::read_dir(&applets).map(|d| d.flatten().count()) {
        if n > 0 {
            ui::kv("authored applets", &format!("{n} (owner-written; not shippable)"));
        }
    }
}

/// `active` / `inactive` / `not installed`, for a unit that may legitimately be
/// absent — a headless appliance has no display.
fn unit_state(unit: &str) -> &'static str {
    if !std::path::Path::new(&format!("/etc/systemd/system/{unit}.service")).exists() {
        return "not installed";
    }
    match std::process::Command::new("systemctl")
        .args(["is-active", "--quiet", unit])
        .status()
    {
        Ok(s) if s.success() => "active",
        _ => "installed but not running",
    }
}

/// The Inference ledger. Shared with `virtues warm-models`, which prints it
/// before pulling anything so the user sees what's about to be exercised.
pub fn print_inference(r: &ResolutionReport, issues: &mut ui::Issues) {
    ui::subsection("Inference");
    ui::kv("accelerator", &r.accelerator);
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

/// The liveness half of the Inference ledger: is anything actually SERVING?
///
/// `print_inference` above reports on-disk model artifacts, which is a different
/// question and was for a long time the only one doctor asked. A box whose NPU
/// daemon had been crash-looping for ten days still printed two green ticks,
/// because both context binaries were exactly where the installer had put them.
/// Files on disk are necessary and nowhere near sufficient, and a ✓ that can't
/// tell the difference is worse than no row at all — it is what someone reads
/// when they go looking for the cause of failing search.
///
/// `/health` is the honest signal, and it is uniform across both inference
/// flavors: llama-server returns 200 once the GGUF is loaded, and `virtues-qnnd`
/// implements the same route by running a real embed through the Hexagon
/// (`crates/virtues-qnnd/src/http.rs`). Either way 200 means "this endpoint can
/// answer right now", which is the thing being claimed.
///
/// Errors, not warnings: with no embed endpoint there is no indexing and no
/// semantic retrieval, and doctor's exit code should say so.
async fn probe_inference(issues: &mut ui::Issues) {
    crate::http_client::ensure_crypto_provider();
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            ui::kv("endpoints", &format!("{} unprobed ({e})", style("⚠").yellow()));
            return;
        }
    };

    // The unit that owns these endpoints differs by profile, and a remedy that
    // names the wrong service is a remedy that wastes someone's evening.
    let dragon = crate::inference_report::is_dragon_profile();

    // Row labels stay inside `ui::kv`'s 12-column leader so the values line up
    // with the rows above them; the prose noun for the issue list is separate,
    // because "embed live answered 503" is not a sentence.
    for (label, noun, base, unit) in [
        (
            "embed live",
            "embed endpoint",
            endpoint("VIRTUES_EMBED_URL", crate::search::embedder::resolve_base_url()),
            if dragon { "virtues-qnnd" } else { "virtues-embed" },
        ),
        (
            "rerank live",
            "rerank endpoint",
            endpoint("VIRTUES_RERANK_URL", crate::search::reranker::resolve_base_url()),
            if dragon { "virtues-qnnd" } else { "virtues-rerank" },
        ),
    ] {
        let health = format!("{base}/health");
        // Distinguish "nothing is listening" from "listening but not ready":
        // the first is a dead unit, the second is a model that won't load, and
        // they send the operator to different places.
        let (mark, finding) = match client.get(&health).send().await {
            Ok(r) if r.status().is_success() => (style("✓ serving").green().to_string(), None),
            Ok(r) => (
                style(format!("✖ unhealthy ({})", r.status())).red().to_string(),
                Some(format!("{noun} answered {} at {base}", r.status())),
            ),
            Err(_) => (
                style("✖ not serving").red().to_string(),
                Some(format!("nothing serving at {base}")),
            ),
        };
        ui::kv(label, &format!("{base}  {mark}"));
        if let Some(what) = finding {
            issues.error(
                format!("{what} — semantic search and reranking are unavailable"),
                Some(&format!("systemctl status {unit}; journalctl -u {unit} -n 50")),
            );
        }
    }
}

/// The endpoint doctor should probe.
///
/// The search modules' `resolve_base_url` reads only the process environment.
/// That is right for the server, which systemd starts with the box
/// `EnvironmentFile`, and wrong here: `sudo virtues doctor` inherits none of it,
/// so a box pointed at a remote endpoint would be probed at the loopback
/// default and reported broken. Fall back to the box env file the way `upgrade`
/// already does for its own paths.
fn endpoint(key: &str, from_process_env: String) -> String {
    if std::env::var(key).is_ok() {
        return from_process_env;
    }
    super::upgrade::read_box_env_var(key)
        .map(|s| s.trim_end_matches('/').to_string())
        .unwrap_or(from_process_env)
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
