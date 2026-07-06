//! The actual install steps — typed ports of bash install.sh's functions.
//!
//! Each function is one logical install operation that the bash version
//! handled inside a `step "Label" cmd` invocation. The cliclack spinner
//! wraps the shell-out; on success we print a `✓ Label`, on failure we
//! surface the last lines of output with full context.
//!
//! Every step is idempotent — running the installer twice on a working
//! box must converge, never regress. Most idempotency comes from the
//! underlying CLIs (apt knows the package is installed; systemctl
//! enable is a no-op on enabled units); a few we guard explicitly
//! (env file: skip if exists, never rotate the encryption key).

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio::process::Command;

use crate::config::InstallConfig;
use crate::mode::{InferenceMode, ValidationReport};
use crate::steps::{run_step, PkgMgr, Target};
use crate::ui;

// ────────────────────────────────────────────────────────────────────────
// System dependencies (apt/dnf): Postgres, Avahi, ca-certs
// ────────────────────────────────────────────────────────────────────────

pub async fn install_deps(target: &Target) -> Result<()> {
    match target.pkg_mgr {
        PkgMgr::Apt => install_deps_apt(target).await,
        PkgMgr::Dnf => install_deps_dnf().await,
    }
}

async fn install_deps_apt(target: &Target) -> Result<()> {
    // apt-get update before anything; needed to refresh package lists.
    let mut cmd = apt();
    cmd.args(["update", "-qq"]);
    run_step("apt index", cmd).await?;

    // PGDG repo for Postgres 18 on Ubuntu 22.04 + 24.04 (default repos
    // ship older PGs). add_pgdg_repo() is idempotent.
    add_pgdg_repo(target).await?;

    apt_install("Postgres 18 + pgvector", &["postgresql-18", "postgresql-18-pgvector"]).await?;
    apt_install("Avahi (mDNS)", &["avahi-daemon", "avahi-utils", "libnss-mdns"]).await?;
    apt_install("ca-certificates + curl", &["ca-certificates", "curl"]).await?;

    systemctl(&["enable", "--now", "postgresql"], "Enable postgresql").await?;
    systemctl(&["enable", "--now", "avahi-daemon"], "Enable avahi-daemon").await?;
    Ok(())
}

async fn install_deps_dnf() -> Result<()> {
    dnf_install("Postgres + pgvector", &["postgresql-server", "postgresql-contrib", "pgvector"]).await?;
    dnf_install("Avahi (mDNS)", &["avahi", "nss-mdns"]).await?;
    dnf_install("ca-certificates + curl", &["ca-certificates", "curl"]).await?;

    if !Path::new("/var/lib/pgsql/data/base").exists() {
        let mut cmd = Command::new("postgresql-setup");
        cmd.arg("--initdb");
        run_step("Init Postgres cluster", cmd).await?;
    }

    systemctl(&["enable", "--now", "postgresql"], "Enable postgresql").await?;
    systemctl(&["enable", "--now", "avahi-daemon"], "Enable avahi-daemon").await?;
    Ok(())
}

async fn add_pgdg_repo(target: &Target) -> Result<()> {
    // Only add PGDG on Ubuntu versions where default repos ship < PG18.
    let needs_pgdg = matches!(
        (target.distro.as_str(), target.distro_version.as_str()),
        ("ubuntu", "22.04" | "24.04" | "25.04" | "25.10"),
    );
    if !needs_pgdg {
        return Ok(());
    }

    apt_install("apt key tooling", &["curl", "ca-certificates", "lsb-release", "gnupg"]).await?;

    fs::create_dir_all("/usr/share/postgresql-common/pgdg")
        .context("creating /usr/share/postgresql-common/pgdg")?;

    let mut cmd = Command::new("curl");
    cmd.args([
        "-fsSL",
        "https://www.postgresql.org/media/keys/ACCC4CF8.asc",
        "-o",
        "/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc",
    ]);
    run_step("PGDG signing key", cmd).await?;

    // Get the distro codename for the PGDG repo URL.
    let codename = lsb_codename().await?;
    let line = format!(
        "deb [signed-by=/usr/share/postgresql-common/pgdg/apt.postgresql.org.asc] \
         https://apt.postgresql.org/pub/repos/apt {codename}-pgdg main\n"
    );
    fs::write("/etc/apt/sources.list.d/pgdg.list", line)
        .context("writing /etc/apt/sources.list.d/pgdg.list")?;

    let mut cmd = apt();
    cmd.args(["update", "-qq"]);
    run_step("Refresh apt index w/ PGDG", cmd).await
}

