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
use crate::steps::{run_step, PkgMgr, Target};
use crate::ui;

// ────────────────────────────────────────────────────────────────────────
// System dependencies (apt/dnf): Postgres, WireGuard, Avahi, ca-certs
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
    apt_install("WireGuard", &["wireguard", "wireguard-tools"]).await?;
    apt_install("Avahi (mDNS)", &["avahi-daemon", "avahi-utils", "libnss-mdns"]).await?;
    apt_install("ca-certificates + curl", &["ca-certificates", "curl"]).await?;

    systemctl(&["enable", "--now", "postgresql"], "Enable postgresql").await?;
    systemctl(&["enable", "--now", "avahi-daemon"], "Enable avahi-daemon").await?;
    Ok(())
}

async fn install_deps_dnf() -> Result<()> {
    dnf_install("Postgres + pgvector", &["postgresql-server", "postgresql-contrib", "pgvector"]).await?;
    dnf_install("WireGuard tooling", &["wireguard-tools"]).await?;
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

    // Write + (re)start one unit per model. restart rather than just
    // enable --now so an installer re-run picks up unit/binary changes.
    for (unit, template, gguf) in [
        ("virtues-embed", EMBED_UNIT_TEMPLATE, &cfg.embed_gguf),
        ("virtues-rerank", RERANK_UNIT_TEMPLATE, &cfg.rerank_gguf),
    ] {
        let body = template
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

/// Shared shape: loopback-only, runs as `virtues`, read-only filesystem
/// (the GGUF is mmap'd read-only; PrivateTmp covers scratch). `-c/-b/-ub
/// 8192` match bge-m3's 8K context — non-causal encoders need the whole
/// sequence to fit one ubatch or llama-server rejects the request.
const EMBED_UNIT_TEMPLATE: &str = r#"[Unit]
Description=Virtues embedding sidecar (llama-server, bge-m3)
Documentation=https://virtues.com/docs
After=network.target

[Service]
Type=simple
User=virtues
Group=virtues
ExecStart=__BIN__ --embedding --pooling cls -m __MODEL__ --host 127.0.0.1 --port 18181 -c 8192 -b 8192 -ub 8192
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
Description=Virtues rerank sidecar (llama-server, bge-reranker-v2-m3)
Documentation=https://virtues.com/docs
After=network.target

[Service]
Type=simple
User=virtues
Group=virtues
ExecStart=__BIN__ --rerank -m __MODEL__ --host 127.0.0.1 --port 18182 -c 8192 -b 8192 -ub 8192
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

    write_setup_sudoers().await?;
    Ok(())
}

/// The setup wizard's one privileged seam: the "name your box" step needs
/// `hostnamectl` (root), but the server runs as `virtues`. Grant EXACTLY the
/// two commands the rename uses — nothing else — via a dedicated sudoers
/// fragment. The server calls them with `sudo -n`, so on an install that
/// predates this rule the wizard step fails fast with a clear message
/// instead of hanging on a password prompt.
async fn write_setup_sudoers() -> Result<()> {
    const PATH: &str = "/etc/sudoers.d/virtues-setup";
    const RULE: &str = "\
# Written by virtues-installer. Lets the setup wizard's \"name your box\"
# step rename the host (the mDNS name follows the hostname). Scoped to
# exactly these commands; the virtues user has no other sudo rights.
virtues ALL=(root) NOPASSWD: /usr/bin/hostnamectl set-hostname *
virtues ALL=(root) NOPASSWD: /usr/bin/systemctl reload-or-restart avahi-daemon
";
    fs::write(PATH, RULE).with_context(|| format!("writing {PATH}"))?;
    // Sudoers fragments must be 0440 or sudo ignores the whole file.
    fs::set_permissions(PATH, fs::Permissions::from_mode(0o440))
        .with_context(|| format!("chmod 0440 {PATH}"))?;
    // Syntax-check; a bad fragment can lock sudo out for everyone. visudo
    // exits non-zero on parse failure → remove the fragment and fail loudly.
    let check = Command::new("visudo").args(["-c", "-f", PATH]).output().await;
    match check {
        Ok(out) if out.status.success() => {
            ui::ok("Sudoers rule for box rename (setup wizard)");
            Ok(())
        }
        Ok(out) => {
            let _ = fs::remove_file(PATH);
            Err(anyhow!(
                "sudoers fragment failed visudo check (removed): {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
        Err(e) => {
            // visudo missing entirely — keep the fragment (it's static and
            // known-good) but say so.
            ui::warn(&format!("visudo not found ({e}) — installed rename rule unchecked"));
            Ok(())
        }
    }
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

pub async fn write_env_file(cfg: &InstallConfig) -> Result<()> {
    let path = cfg.env_file_path();
    if path.exists() {
        // Existing file — append any missing required keys without touching
        // anything already in there. Critical: never rotate the encryption
        // key (would invalidate every stored credential).
        return merge_env_file(&path, cfg).await;
    }
    let key = openssl_rand_base64_32().await?;
    let now = chrono_utc_iso();
    let body = format!(
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
         VIRTUES_EMBED_URL=http://127.0.0.1:18181\n\
         VIRTUES_RERANK_URL=http://127.0.0.1:18182\n",
        static_dir = cfg.web_dir().display(),
        storage_path = cfg.data_dir.join("lake").display(),
        atlas = cfg.atlas_url,
        api = cfg.virtues_api_url,
        models_dir = cfg.models_dir().display(),
    );
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
async fn merge_env_file(path: &std::path::Path, cfg: &InstallConfig) -> Result<()> {
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
    let want: &[(&str, String)] = &[
        ("DATABASE_URL", "postgres:///virtues".to_string()),
        ("ENVIRONMENT", "production".to_string()),
        ("STATIC_DIR", cfg.web_dir().display().to_string()),
        ("STORAGE_PATH", cfg.data_dir.join("lake").display().to_string()),
        ("VIRTUES_ATLAS_URL", cfg.atlas_url.clone()),
        ("VIRTUES_API_URL", cfg.virtues_api_url.clone()),
        ("VIRTUES_MODELS_DIR", cfg.models_dir().display().to_string()),
        ("VIRTUES_EMBED_URL", "http://127.0.0.1:18181".to_string()),
        ("VIRTUES_RERANK_URL", "http://127.0.0.1:18182".to_string()),
    ];

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

    // Sibling unit: virtues-wireguard reconciles wg0 from the peer rows
    // virtues writes to Postgres. Without it, pair succeeds but the WG
    // handshake from a paired desktop never completes — there's no peer
    // installed on the kernel side.
    //
    // Skipped silently when the binary isn't present (older tarballs from
    // before v0.2.1 didn't ship it). download.rs already warned the user.
    if cfg.wg_binary_path().exists() {
        let wg_body = WIREGUARD_UNIT_TEMPLATE
            .replace("__BIN__", &cfg.wg_binary_path().display().to_string())
            .replace("__DATA_DIR__", &cfg.data_dir.display().to_string());
        fs::write("/etc/systemd/system/virtues-wireguard.service", wg_body)
            .context("writing /etc/systemd/system/virtues-wireguard.service")?;
    }

    let mut cmd = Command::new("systemctl");
    cmd.arg("daemon-reload");
    run_step("Install systemd unit", cmd).await
}

/// Enable + start `virtues-wireguard.service` if its binary was installed.
/// Called after the main `virtues.service` is up so the WG reconciler reads
/// a populated DB (server keypair, any pre-existing peer rows).
pub async fn enable_wireguard_unit(cfg: &InstallConfig) -> Result<()> {
    if !cfg.wg_binary_path().exists() {
        return Ok(());
    }
    let mut cmd = Command::new("systemctl");
    cmd.args(["enable", "--now", "virtues-wireguard"]);
    run_step("Enable + start virtues-wireguard service", cmd).await
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

NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=__DATA_DIR__
ProtectHome=true
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=false
ProtectControlGroups=true
RestrictSUIDSGID=true
LockPersonality=true
RestrictNamespaces=true
SystemCallArchitectures=native

# No capabilities: the main app is rootless and does NO kernel networking —
# it persists WG peers to the DB and the virtues-wireguard daemon (the only
# holder of CAP_NET_ADMIN) reconciles wg0 from there. Port 8000 is unprivileged
# so no CAP_NET_BIND_SERVICE either. Empty bounding set drops everything.
CapabilityBoundingSet=

[Install]
WantedBy=multi-user.target
"#;

/// Privileged WG reconciler — does netlink to `wg0` and records the box's
/// current public endpoint into `box_secrets` for the rendezvous publisher.
///
/// `After=virtues.service` so the main app has a chance to migrate the DB +
/// mint the server keypair before reconcile runs. The reconciler is
/// idempotent and retries internally, so the strict ordering is more about
/// log clarity than correctness.
///
/// `User=virtues` + `AmbientCapabilities=CAP_NET_ADMIN`: systemd starts as
/// root, sets the cap, then drops to `virtues` while keeping the capability
/// in the ambient set. Same pattern as `virtues.service`.
const WIREGUARD_UNIT_TEMPLATE: &str = r#"[Unit]
Description=Virtues WireGuard reconciler — kernel wg0 from the peer store
Documentation=https://virtues.com/docs
After=virtues.service postgresql.service
Wants=postgresql.service
Requires=virtues.service

[Service]
Type=simple
User=virtues
Group=virtues
WorkingDirectory=__DATA_DIR__
EnvironmentFile=-__DATA_DIR__/virtues.env
ExecStart=__BIN__
Restart=on-failure
RestartSec=5s

NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=__DATA_DIR__
ProtectHome=true
PrivateTmp=true
ProtectKernelTunables=true
ProtectKernelModules=false
ProtectControlGroups=true
RestrictSUIDSGID=true
LockPersonality=true
RestrictNamespaces=true
SystemCallArchitectures=native

# Only NET_ADMIN — no port binding, no other privileged ops.
AmbientCapabilities=CAP_NET_ADMIN
CapabilityBoundingSet=CAP_NET_ADMIN

[Install]
WantedBy=multi-user.target
"#;

// ────────────────────────────────────────────────────────────────────────
// Post-install health check
// ────────────────────────────────────────────────────────────────────────

pub async fn health_check(cfg: &InstallConfig) -> Result<u32> {
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
