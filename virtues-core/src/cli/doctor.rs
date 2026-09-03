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
    // `install_manifest::appliance()`, not the raw field: a box installed before
    // that field existed has `None`, which means "no opinion", not "DIY". See
    // that function for why the difference matters after an upgrade.
    if !crate::install_manifest::appliance() {
        return;
    }
    let Some(m) = crate::install_manifest::get().as_ref() else {
        return;
    };
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
/// `/health` is the cheap signal, and it is uniform across both inference
/// flavors we ship: llama-server returns 200 once the GGUF is loaded, and
/// `virtues-qnnd` implements the same route by running a real embed through the
/// Hexagon (`crates/virtues-qnnd/src/http.rs`). Either way 200 means "this
/// endpoint can answer right now", which is the thing being claimed.
///
/// It is not, however, the *only* signal, and treating it as one was a bug: the
/// route is llama-server's, not part of the OpenAI shape. Ollama — which the
/// installer recommends first for NVIDIA and x86 CPU — answers 404 on `/health`
/// and serves `/v1/embeddings` flawlessly, so a perfectly working box read as
/// `✖ not serving`. A row that condemns a healthy endpoint sends someone to
/// restart a unit that was never broken.
///
/// So three states, not two. 2xx is serving. A non-2xx (or a `/health` that
/// isn't there to answer) falls through to the work the endpoint actually
/// exists to do — a one-input embed, a two-document rerank — and that verdict
/// is the row. Only when both fail is the endpoint down.
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
    // The fallback probe runs real inference, so it needs a real budget — a
    // cold CPU model can spend several seconds on its first token. Only built
    // because a separate timeout is the whole point; the 3s client above stays
    // right for `/health`, which is meant to be instant.
    let work_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .ok();

    // The unit that owns these endpoints differs by profile, and a remedy that
    // names the wrong service is a remedy that wastes someone's evening.
    let dragon = crate::inference_report::is_dragon_profile();

    // Row labels stay inside `ui::kv`'s 12-column leader so the values line up
    // with the rows above them; the prose noun for the issue list is separate,
    // because "embed live answered 503" is not a sentence.
    for (label, noun, base, unit, work) in [
        (
            "embed live",
            "embed endpoint",
            endpoint("VIRTUES_EMBED_URL", crate::search::embedder::resolve_base_url()),
            if dragon { "virtues-qnnd" } else { "virtues-embed" },
            Work::Embed,
        ),
        (
            "rerank live",
            "rerank endpoint",
            endpoint("VIRTUES_RERANK_URL", crate::search::reranker::resolve_base_url()),
            if dragon { "virtues-qnnd" } else { "virtues-rerank" },
            Work::Rerank,
        ),
    ] {
        let health = format!("{base}/health");
        // Distinguish "nothing is listening" from "listening but not ready"
        // from "listening, working, and simply has no /health route": the first
        // is a dead unit, the second is a model that won't load, and the third
        // is a healthy Ollama. They send the operator to three different
        // places, one of which is "nowhere, it's fine".
        let health_answer = client.get(&health).send().await;
        let healthy = matches!(&health_answer, Ok(r) if r.status().is_success());
        let (mark, finding) = if healthy {
            (style("✓ serving").green().to_string(), None)
        } else {
            // `/health` didn't vouch for it. Ask it to do the actual job.
            match work_probe(work_client.as_ref(), &base, &work).await {
                Ok(()) => (
                    style("✓ serving (no /health)").green().to_string(),
                    None,
                ),
                Err(why) => match &health_answer {
                    Ok(r) => (
                        style(format!("✖ unhealthy ({})", r.status())).red().to_string(),
                        Some(format!(
                            "{noun} answered {} at {base} and {why}",
                            r.status()
                        )),
                    ),
                    Err(_) => (
                        style("✖ not serving").red().to_string(),
                        Some(format!("nothing serving at {base} ({why})")),
                    ),
                },
            }
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

/// What an endpoint is for, so the fallback probe can ask it to do that.
enum Work {
    Embed,
    Rerank,
}

/// Ask the endpoint to do its actual job.
///
/// This is the authoritative liveness answer — `/health` is only a shortcut to
/// it — and it is the same call `HttpEmbedder::new` makes at startup, so a
/// green row here means the server will come up rather than merely that a
/// socket is open.
///
/// Deliberately cheap (one short input; two one-character documents) and
/// deliberately shaped like the installer's setup probes
/// (`mode.rs::validate_manual`, `probe_rerank`), so setup and doctor cannot
/// disagree about the same server. The body is inspected, not just the status:
/// a 200 carrying an error object is a shape we have shipped before, and it
/// must not read as green.
async fn work_probe(
    client: Option<&reqwest::Client>,
    base: &str,
    work: &Work,
) -> Result<(), String> {
    let Some(client) = client else {
        return Err("its probe client could not be built".to_string());
    };
    let (path, body) = match work {
        Work::Embed => (
            "/v1/embeddings",
            serde_json::json!({ "input": ["doctor probe"], "model": embed_model() }),
        ),
        Work::Rerank => (
            "/v1/rerank",
            serde_json::json!({
                "model": "default",
                "query": "probe",
                "documents": ["a", "b"],
                "top_n": 2,
            }),
        ),
    };

    let resp = match client.post(format!("{base}{path}")).json(&body).send().await {
        Ok(r) if r.status().is_success() => r,
        Ok(r) => return Err(format!("its {path} probe answered {}", r.status())),
        Err(e) => return Err(format!("its {path} probe failed ({e})")),
    };
    let payload: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return Err(format!("its {path} probe returned unparseable JSON ({e})")),
    };

    let served = match work {
        // Accepts both OpenAI-style shapes the embedder does: a `data` array of
        // rows, or a bare top-level row array.
        Work::Embed => payload
            .get("data")
            .and_then(|d| d.as_array())
            .or_else(|| payload.as_array())
            .and_then(|rows| rows.first())
            .and_then(|row| row.get("embedding"))
            .and_then(|e| e.as_array())
            .is_some_and(|v| !v.is_empty()),
        Work::Rerank => payload
            .get("results")
            .and_then(|r| r.as_array())
            .is_some_and(|r| !r.is_empty()),
    };
    if served {
        Ok(())
    } else {
        Err(format!("its {path} probe returned no usable result"))
    }
}

/// The routing key doctor's embed probe should send. Ignored by llama.cpp
/// (it serves whatever it loaded) and load-bearing for Ollama-style servers
/// that route by model name — so it is resolved from the same two places as
/// the URLs, for the same reason `endpoint` exists.
fn embed_model() -> String {
    std::env::var("VIRTUES_EMBED_MODEL")
        .ok()
        .or_else(|| super::upgrade::read_box_env_var("VIRTUES_EMBED_MODEL"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "default".to_string())
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