async fn lsb_codename() -> Result<String> {
    let out = Command::new("lsb_release").args(["-cs"]).output().await?;
    if !out.status.success() {
        return Err(anyhow!("lsb_release -cs failed"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn apt() -> Command {
    let mut c = Command::new("apt-get");
    c.env("DEBIAN_FRONTEND", "noninteractive");
    c
}

async fn apt_install(label: &str, pkgs: &[&str]) -> Result<()> {
    let mut cmd = apt();
    cmd.args(["install", "-y", "-qq"]);
    cmd.args(pkgs);
    run_step(label, cmd).await
}

async fn dnf_install(label: &str, pkgs: &[&str]) -> Result<()> {
    let mut cmd = Command::new("dnf");
    cmd.args(["install", "-y", "-q"]);
    cmd.args(pkgs);
    run_step(label, cmd).await
}

async fn systemctl(args: &[&str], label: &str) -> Result<()> {
    let mut cmd = Command::new("systemctl");
    cmd.args(args);
    run_step(label, cmd).await
}

// ────────────────────────────────────────────────────────────────────────
// Inference sidecars — llama-server hosting the embed + rerank GGUFs
// ────────────────────────────────────────────────────────────────────────

/// Embedding sidecar (:18181) and rerank sidecar (:18182): two llama-server
/// units, one model each. The binary comes out of the release tarball
/// (download.rs put it at `cfg.llama_binary_path()`); the GGUFs come from
/// the pinned models release, SHA-verified. v0.1.0 ran Ollama here — it
/// had no rerank endpoint, and its `curl | sh` installer + mutable model
/// registry were the opposite of an appliance's pin-everything posture.
pub async fn install_inference(cfg: &InstallConfig) -> Result<()> {
    let bin = cfg.llama_binary_path();
    if !bin.exists() {
        return Err(anyhow!(
            "llama-server not found at {} — tarball predates the v0.1.1 inference sidecars?",
            bin.display()
        ));
    }

    // Fetch both GGUFs (skips any already on disk — they're SHA-verified
    // at download time and immutable afterwards).
    let models_dir = cfg.models_dir();
    fs::create_dir_all(&models_dir)
        .with_context(|| format!("creating {}", models_dir.display()))?;
    for gguf in [&cfg.embed_gguf, &cfg.rerank_gguf] {
        crate::download::fetch_model(cfg, gguf).await?;
    }
    // The sidecars run as `virtues` (created earlier in the flow); the
    // GGUFs only need to be readable, but keep ownership uniform.
    let mut cmd = Command::new("chown");
    cmd.args(["-R", "virtues:virtues", models_dir.to_str().unwrap()]);
    let _ = cmd.output().await;

    // GPU offload depends on the sidecar user being in the host's GPU groups
    // (create_user added `virtues` to them); the hardened unit drops all caps
    // and sets NoNewPrivileges, so the membership only takes effect at runtime
    // if the unit also declares it. Emit a SupplementaryGroups= line for the
    // groups that exist, or nothing on a CPU-only host.
    let supp_groups = match gpu_access_groups().await.as_slice() {
        [] => String::new(),
        groups => format!("SupplementaryGroups={}\n", groups.join(" ")),
    };

    // Write + (re)start one unit per model. restart rather than just
    // enable --now so an installer re-run picks up unit/binary changes.
    for (unit, template, gguf) in [
        ("virtues-embed", EMBED_UNIT_TEMPLATE, &cfg.embed_gguf),
        ("virtues-rerank", RERANK_UNIT_TEMPLATE, &cfg.rerank_gguf),
    ] {
        let body = template
            .replace("__SUPP_GROUPS__", &supp_groups)
            .replace("__BIN__", &bin.display().to_string())
            .replace("__MODEL__", &models_dir.join(gguf).display().to_string());
        fs::write(format!("/etc/systemd/system/{unit}.service"), body)
            .with_context(|| format!("writing {unit}.service"))?;
    }
    let mut cmd = Command::new("systemctl");
    cmd.arg("daemon-reload");
    run_step("Install inference sidecar units", cmd).await?;
    let mut cmd = Command::new("systemctl");
    cmd.args(["enable", "virtues-embed", "virtues-rerank"]);
    run_step("Enable inference sidecars", cmd).await?;
    let mut cmd = Command::new("systemctl");
    cmd.args(["restart", "virtues-embed", "virtues-rerank"]);
    run_step("Start inference sidecars", cmd).await
}

/// The standard Linux groups that gate access to GPU device nodes
/// (`/dev/dri/*` and friends). The llama-server sidecars run unprivileged as
/// `virtues`; without membership in these groups the GPU backend can't open
/// its device nodes and init fails, so llama.cpp silently falls back to CPU.
/// This exists for the Dragon image's GPU (offload via Vulkan/OpenCL — the
/// mechanism is generic). We only wire up groups that actually exist on the
/// host: a CPU-only host has neither and correctly stays on CPU.
async fn gpu_access_groups() -> Vec<&'static str> {
    let mut groups = Vec::new();
    for g in ["video", "render"] {
        let exists = Command::new("getent")
            .args(["group", g])
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);
        if exists {
            groups.push(g);
        }
    }
    groups
}

/// Shared shape: loopback-only, runs as `virtues`, read-only filesystem
/// (the GGUF is mmap'd read-only; PrivateTmp covers scratch).
///
/// Flags:
/// - **embed → `-ngl 0` (CPU), rerank → `-ngl 99` (GPU).** The two workloads
///   want opposite hardware: EmbeddingGemma's activations can't run fp16, so
///   fp16 GPU paths force fp32 and are *slower than CPU* (and CPU is fine
///   for background embedding). gte-modernbert reranking is markedly faster
///   on the Dragon image's GPU. `-ngl 99` is a no-op on a CPU-only build and
///   needs the GPU `SupplementaryGroups=` below or backend init silently
///   falls back to CPU.
///   `--pooling mean` (EmbeddingGemma) / `--pooling rank` (cross-encoder).
/// - `-c/-b/-ub 2048` right-sizes context. Both models do longer, but our
///   chunks are ≤512 tok and rerank docs are capped at ~256, so 2048 is ample
///   and 8K would just bloat KV + compute buffers (~0.5 GB) for unused reach.
/// - `-np 1`: single-tenant box; 1 slot vs the auto-4 saves ~0.9 GB of
///   per-slot buffers (the bigger memory win). Concurrent requests queue,
///   which is fine here.
/// - `--cache-ram 0`: disables the prompt cache (an up-to-8 GB reservation)
///   — useless for embed/rerank where every input is unique.
/// Together these cut each sidecar from ~2.5 GB RSS to ~1 GB, which is what
/// leaves the Dragon's unified memory pool room for `-ngl 99` to fit.
/// `__SUPP_GROUPS__` is replaced at install time with a `SupplementaryGroups=`
/// line for whatever GPU groups exist (see `gpu_access_groups`), or removed
/// entirely on a CPU-only host — an undefined supplementary group would make
/// systemd fail the unit (216/GROUP), which is worse than CPU fallback.
const EMBED_UNIT_TEMPLATE: &str = r#"[Unit]
Description=Virtues embedding sidecar (llama-server, embeddinggemma-300m)
Documentation=https://virtues.com/docs
After=network.target

[Service]
Type=simple
User=virtues
Group=virtues
__SUPP_GROUPS__ExecStart=__BIN__ --embedding --pooling mean -m __MODEL__ --host 127.0.0.1 --port 18181 -c 2048 -b 2048 -ub 2048 -np 1 --cache-ram 0 -ngl 0
Restart=on-failure
RestartSec=5

NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictSUIDSGID=true
LockPersonality=true
SystemCallArchitectures=native
CapabilityBoundingSet=

[Install]
WantedBy=multi-user.target
"#;

const RERANK_UNIT_TEMPLATE: &str = r#"[Unit]
Description=Virtues rerank sidecar (llama-server, gte-reranker-modernbert-base)
Documentation=https://virtues.com/docs
After=network.target

[Service]
Type=simple
User=virtues
Group=virtues
__SUPP_GROUPS__ExecStart=__BIN__ --rerank --pooling rank -m __MODEL__ --host 127.0.0.1 --port 18182 -c 2048 -b 2048 -ub 2048 -np 1 --cache-ram 0 -ngl 99
Restart=on-failure
RestartSec=5

NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true
ProtectKernelTunables=true
ProtectControlGroups=true
RestrictSUIDSGID=true
LockPersonality=true
SystemCallArchitectures=native
CapabilityBoundingSet=

[Install]
WantedBy=multi-user.target
"#;

// ────────────────────────────────────────────────────────────────────────
// Locale — every box surface assumes UTF-8
// ────────────────────────────────────────────────────────────────────────

/// Ensure the system locale is UTF-8 (`C.UTF-8` — built into glibc on every
/// target distro; no `locales` package or locale-gen needed).
///
/// This is an install step, not a render-time fallback, on purpose: the
/// brand mark (`∴`), the half-block QR in the setup handoff, and the
/// box-drawing rules are all multibyte UTF-8, and a `LANG=C` minimal image
/// (or a tmux/screen started under one) renders them as diamonds/mojibake
/// at the worst possible moment — onboarding. Provisioning the locale once
/// keeps every render path single-path. (A terminal that still mangles
/// glyphs after this has a client-side font problem the box can't fix.)
pub async fn ensure_utf8_locale() -> Result<()> {
    // `locale charmap` reports the charmap of the locale that ACTUALLY
    // resolved — not what the env claims. The distinction matters: ssh
    // forwards the client's LANG (e.g. en_US.UTF-8), but if the box never
    // generated that locale, setlocale silently falls back to C and
    // locale-aware programs (tmux, less) mangle multibyte output while the
    // env still says "UTF-8". Env sniffing is only the fallback for images
    // without the `locale` binary.
    let session_is_utf8 = match tokio::process::Command::new("locale")
        .arg("charmap")
        .output()
        .await
    {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim().eq_ignore_ascii_case("utf-8")
        }
        _ => ["LC_ALL", "LC_CTYPE", "LANG"]
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
            .map(|v| v.to_ascii_lowercase().replace('-', "").contains("utf8"))
            .unwrap_or(false),
    };
    if session_is_utf8 {
        ui::skip("Locale already UTF-8");
        return Ok(());
    }

    // Persist the system default. localectl writes /etc/default/locale (or
    // /etc/locale.conf); fall back to writing the file directly on images
    // without it (containers).
    let mut cmd = Command::new("localectl");
    cmd.args(["set-locale", "LANG=C.UTF-8"]);
    if run_step("System locale → C.UTF-8", cmd).await.is_err() {
        fs::write("/etc/default/locale", "LANG=C.UTF-8\n")
            .context("writing /etc/default/locale")?;
        ui::ok("System locale → C.UTF-8 (wrote /etc/default/locale)");
    }

    // Make THIS run consistent too: the vars survive the exec into
    // `virtues init` (sudo's default env_keep preserves LANG/LC_*), so the
    // very first handoff/QR already renders under a UTF-8 locale. LC_ALL too:
    // a forwarded-but-ungenerated LC_ALL would otherwise override LANG.
    std::env::set_var("LANG", "C.UTF-8");
    std::env::set_var("LC_ALL", "C.UTF-8");
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────
// mDNS — hostname + Avahi service advertisement
// ────────────────────────────────────────────────────────────────────────

pub async fn configure_mdns() -> Result<()> {
    let current = hostname().await?;
    if current != "virtues" {
        if std::env::var("VIRTUES_KEEP_HOSTNAME").as_deref() == Ok("1") {
            ui::warn(&format!(
                "Keeping hostname '{current}' (VIRTUES_KEEP_HOSTNAME=1). \
                 Reachable at https://{current}.local, not virtues.local."
            ));
        } else {
            let mut cmd = Command::new("hostnamectl");
            cmd.args(["set-hostname", "virtues"]);
            run_step(&format!("Hostname → virtues (was {current})"), cmd).await?;
        }
    } else {
        ui::skip("Hostname already 'virtues'");
    }

    fs::create_dir_all("/etc/avahi/services").context("/etc/avahi/services")?;
    fs::write("/etc/avahi/services/virtues.service", AVAHI_SERVICE)
        .context("writing avahi service")?;

    let mut cmd = Command::new("bash");
    cmd.args([
        "-c",
        "systemctl reload avahi-daemon 2>/dev/null || systemctl restart avahi-daemon",
    ]);
    run_step("Advertise _http._tcp via avahi-daemon", cmd).await
}

async fn hostname() -> Result<String> {
    let out = Command::new("hostnamectl").args(["--static"]).output().await?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        let h = Command::new("hostname").output().await?;
        Ok(String::from_utf8_lossy(&h.stdout).trim().to_string())
    }
}

const AVAHI_SERVICE: &str = r#"<?xml version="1.0" standalone='no'?>
<!DOCTYPE service-group SYSTEM "avahi-service.dtd">
<service-group>
  <name replace-wildcards="yes">Virtues on %h</name>
  <service>
    <type>_http._tcp</type>
    <port>8000</port>
    <txt-record>path=/</txt-record>
    <txt-record>service=virtues</txt-record>
  </service>
</service-group>
"#;

// ────────────────────────────────────────────────────────────────────────
// System user + data directory
// ────────────────────────────────────────────────────────────────────────

pub async fn create_user(cfg: &InstallConfig) -> Result<()> {
    let id_out = Command::new("id").arg("-u").arg("virtues").output().await?;
    if id_out.status.success() {
        ui::skip("System user 'virtues' already exists");
    } else {
        let mut cmd = Command::new("useradd");
        cmd.args([
            "--system",
            "--home-dir",
            cfg.data_dir.to_str().unwrap(),
            "--shell",
            "/usr/sbin/nologin",
            "virtues",
        ]);
        run_step("Create system user 'virtues'", cmd).await?;
    }

    // Add `virtues` to the host's GPU-access groups so the inference sidecars
    // can reach the GPU device nodes (else they silently run on CPU — see
    // gpu_access_groups + the sidecar units). Runs on both the fresh and
    // already-exists paths so upgrades of older boxes pick it up; `usermod -aG`
    // is additive and idempotent. No-op on a CPU-only host (no such groups).
    let gpu_groups = gpu_access_groups().await;
    if !gpu_groups.is_empty() {
        let mut cmd = Command::new("usermod");
        cmd.args(["-aG", &gpu_groups.join(","), "virtues"]);
        run_step(
            &format!("Grant 'virtues' GPU access ({})", gpu_groups.join(", ")),
            cmd,
        )
        .await?;
    }

    for sub in &["lake", "models", "secrets"] {
        fs::create_dir_all(cfg.data_dir.join(sub))
            .with_context(|| format!("creating {}/{sub}", cfg.data_dir.display()))?;
    }
    // chown -R virtues:virtues + 0700 on secrets
    let mut cmd = Command::new("chown");
    cmd.args([
        "-R",
        "virtues:virtues",
        cfg.data_dir.to_str().unwrap(),
    ]);
    run_step("chown data dir", cmd).await?;

    let secrets = cfg.data_dir.join("secrets");
    fs::set_permissions(&secrets, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("chmod 0700 {}", secrets.display()))?;
    ui::ok(&format!("Data dir ready ({})", cfg.data_dir.display()));

    // Passwordless sudo for the owner's admin shell. `virtues` is a system
    // account with no password, so interactive sudo has nothing to authenticate
    // against — NOPASSWD is the only thing that works, not just a convenience.
    // Safe here because the only way to reach a `virtues` shell is the
    // auth-gated web terminal (WG/SPKI + session); there's no local login
    // (shell is /usr/sbin/nologin). Drop-file at 0440, the mode sudo requires.
    let sudoers = "/etc/sudoers.d/virtues";
    fs::write(sudoers, "virtues ALL=(ALL) NOPASSWD: ALL\n")
        .with_context(|| format!("writing {sudoers}"))?;
    fs::set_permissions(sudoers, fs::Permissions::from_mode(0o440))
        .with_context(|| format!("chmod 0440 {sudoers}"))?;
    ui::ok("Granted 'virtues' passwordless sudo (/etc/sudoers.d/virtues)");

    Ok(())
}

// ────────────────────────────────────────────────────────────────────────
// Postgres role + database + pgvector extension
// ────────────────────────────────────────────────────────────────────────

pub async fn provision_db() -> Result<()> {
    if !psql_exists("SELECT 1 FROM pg_roles WHERE rolname='virtues'").await? {
        let mut cmd = Command::new("sudo");
        cmd.args([
            "-u", "postgres",
            "createuser",
            "--no-superuser",
            "--no-createrole",
            "--createdb",
            "virtues",
        ]);
        run_step("Create Postgres role 'virtues'", cmd).await?;
    } else {
        ui::skip("Postgres role 'virtues' already exists");
    }

    if !psql_exists("SELECT 1 FROM pg_database WHERE datname='virtues'").await? {
        let mut cmd = Command::new("sudo");
        cmd.args(["-u", "postgres", "createdb", "-O", "virtues", "virtues"]);
        run_step("Create Postgres database 'virtues'", cmd).await?;
    } else {
        ui::skip("Postgres database 'virtues' already exists");
    }

    // pgvector's CREATE EXTENSION requires superuser. Run it as postgres
    // so the virtues role doesn't need elevation later.
    let mut cmd = Command::new("sudo");
    cmd.args([
        "-u", "postgres",
        "psql", "-d", "virtues",
        "-c", "CREATE EXTENSION IF NOT EXISTS vector",
    ]);
    run_step("Install pgvector extension", cmd).await
}

async fn psql_exists(sql: &str) -> Result<bool> {
    let out = Command::new("sudo")
        .args(["-u", "postgres", "psql", "-tAc", sql])
        .output()
        .await?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    Ok(stdout.trim() == "1")
}

// ────────────────────────────────────────────────────────────────────────
// Env file — DATABASE_URL, encryption key, prod URLs
// ────────────────────────────────────────────────────────────────────────

/// The inference-related env keys, per mode.
///
/// Dragon: mode marker + the loopback sidecar defaults. Manual: mode marker,
/// the user's endpoint URLs, plus the fingerprint + dims recorded by
/// `mode::validate_manual` — the runtime re-embeds the probe strings at boot
/// and refuses to serve search against a silently-swapped model.
fn inference_env_keys(
    mode: &InferenceMode,
    validation: Option<&ValidationReport>,
) -> Vec<(&'static str, String)> {
    match mode {
        // Dragon + Bundled both talk to locally-provisioned sidecars on
        // loopback; the runtime uses the EmbeddingGemma defaults (256-dim
        // truncation, gemma prompts) since it's our own model — no fingerprint
        // pin. `VIRTUES_INFERENCE` records which path chose it.
        InferenceMode::Dragon | InferenceMode::Bundled => vec![
            (
                "VIRTUES_INFERENCE",
                match mode {
                    InferenceMode::Bundled => "bundled".to_string(),
                    _ => "dragon".to_string(),
                },
            ),
            ("VIRTUES_EMBED_URL", "http://127.0.0.1:18181".to_string()),
            ("VIRTUES_RERANK_URL", "http://127.0.0.1:18182".to_string()),
        ],
        InferenceMode::Manual { embed_url, embed_model, rerank_url, .. } => {
            let mut keys = vec![
                ("VIRTUES_INFERENCE", "manual".to_string()),
                ("VIRTUES_EMBED_URL", embed_url.clone()),
                ("VIRTUES_EMBED_MODEL", embed_model.clone()),
            ];
            if let Some(url) = rerank_url {
                keys.push(("VIRTUES_RERANK_URL", url.clone()));
            }
            if let Some(v) = validation {
                keys.push(("VIRTUES_EMBED_FINGERPRINT", v.fingerprint.clone()));
                keys.push(("VIRTUES_EMBED_DIMS", v.dims.to_string()));
                // Only pin non-empty prompts; an empty prefix is the runtime
                // embedder's default for a manual endpoint, so writing it adds
                // noise. A wrong/absent prefix never corrupts the index (it's
                // not part of the fingerprint), only recall quality.
                // Quote prompt values: they carry significant trailing spaces
                // (e.g. "query: ") that both systemd EnvironmentFile and dotenv
                // strip from unquoted values. Quoting preserves them verbatim.
                if !v.query_prompt.is_empty() {
                    keys.push(("VIRTUES_EMBED_QUERY_PROMPT", quote_env_value(&v.query_prompt)));
                }
                if !v.doc_prompt.is_empty() {
                    keys.push(("VIRTUES_EMBED_DOC_PROMPT", quote_env_value(&v.doc_prompt)));
                }
            }
            keys
        }
    }
}

/// Wrap a value in double quotes so systemd's EnvironmentFile and dotenv both
/// preserve significant leading/trailing whitespace (which they strip from
/// unquoted values). Both parsers unescape `\\` and `\"` inside double quotes,
/// so escape those two.
fn quote_env_value(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

pub async fn write_env_file(
    cfg: &InstallConfig,
    mode: &InferenceMode,
    validation: Option<&ValidationReport>,
) -> Result<()> {
    let path = cfg.env_file_path();
    if path.exists() {
        // Existing file — append any missing required keys without touching
        // anything already in there. Critical: never rotate the encryption
        // key (would invalidate every stored credential).
        return merge_env_file(&path, cfg, mode, validation).await;
    }
    let key = openssl_rand_base64_32().await?;
    let now = chrono_utc_iso();
    let mut body = format!(
        "# Generated by virtues-installer on {now}.\n\
         # DATABASE_URL omits host -> Unix socket -> peer auth, no password.\n\
         DATABASE_URL=postgres:///virtues\n\
         VIRTUES_ENCRYPTION_KEY={key}\n\
         ENVIRONMENT=production\n\
         STATIC_DIR={static_dir}\n\
         STORAGE_PATH={storage_path}\n\
         VIRTUES_ATLAS_URL={atlas}\n\
         VIRTUES_API_URL={api}\n\
         VIRTUES_MODELS_DIR={models_dir}\n\
         VIRTUES_ACTIONS_DIR={actions_dir}\n\
         VIRTUES_ACTIONS_BIN_DIR={actions_bin_dir}\n",
        static_dir = cfg.web_dir().display(),
        storage_path = cfg.data_dir.join("lake").display(),
        atlas = cfg.atlas_url,
        api = cfg.virtues_api_url,
        models_dir = cfg.models_dir().display(),
        actions_dir = cfg.actions_dir().display(),
        actions_bin_dir = cfg.actions_bin_dir().display(),
    );
    for (k, v) in inference_env_keys(mode, validation) {
        body.push_str(&format!("{k}={v}\n"));
    }
    fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    let mut cmd = Command::new("chown");
    cmd.args(["virtues:virtues", path.to_str().unwrap()]);
    let _ = cmd.output().await;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .with_context(|| format!("chmod 0600 {}", path.display()))?;
    ui::ok(&format!("Wrote {}", path.display()));
    Ok(())
}

/// Append any missing required keys to an existing env file.
///
/// Never touches existing values (especially not VIRTUES_ENCRYPTION_KEY).
/// This is how the installer keeps a re-run idempotent without leaving
/// the user stuck on an older env file that's missing new keys we added
/// in a later version. Today the typical case is VIRTUES_ATLAS_URL and
/// VIRTUES_API_URL, added in v0.1.1.
async fn merge_env_file(
    path: &std::path::Path,
    cfg: &InstallConfig,
    mode: &InferenceMode,
    validation: Option<&ValidationReport>,
) -> Result<()> {
    let existing = fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;

    // Parse keys present (lines like KEY=...). Comment lines + blanks
    // are ignored.
    let present: std::collections::HashSet<String> = existing
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            l.split_once('=').map(|(k, _)| k.trim().to_string())
        })
        .collect();

    // The full list of keys this installer would write on a fresh
    // install. Anything missing gets appended.
    let mut want: Vec<(&str, String)> = vec![
        ("DATABASE_URL", "postgres:///virtues".to_string()),
        ("ENVIRONMENT", "production".to_string()),
        ("STATIC_DIR", cfg.web_dir().display().to_string()),
        ("STORAGE_PATH", cfg.data_dir.join("lake").display().to_string()),
        ("VIRTUES_ATLAS_URL", cfg.atlas_url.clone()),
        ("VIRTUES_API_URL", cfg.virtues_api_url.clone()),
        ("VIRTUES_MODELS_DIR", cfg.models_dir().display().to_string()),
        ("VIRTUES_ACTIONS_DIR", cfg.actions_dir().display().to_string()),
        ("VIRTUES_ACTIONS_BIN_DIR", cfg.actions_bin_dir().display().to_string()),
    ];
    want.extend(inference_env_keys(mode, validation));

    let missing: Vec<&(&str, String)> = want.iter().filter(|(k, _)| !present.contains(*k)).collect();
    if missing.is_empty() {
        ui::skip(&format!("Env file at {} already complete", path.display()));
        return Ok(());
    }

    let mut body = existing;
    if !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str(&format!("\n# Added by virtues-installer on {}.\n", chrono_utc_iso()));
    for (k, v) in &missing {
        body.push_str(&format!("{k}={v}\n"));
    }
    fs::write(path, body).with_context(|| format!("writing {}", path.display()))?;
    ui::ok(&format!(
        "Added {} missing keys to {}",
        missing.len(),
        path.display()
    ));
    Ok(())
}

async fn openssl_rand_base64_32() -> Result<String> {
    let out = Command::new("openssl")
        .args(["rand", "-base64", "32"])
        .output()
        .await
        .context("openssl rand")?;
    if !out.status.success() {
        return Err(anyhow!("openssl rand failed"));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn chrono_utc_iso() -> String {
    // Skipping a chrono dep just for this — gettimeofday + strftime via
    // libc would be more code than this string is worth. Just use SystemTime
    // and print seconds since epoch in ISO-ish format.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("epoch-{secs}")
}

// ────────────────────────────────────────────────────────────────────────
// `virtues bringup` — migrations + box identity
// ────────────────────────────────────────────────────────────────────────

pub async fn run_bringup(cfg: &InstallConfig) -> Result<()> {
    let bin = cfg.binary_path();
    if !bin.exists() {
        return Err(anyhow!("virtues binary not installed at {}", bin.display()));
    }
    let env_file = cfg.env_file_path();
    let cmd_str = format!(
        "set -a; . '{env_file}'; set +a; '{bin}' bringup",
        env_file = env_file.display(),
        bin = bin.display(),
    );
    let mut cmd = Command::new("sudo");
    cmd.args(["-u", "virtues", "bash", "-c", &cmd_str]);
    run_step("Load env + run virtues bringup", cmd).await
}

// ────────────────────────────────────────────────────────────────────────
// systemd unit
// ────────────────────────────────────────────────────────────────────────

pub async fn install_systemd_unit(cfg: &InstallConfig) -> Result<()> {
    let body = SYSTEMD_UNIT_TEMPLATE
        .replace("__BIN__", &cfg.binary_path().display().to_string())
        .replace("__DATA_DIR__", &cfg.data_dir.display().to_string());
    fs::write("/etc/systemd/system/virtues.service", body)
        .context("writing /etc/systemd/system/virtues.service")?;

    let mut cmd = Command::new("systemctl");
    cmd.arg("daemon-reload");
    run_step("Install systemd unit", cmd).await
}

const SYSTEMD_UNIT_TEMPLATE: &str = r#"[Unit]
Description=Virtues — your data, on your hardware
Documentation=https://virtues.com/docs
After=postgresql.service network-online.target
Wants=postgresql.service network-online.target

[Service]
Type=simple
User=virtues
Group=virtues
WorkingDirectory=__DATA_DIR__
EnvironmentFile=-__DATA_DIR__/virtues.env
ExecStartPre=/bin/sh -c 'until pg_isready -h /var/run/postgresql -d virtues -U virtues -t 1 >/dev/null 2>&1; do sleep 1; done'
ExecStart=__BIN__ server --host [::] --port 8000
ExecStopPost=__BIN__ report-crash
TimeoutStartSec=120
Restart=on-failure
RestartSec=5

# Sandbox intentionally OFF on THIS unit only. The web terminal
# (/ws/terminal) spawns the owner's admin shell as a child of this process, so
# the shell inherits whatever sandbox this unit has — and a real admin shell
# must do real sysadmin work: sudo to root, write /etc and /usr, install
# packages, run containers, load modules, edit sysctls. Each flag below blocked
# part of that:
#   NoNewPrivileges/RestrictSUIDSGID — blocked sudo's setuid escalation.
#   CapabilityBoundingSet=            — root regained uid 0 but ZERO caps, so
#                                       mount/modprobe/DAC-override still failed.
#   ProtectSystem/ProtectHome         — /usr,/etc,/home mounted read-only.
#   ProtectKernel*/ControlGroups      — blocked sysctl + cgroup writes.
#   RestrictNamespaces                — blocked containers (podman/unshare).
# The box is a single-tenant, owner-operated appliance; that shell IS the admin
# surface and its security boundary is the WG/SPKI + session auth in front of
# /ws/terminal, not this sandbox. TRADE-OFF: the network-facing app on :8000
# runs in this same unit, so an RCE in the app now escalates to root — accepted
# deliberately for an owner-operated box. The inference sidecars keep their full
# sandbox; only this human-facing unit is opened up.
NoNewPrivileges=false
RestrictSUIDSGID=false
SystemCallArchitectures=native

[Install]
WantedBy=multi-user.target
"#;

// ────────────────────────────────────────────────────────────────────────
// Post-install health check
// ────────────────────────────────────────────────────────────────────────

/// `check_sidecars` is true only in Dragon mode — a manual-inference box has
/// no local sidecar units and no GGUFs on disk, and probing for them would
/// report phantom issues on every healthy install.
pub async fn health_check(cfg: &InstallConfig, check_sidecars: bool) -> Result<u32> {
    let mut issues = 0u32;

    // Postgres reachable via peer auth (no password prompt, no TCP).
    let pg = Command::new("sudo")
        .args(["-u", "virtues", "psql", "-d", "virtues", "-c", "SELECT 1"])
        .stdin(std::process::Stdio::null())
        .output()
        .await?;
    if pg.status.success() {
        ui::ok("Postgres reachable as 'virtues' (peer auth)");
    } else {
        ui::warn("Postgres connection as 'virtues' failed");
        issues += 1;
    }

    if check_sidecars {
        // Inference sidecars responding. /health returns 200 only once the
        // model is loaded, so this also catches a bad/missing GGUF. Model load
        // can take a few seconds after `systemctl start` — retry briefly.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()?;
        for (port, unit) in [(18181u16, "virtues-embed"), (18182, "virtues-rerank")] {
            let url = format!("http://127.0.0.1:{port}/health");
            let mut up = false;
            for _ in 0..10 {
                if matches!(client.get(&url).send().await, Ok(r) if r.status().is_success()) {
                    up = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            if up {
                ui::ok(&format!("{unit} responding on :{port}"));
            } else {
                ui::warn(&format!(
                    "{unit} not responding on :{port} — journalctl -u {unit}"
                ));
                issues += 1;
            }
        }

        // GGUFs on disk.
        for gguf in [&cfg.embed_gguf, &cfg.rerank_gguf] {
            let p = cfg.models_dir().join(gguf);
            if p.is_file() {
                ui::ok(&format!("Model present: {gguf}"));
            } else {
                ui::warn(&format!("Model missing: {} — re-run the installer", p.display()));
                issues += 1;
            }
        }
    } else {
        ui::skip("Manual inference — endpoints validated earlier, no local sidecars to probe");
    }

    // Binary --version (now clean, observability init is skipped for
    // trivial subcommands in v0.1.1).
    let ver = Command::new(cfg.binary_path()).arg("--version").output().await;
    match ver {
        Ok(o) if o.status.success() => {
            let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
            ui::ok(&format!("virtues binary OK: {v}"));
        }
        _ => {
            ui::warn("virtues --version probe failed");
            issues += 1;
        }
    }

    Ok(issues)
}
