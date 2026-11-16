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
    // Pre-flight already refreshed the package lists and decided where
    // Postgres 18 comes from. add_pgdg_repo() is idempotent.
    add_pgdg_repo(target).await?;

    apt_install("Postgres 18 + pgvector", &["postgresql-18", "postgresql-18-pgvector"]).await?;
    apt_install("Avahi (mDNS)", &["avahi-daemon", "avahi-utils", "libnss-mdns"]).await?;
    apt_install("ca-certificates + curl", &["ca-certificates", "curl"]).await?;
    // The web terminal runs its shell inside tmux so a closed tab or a dropped
    // connection detaches instead of killing whatever is running. Without tmux
    // it degrades to a bare shell that dies with the websocket.
    apt_install("tmux (terminal sessions)", &["tmux"]).await?;
    // `llama-server` (from our own tarball) links libgomp, and a minimal Ubuntu
    // has no reason to carry it — a cloud image, a container base, a netboot
    // install. Missing, both inference sidecars die at exec with
    //   error while loading shared libraries: libgomp.so.1
    // which systemd reports as status=127 and then buries under its own restart
    // spam. The install itself still reports success: `install_inference` only
    // checks that the binary EXISTS, and the health probe's miss is a warning.
    // Found on a fresh 24.04 arm64 box on 2026-09-03, restart counter at 54.
    apt_install("libgomp (llama-server runtime)", &["libgomp1"]).await?;

    systemctl(&["enable", "--now", "postgresql"], "Enable postgresql").await?;
    systemctl(&["enable", "--now", "avahi-daemon"], "Enable avahi-daemon").await?;
    Ok(())
}

/// Automatic OS security patching, on every install.
///
/// The system layer (kernel, OpenSSL, Postgres, everything from the package
/// manager) is not carried in a Virtues release, so `virtues upgrade` cannot
/// patch it. The distro's own updater closes that: the server pulls security
/// fixes on a timer and applies them itself.
///
/// Security updates only, never feature updates, so an unwatched server never
/// moves a package to a new feature version under itself. No automatic
/// reboot: userspace fixes apply at once, and a kernel fix waits for the next
/// reboot the owner chooses.
///
/// Fedora is skipped with a pointer to `dnf-automatic`: its configuration has
/// not been tested here.
pub async fn enable_security_updates(target: &Target) -> Result<()> {
    match target.pkg_mgr {
        PkgMgr::Apt => enable_unattended_upgrades().await,
        PkgMgr::Dnf => {
            ui::skip("Automatic security updates: turn on dnf-automatic to have Fedora apply them");
            Ok(())
        }
    }
}

async fn enable_unattended_upgrades() -> Result<()> {
    apt_install("Automatic security updates", &["unattended-upgrades"]).await?;

    fs::create_dir_all("/etc/apt/apt.conf.d").context("mkdir apt.conf.d")?;
    fs::write(
        "/etc/apt/apt.conf.d/20auto-upgrades",
        "APT::Periodic::Update-Package-Lists \"1\";\n\
         APT::Periodic::Unattended-Upgrade \"1\";\n",
    )
    .context("writing 20auto-upgrades")?;
    fs::write("/etc/apt/apt.conf.d/52virtues-unattended", UNATTENDED_CONF)
        .context("writing 52virtues-unattended")?;

    // The timers ship disabled on some server images; enable them explicitly.
    for timer in ["apt-daily.timer", "apt-daily-upgrade.timer"] {
        systemctl_try(&["enable", "--now", timer]).await;
    }
    ui::ok("Automatic OS security updates on (security fixes only, no automatic reboot)");
    Ok(())
}

const UNATTENDED_CONF: &str = r#"// Installed by virtues-installer.
// Security fixes only, never feature updates on an unwatched server.
Unattended-Upgrade::Allowed-Origins {
    "${distro_id}:${distro_codename}-security";
    "${distro_id}ESMApps:${distro_codename}-apps-security";
    "${distro_id}ESM:${distro_codename}-infra-security";
};
// Never reboot the server out from under its owner. Userspace fixes apply
// immediately; a kernel fix waits for the next reboot.
Unattended-Upgrade::Automatic-Reboot "false";
Unattended-Upgrade::Remove-Unused-Kernel-Packages "true";
Unattended-Upgrade::Remove-New-Unused-Dependencies "true";
"#;

async fn install_deps_dnf() -> Result<()> {
    // UNVERSIONED, unlike the apt path's pinned `postgresql-18`, so a Fedora
    // box gets whatever its release ships — 16 or 17 today. That asymmetry is
    // known and tolerated rather than accidental: Fedora is a DIY-only target,
    // its Postgres is recent enough for everything we use, and there is no PGDG
    // equivalent worth carrying for it.
    //
    // The cost lands in ONE place, so it is worth naming: `pg_dump` output from
    // a newer server cannot be read by an older `pg_restore`. A backup taken on
    // a Debian or Ubuntu box (18) will not restore onto a Fedora box on 16. If that
    // ever needs to work, this is the line to change.
    dnf_install("Postgres + pgvector", &["postgresql-server", "postgresql-contrib", "pgvector"]).await?;
    dnf_install("Avahi (mDNS)", &["avahi", "nss-mdns"]).await?;
    dnf_install("ca-certificates + curl", &["ca-certificates", "curl"]).await?;
    // See the apt path: tmux is what makes web-terminal sessions survive a
    // dropped connection.
    dnf_install("tmux (terminal sessions)", &["tmux"]).await?;
    // See the apt path: llama-server links libgomp, and its absence kills both
    // inference sidecars at exec while the install still reports success.
    // Fedora spells the package without the soname suffix.
    dnf_install("libgomp (llama-server runtime)", &["libgomp"]).await?;

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
    // Only when the distro's own archive has no Postgres 18 (pre-flight).
    let Some(codename) = target.pgdg.as_deref() else {
        return Ok(());
    };

    apt_install("apt key tooling", &["curl", "ca-certificates", "gnupg"]).await?;

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

/// Best-effort `systemctl`: output captured and discarded, failure ignored.
async fn systemctl_try(args: &[&str]) {
    let _ = Command::new("systemctl").args(args).output().await;
}

/// `mkdir -p` the parent, then write the file.
fn install_file(path: &str, body: &str) -> Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
    }
    fs::write(path, body).with_context(|| format!("writing {path}"))
}

// ────────────────────────────────────────────────────────────────────────
// Inference sidecar — llama-server hosting the embedding GGUF
// ────────────────────────────────────────────────────────────────────────

/// The embedding sidecar (:18181): llama-server serving one model. The binary
/// comes out of the release tarball (download.rs put it at
/// `cfg.llama_binary_path()`); the GGUF comes from the pinned models release,
/// SHA-verified.
///
/// No reranker: search doesn't rerank (see `search::query` in core), and
/// `virtues upgrade` retires the one older installs ran.
pub async fn install_inference(cfg: &InstallConfig) -> Result<()> {
    let bin = cfg.llama_binary_path();
    if !bin.exists() {
        return Err(anyhow!(
            "llama-server not found at {} — tarball predates the v0.1.1 inference sidecars?",
            bin.display()
        ));
    }

    // Fetch the GGUF (skipped when already on disk — it's SHA-verified at
    // download time and immutable afterwards).
    let models_dir = cfg.models_dir();
    fs::create_dir_all(&models_dir)
        .with_context(|| format!("creating {}", models_dir.display()))?;
    crate::download::fetch_model(cfg, &cfg.embed_gguf).await?;
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

    // Write + (re)start the unit. restart rather than just enable --now so an
    // installer re-run picks up unit/binary changes.
    let body = EMBED_UNIT_TEMPLATE
        .replace("__SUPP_GROUPS__", &supp_groups)
        .replace("__BIN__", &bin.display().to_string())
        .replace("__MODEL__", &models_dir.join(&cfg.embed_gguf).display().to_string());
    fs::write("/etc/systemd/system/virtues-embed.service", body)
        .context("writing virtues-embed.service")?;
    systemctl(&["daemon-reload"], "Install the inference sidecar unit").await?;
    systemctl(&["enable", "virtues-embed"], "Enable the inference sidecar").await?;
    systemctl(&["restart", "virtues-embed"], "Start the inference sidecar").await
}

/// libpdfium — native PDF text extraction for the `document_extraction`
/// action (agents/record/researcher.md D1). Mode-independent (CPU parse; runs the same on
/// the Dragon and a DIY box), SHA-verified from the models bucket like every
/// other model asset, skipped when already present.
pub async fn install_pdfium(cfg: &InstallConfig) -> Result<()> {
    crate::download::fetch_asset(cfg, &cfg.pdfium_asset(), cfg.pdfium_lib_path()).await?;
    let mut cmd = Command::new("chown");
    cmd.args(["-R", "virtues:virtues", cfg.pdfium_dir().to_str().unwrap()]);
    let _ = cmd.output().await;
    Ok(())
}

/// Provision the Dragon NPU daemon: fetch the QAIRT context binaries +
/// tokenizers, then install + start `virtues-qnnd.service`. Replaces the
/// llama-server sidecar on our board — but serves the SAME llama-compatible
/// HTTP inference contract on :18181, so the runtime talks to it through
/// `VIRTUES_EMBED_URL` like any other endpoint. (It also loads a ColBERT
/// reranker and serves it on :18182; search doesn't call it.)
///
/// Depends on two things this installer does NOT produce: the `virtues-qnnd`
/// binary in the tarball (CI must build it for aarch64 with `QNN_SDK_ROOT`) and
/// the Qualcomm QAIRT runtime libs on the Dragon's OS image (proprietary — see
/// `QNN_UNIT_TEMPLATE`).
pub async fn install_qnn(cfg: &InstallConfig) -> Result<()> {
    let bin = cfg.qnnd_binary_path();
    if !bin.exists() {
        return Err(anyhow!(
            "virtues-qnnd not found at {} — this tarball has no NPU daemon \
             (the aarch64 build leg needs QNN_SDK_ROOT to produce a real one)",
            bin.display()
        ));
    }

    let qnn_dir = cfg.qnn_models_dir();
    fs::create_dir_all(&qnn_dir).with_context(|| format!("creating {}", qnn_dir.display()))?;

    // Context binaries + tokenizers (SHA-verified, skipped if already present).
    for name in [&cfg.qnn_embed_bin, &cfg.qnn_rerank_bin] {
        crate::download::fetch_asset(cfg, name, qnn_dir.join(name)).await?;
    }
    for (dest_rel, asset) in &cfg.qnn_tokenizers {
        crate::download::fetch_asset(cfg, asset, qnn_dir.join(dest_rel)).await?;
    }

    let mut cmd = Command::new("chown");
    cmd.args(["-R", "virtues:virtues", qnn_dir.to_str().unwrap()]);
    let _ = cmd.output().await;

    // The daemon opens the Hexagon DSP node (`/dev/fastrpc-cdsp`, `render`
    // group) — the same detection the GPU path uses covers it.
    let supp_groups = match gpu_access_groups().await.as_slice() {
        [] => String::new(),
        groups => format!("SupplementaryGroups={}\n", groups.join(" ")),
    };
    // QAIRT runtime libs (Qualcomm-proprietary — not shipped by us). Prefer an
    // explicit VIRTUES_QNN_LIB_DIR, else auto-detect libQnnHtp.so under the usual
    // roots (a QAIRT SDK unpack, a Radxa QAIRT install, …). Both
    // LD_LIBRARY_PATH (host) and ADSP_LIBRARY_PATH (DSP skel) must point there.
    //
    // Missing libs is a REFUSAL to install the unit, not a warning we print on
    // the way past. Enabling a daemon we have just proven cannot load its own
    // runtime buys nothing and costs a permanent crash loop: `Restart=on-failure`
    // at `RestartSec=5` is two starts per ten seconds, which never trips
    // systemd's default rate limit, so it restarts every five seconds forever.
    // One box ran that loop for ten days — 169k restarts, a 1GB journal, and a
    // core of CPU burnt continuously — while the installer's warning sat far
    // above in a scrollback nobody re-read.
    let qnn_env = match detect_qnn_libs(cfg).await {
        Some((host, adsp)) => {
            ui::ok(&format!("QNN runtime libs: {host}"));
            format!("Environment=LD_LIBRARY_PATH={host}\nEnvironment=ADSP_LIBRARY_PATH={adsp}\n")
        }
        // `detect_qnn_libs` returning None spans two very different worlds: the
        // Dragon's OS image, where the libs sit on the default loader path
        // and the unit correctly needs no env at all, and a box that simply has
        // no QAIRT on it. A bounded `find` cannot tell those apart, so ask the
        // loader instead — that is the question that actually matters.
        None if loader_has_lib("libQnnHtp.so").await => {
            ui::ok("QNN runtime libs: on the default loader path");
            String::new()
        }
        // Nothing on the box: fetch the libs from Qualcomm's own public
        // distribution. This is the ordinary path for a DIY Dragon — the lab
        // box only works because someone hand-unpacked a 1.44 GB SDK into
        // /qairt-extract, which nobody else is going to do.
        None => match crate::qairt::ensure_libs(cfg).await {
            Ok((host, dsp)) => format!(
                "Environment=LD_LIBRARY_PATH={host}\nEnvironment=ADSP_LIBRARY_PATH={dsp};/usr/lib/dsp/cdsp\n",
                host = host.display(),
                dsp = dsp.display(),
            ),
            Err(e) => {
                // A previous install may have left the loop running; stopping
                // it is the useful half of this branch.
                systemctl_try(&["disable", "--now", "virtues-qnnd"]).await;
                ui::warn(&format!(
                    "could not obtain the QAIRT runtime libs ({e}) — NPU daemon NOT installed, \
                     so this box has no embedding endpoint and semantic search will \
                     not work. Unpack the QAIRT Community SDK on the box by hand, point \
                     VIRTUES_QNN_LIB_DIR at its lib/aarch64-*-linux-*/ directory, and re-run \
                     this installer."
                ));
                return Ok(());
            }
        },
    };

    let body = QNN_UNIT_TEMPLATE
        .replace("__SUPP_GROUPS__", &supp_groups)
        .replace("__QNN_ENV__", &qnn_env)
        .replace("__BIN__", &bin.display().to_string())
        .replace("__EMBED_BIN__", &qnn_dir.join(&cfg.qnn_embed_bin).display().to_string())
        .replace("__RERANK_BIN__", &qnn_dir.join(&cfg.qnn_rerank_bin).display().to_string())
        .replace("__QNN_DIR__", &qnn_dir.display().to_string());
    fs::write("/etc/systemd/system/virtues-qnnd.service", body)
        .context("writing virtues-qnnd.service")?;
    arm_hardware_watchdog()?;

    // Also applies the watchdog drop-in: `daemon-reload` re-reads system.conf.d
    // (verified on the lab Dragon; no daemon-reexec needed).
    systemctl(&["daemon-reload"], "Install NPU daemon unit").await?;
    systemctl(&["enable", "virtues-qnnd"], "Enable NPU daemon").await?;
    systemctl(&["restart", "virtues-qnnd"], "Start NPU daemon").await
}

/// Where the watchdog drop-in lives. Uninstall removes it by this path, which
/// must match `system_repairs::WATCHDOG_DROPIN` in core.
const WATCHDOG_DROPIN: &str = "/etc/systemd/system.conf.d/10-virtues-watchdog.conf";

/// Have systemd ping the SoC's hardware watchdog, so a hung kernel reboots the
/// box instead of waiting for someone to pull the plug.
///
/// This lives beside the NPU daemon because the NPU is what hangs the kernel.
/// Loading a context binary the cDSP can't map crashes the cDSP, and this
/// kernel's FastRPC driver (6.18.2-3-qcom) then frees reserved DMA pages as the
/// process exits. The box logged `BUG: Bad page state` sixty times and stopped
/// answering on every interface. There was no OOM and no panic, so nothing
/// rebooted it. `qnnd` shares that cDSP, so any Dragon can hit it.
///
/// Dragon only, DIY included, because we are the ones driving its NPU. A
/// server with no NPU work from us keeps its own systemd settings. Skipped
/// when there is no `/dev/watchdog0`.
///
/// 30 s is inside `qcom_wdt`'s 32 s hardware maximum. systemd pings at half the
/// interval, so a healthy box never comes close. `RebootWatchdogSec` keeps its
/// default of 10 min, which already covers a hung shutdown.
fn arm_hardware_watchdog() -> Result<()> {
    if !Path::new("/dev/watchdog0").exists() {
        ui::skip("No hardware watchdog on this board");
        return Ok(());
    }
    install_file(WATCHDOG_DROPIN, WATCHDOG_CONF)?;
    ui::ok("Hardware watchdog on: a hung box reboots itself within 30 s");
    Ok(())
}

/// One copy for the installer (fresh installs) and core's `dragon-watchdog`
/// repair (the existing fleet, on upgrade), so the two can't drift.
const WATCHDOG_CONF: &str = include_str!("../../../virtues-core/src/cli/watchdog.conf");

/// Host-lib directories inside a QAIRT SDK, best first.
///
/// A QAIRT unpack ships the same `libQnnHtp.so` for every target it supports —
/// the lab Dragon has SIX copies, including `x86_64-linux-clang` and
/// `aarch64-android`. Order here is deliberate: `aarch64-oe-linux-gcc11.2` is
/// the variant the lab box has actually been running against (Ubuntu 24.04,
/// glibc 2.39), so it leads despite "oe" suggesting Yocto. The rest follow as
/// plausible aarch64-Linux fallbacks.
const QNN_HOST_LIB_DIRS: &[&str] = &[
    "aarch64-oe-linux-gcc11.2",
    "aarch64-ubuntu-gcc9.4",
    "aarch64-oe-linux-gcc9.3",
    "aarch64-oe-linux-gcc8.2",
];

/// The directory containing a named QNN `.so`, via a bounded `find` under the
/// roots a QAIRT SDK unpack typically lands in (never a full-FS scan).
///
/// Ranked, not `head -1`. This used to take whatever the find emitted first,
/// which is filesystem order — so on a box carrying a full SDK the winner was
/// arbitrary among six candidates, and two of them (x86_64, android) would have
/// pointed `LD_LIBRARY_PATH` at libraries that cannot load on this box at all.
/// It happened to land on a working directory on the lab Dragon; nothing made
/// that reproducible on the next one.
async fn find_lib_dir(name: &str) -> Option<String> {
    let script = format!(
        "find /opt /usr/lib /usr/local/lib /qairt* \"$HOME\" \
         /usr/lib/python3*/dist-packages /usr/local/lib/python3*/dist-packages \
         -maxdepth 8 -name {name} 2>/dev/null"
    );
    let out = Command::new("bash").arg("-c").arg(&script).output().await.ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    rank_lib_dirs(stdout.lines())
}

/// Pick the best directory from `find` hits for a QNN `.so`. Split out from the
/// shell-out so the ranking is testable; see `tests` below for the six-candidate
/// case the lab Dragon actually presents.
fn rank_lib_dirs<'a>(hits: impl Iterator<Item = &'a str>) -> Option<String> {
    let dirs: Vec<&str> = hits
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(|l| Path::new(l).parent()?.to_str())
        // Wrong-platform builds are never a fallback — a mis-set
        // LD_LIBRARY_PATH is worse than no LD_LIBRARY_PATH, because the failure
        // surfaces as an opaque dlopen error at daemon start rather than as the
        // clean "no libs" refusal.
        .filter(|d| !d.contains("x86_64") && !d.contains("android") && !d.contains("windows"))
        .collect();

    // Preferred host triples first.
    for want in QNN_HOST_LIB_DIRS {
        if let Some(d) = dirs.iter().find(|d| d.ends_with(want)) {
            return Some((*d).to_string());
        }
    }
    // The DSP skel arrives through this same function and matches no host
    // triple; its canonical home is the SDK's unsigned dir. Prefer that over a
    // loose copy someone left in a working directory.
    if let Some(d) = dirs.iter().find(|d| d.ends_with("hexagon-v68/unsigned")) {
        return Some((*d).to_string());
    }
    dirs.iter()
        .find(|d| d.contains("aarch64"))
        .or_else(|| dirs.first())
        .map(|d| (*d).to_string())
}

/// Can the dynamic loader resolve `name` with no help from us? `ldconfig -p`
/// prints the ld.so cache, which is the same thing the daemon's `dlopen` will
/// consult — so this answers "would it load if the unit set no LD_LIBRARY_PATH",
/// which a `find` over candidate directories cannot.
async fn loader_has_lib(name: &str) -> bool {
    let script = format!("ldconfig -p 2>/dev/null | grep -qF -- {name}");
    match Command::new("bash").arg("-c").arg(&script).output().await {
        Ok(out) => out.status.success(),
        Err(_) => false,
    }
}

/// Resolve `(LD_LIBRARY_PATH, ADSP_LIBRARY_PATH)` for the QNN daemon. These are
/// DIFFERENT directories in the QAIRT SDK layout: the host libs (`libQnnHtp.so`,
/// `libQnnSystem.so`) live under `lib/<host-triple>/`, but the DSP skel the
/// Hexagon actually loads (`libQnnHtpV68Skel.so`) lives under
/// `lib/hexagon-v68/unsigned/`. Pointing ADSP at the host dir → "Failed to load
/// skel" and the daemon dies (learned on-device). LD = host dir; ADSP = skel dir
/// as a `;`-separated list, with the standard Radxa DSP path `/usr/lib/dsp/cdsp`
/// appended. `VIRTUES_QNN_LIB_DIR` overrides the host dir. `None` → libs are on
/// the default loader path (baked image); the unit needs no env.
async fn detect_qnn_libs(cfg: &InstallConfig) -> Option<(String, String)> {
    let host = match cfg.qnn_lib_dir() {
        Some(dir) => dir,
        None => find_lib_dir("libQnnHtp.so").await?,
    };
    let skel = find_lib_dir("libQnnHtpV68Skel.so").await.unwrap_or_else(|| host.clone());
    Some((host, format!("{skel};/usr/lib/dsp/cdsp")))
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
/// - **`-ngl 0` (CPU).** EmbeddingGemma's activations can't run fp16, so fp16
///   GPU paths force fp32 and are *slower than CPU* (and CPU is fine for
///   background embedding). `--pooling mean` is EmbeddingGemma's pooling.
/// - `-c/-b/-ub 2048` right-sizes context. The model does longer, but our
///   chunks are ≤512 tok, so 2048 is ample and 8K would just bloat KV +
///   compute buffers (~0.5 GB) for unused reach.
/// - `-np 1`: single-tenant box; 1 slot vs the auto-4 saves ~0.9 GB of
///   per-slot buffers (the bigger memory win). Concurrent requests queue,
///   which is fine here.
/// - `--cache-ram 0`: disables the prompt cache (an up-to-8 GB reservation)
///   — useless for embedding, where every input is unique.
/// Together these cut the sidecar from ~2.5 GB RSS to ~1 GB.
/// `__SUPP_GROUPS__` is replaced at install time with a `SupplementaryGroups=`
/// line for whatever GPU groups exist (see `gpu_access_groups`), or removed
/// entirely on a CPU-only host — an undefined supplementary group would make
/// systemd fail the unit (216/GROUP), which is worse than CPU fallback.
const EMBED_UNIT_TEMPLATE: &str = r#"[Unit]
Description=Virtues embedding sidecar (llama-server, embeddinggemma-2)
Documentation=https://virtues.com/docs
After=network.target
# Cap the restart loop (see QNN_UNIT_TEMPLATE): Restart=on-failure at
# RestartSec=5 is 2 starts/10s and never trips systemd's default burst of 5, so
# a permanently-broken sidecar loops forever with no `failed` unit to see.
StartLimitIntervalSec=300
StartLimitBurst=5

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

/// The Dragon NPU daemon unit. `virtues-qnnd` loads the two QAIRT context
/// binaries once and serves the box's llama-compatible HTTP inference contract
/// on loopback :18181/:18182 (its internal engine loop stays on :7788 — see
/// `crates/virtues-qnnd`); `--models-dir` points it at the tokenizers shipped
/// next to the context binaries. `--burst` pins the HTP to its performance
/// power corners. `__SUPP_GROUPS__` grants access to the DSP device node
/// (`/dev/fastrpc-cdsp`, `render` group). `__QNN_ENV__` sets both
/// `LD_LIBRARY_PATH` (host-side `libQnnHtp.so`/`libQnnSystem.so`) and
/// `ADSP_LIBRARY_PATH` (the DSP-side `libQnnHtpV68Skel.so`, loaded onto the
/// Hexagon) to the detected QAIRT lib dir — verified sufficient on-device. Empty
/// when the libs are already on the default loader path (the Dragon's OS image).
///
/// `StartLimit*` caps the restart loop. Without it `Restart=on-failure` at
/// `RestartSec=5` is two starts per ten seconds, which stays under systemd's
/// default burst of five, so a daemon that can never start restarts every five
/// seconds indefinitely — observed at 169k restarts, a 1GB journal and a core
/// of CPU burnt. The tradeoff is deliberate: five failures inside five minutes
/// and the unit stops and stays stopped, which does mean a genuinely transient
/// failure needs a manual `systemctl reset-failed`. A unit sitting visibly in
/// `failed` is a better artifact than one hiding a permanent fault behind an
/// `activating` that never lands.
const QNN_UNIT_TEMPLATE: &str = r#"[Unit]
Description=Virtues NPU inference daemon (virtues-qnnd, Hexagon v68)
Documentation=https://virtues.com/docs
After=network.target
StartLimitIntervalSec=300
StartLimitBurst=5

[Service]
Type=simple
User=virtues
Group=virtues
# The Hexagon DSPs load firmware from the rootfs, but the kernel's first
# request comes at ~0.8s — before the rootfs is mounted — and NOTHING ever
# retries: no unit, no udev hook, nothing. Both remoteprocs then sit offline
# for the life of the boot, and every qnnd start dies in transport setup
# (QNN error 14001) until StartLimit parks the unit. Found on the first
# fresh-board master build, 2026-08-18. The `+` prefix runs this as root and
# outside the sandbox (User=, ProtectKernelTunables= do not apply), which it
# needs to write /sys; it starts any offline DSP and waits for the fastrpc
# node to appear before the daemon reaches for it.
ExecStartPre=+/bin/sh -c 'for r in /sys/class/remoteproc/remoteproc*; do [ "$(cat $r/state 2>/dev/null)" = offline ] && echo start > $r/state || :; done; i=0; while [ ! -e /dev/fastrpc-cdsp ] && [ $i -lt 20 ]; do i=$((i+1)); sleep 0.5; done'
__SUPP_GROUPS____QNN_ENV__ExecStart=__BIN__ __EMBED_BIN__ __RERANK_BIN__ --burst --port 7788 --models-dir __QNN_DIR__
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
/// brand mark (`∴`) and the box-drawing rules in the setup handoff are
/// multibyte UTF-8, and a `LANG=C` minimal image
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
    // very first handoff already renders under a UTF-8 locale. LC_ALL too:
    // a forwarded-but-ungenerated LC_ALL would otherwise override LANG.
    std::env::set_var("LANG", "C.UTF-8");
    std::env::set_var("LC_ALL", "C.UTF-8");
    Ok(())
}

// ────────────────────────────────────────────────────────────────────────
// Firewall — the ports pairing and the app need
// ────────────────────────────────────────────────────────────────────────

/// The server's iroh UDP port (`virtues_iroh::DEFAULT_IROH_PORT`), which the
/// apps use to reach it directly on the same network.
const IROH_PORT: u16 = 51820;

/// Open what a device on the same network needs when a firewall is on:
/// TCP 8000 (pairing, the app's address), mDNS (finding `virtues.local`) and
/// the iroh UDP port. Fedora Server and RHEL turn firewalld on by default,
/// and with it closed, pairing fails while the install reports success.
///
/// Only firewalld or ufw that is already running is touched, and only to add
/// these. A machine with neither, or with a firewall off, is left as it is.
/// A failure warns and names the command, because the server still runs and
/// a device away from home reaches it through the relay.
pub async fn open_firewall() -> Result<()> {
    let quiet_ok = |prog: &str, args: &[&str]| {
        let mut c = std::process::Command::new(prog);
        c.args(args).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        c.status().is_ok_and(|s| s.success())
    };
    let iroh = format!("{IROH_PORT}/udp");

    if quiet_ok("firewall-cmd", &["--state"]) {
        let adds = ["--add-port=8000/tcp".to_string(), "--add-service=mdns".to_string(), format!("--add-port={iroh}")];
        let mut ok = true;
        for flags in [vec!["--permanent".to_string()], vec![]] {
            let args: Vec<&str> = flags.iter().chain(adds.iter()).map(String::as_str).collect();
            ok &= quiet_ok("firewall-cmd", &args);
        }
        if ok {
            ui::ok(&format!("Firewall (firewalld): opened 8000/tcp, mDNS and {iroh}"));
        } else {
            ui::warn(&format!(
                "Couldn't open the firewall for Virtues. Run: sudo firewall-cmd --permanent --add-port=8000/tcp \
                 --add-service=mdns --add-port={iroh} && sudo firewall-cmd --reload"
            ));
        }
        return Ok(());
    }

    let ufw_active = std::process::Command::new("ufw")
        .arg("status")
        .output()
        .is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains("Status: active"));
    if ufw_active {
        let ok = ["8000/tcp", "5353/udp", iroh.as_str()]
            .iter()
            .all(|rule| quiet_ok("ufw", &["allow", rule]));
        if ok {
            ui::ok(&format!("Firewall (ufw): opened 8000/tcp, mDNS (5353/udp) and {iroh}"));
        } else {
            ui::warn(&format!(
                "Couldn't open the firewall for Virtues. Run: sudo ufw allow 8000/tcp && sudo ufw allow 5353/udp && sudo ufw allow {iroh}"
            ));
        }
    }
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
                 Reachable at http://{current}.local:8000, not virtues.local."
            ));
        } else {
            let mut cmd = Command::new("hostnamectl");
            cmd.args(["set-hostname", "virtues"]);
            run_step(&format!("Hostname → virtues (was {current})"), cmd).await?;
        }
    } else {
        ui::skip("Hostname already 'virtues'");
    }

    install_file("/etc/avahi/services/virtues.service", AVAHI_SERVICE)?;

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

    // `applets` is the writable applet tree. It MUST exist and MUST be owned
    // by the service user before first boot: chat authoring does
    // `create_dir_all` under it as `virtues`, and when nothing created it the
    // failure was a bare `mkdir failed: Permission denied` with no indication
    // of which directory or why.
    for sub in &["lake", "models", "applets"] {
        fs::create_dir_all(cfg.data_dir.join(sub))
            .with_context(|| format!("creating {}/{sub}", cfg.data_dir.display()))?;
    }
    migrate_applets_out_of_shipped_tree(cfg)?;
    // chown -R virtues:virtues, EXCEPT the Postgres cluster.
    //
    // Boxes provisioned as Dragons by earlier installers keep their cluster at
    // `<data dir>/postgresql`, symlinked from `/var/lib/postgresql`. This
    // installer no longer moves the cluster there, but a reinstall over one of
    // those boxes still finds it, and it belongs to `postgres`. A recursive
    // chown over the whole data dir takes it too, and Postgres then cannot read
    // its own files:
    //
    //     FATAL: could not open file "global/pg_filenode.map": Permission denied
    //
    // The failure surfaces as `createuser` failing, which reads like a Postgres
    // problem and is really an ownership one. On a box without that directory
    // the prune matches nothing.
    //
    // `-prune` rather than a chown of each sibling: subdirectories here are not
    // a fixed list (lake, models, applets, journal, backups, upgrade
    // staging, and whatever comes next), and enumerating them means the next
    // one added is silently left with root ownership.
    let mut cmd = Command::new("find");
    cmd.args([
        cfg.data_dir.to_str().unwrap(),
        "-path",
        &cfg.data_dir.join("postgresql").display().to_string(),
        "-prune",
        "-o",
        "-exec",
        "chown",
        "virtues:virtues",
        "{}",
        "+",
    ]);
    run_step("chown data dir (not the Postgres cluster)", cmd).await?;

    ui::ok(&format!("Data dir ready ({})", cfg.data_dir.display()));

    // Passwordless sudo for the owner's admin shell. `virtues` is a system
    // account with no password, so interactive sudo has nothing to authenticate
    // against — NOPASSWD is the only thing that works, not just a convenience.
    // Safe here because the only way to reach a `virtues` shell is the web
    // terminal, which takes a paired device with the app's owner proof or
    // the console socket's owner (`console_socket.rs`); there's no local
    // login (shell is /usr/sbin/nologin). Drop-file at 0440, the mode sudo requires.
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

    provision_separation_roles().await?;

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
    run_step("Install pgvector extension", cmd).await?;

    harden_postgres().await
}

/// The NOLOGIN privilege-separation roles the schema depends on — created here
/// as `postgres`, not by the migrations that declare them.
///
/// These are CLUSTER objects, not database objects. Migration 0052
/// (`virtues_face_reader`) and 0054 (`virtues_applet_writer`) each open with a
/// guarded `CREATE ROLE … NOLOGIN`, and migrations run at pool connect as the
/// `virtues` LOGIN role — which `provision_db` above deliberately creates with
/// `--no-createrole`. So the installer handed the server a role and then the
/// server's own schema demanded a privilege that same installer had just
/// withheld: migration 52 aborts, the server exits, the box never comes up.
/// Every box past 0052 hits it. Neither environment that could have caught it
/// does, and for different reasons — dev makes `virtues` a SUPERUSER
/// (`Makefile`), and CI pre-creates both roles as the superuser in its own
/// setup step, added after this same failure surfaced there first.
///
/// Pre-creating as `postgres` keeps `--no-createrole` intact — the login role
/// still cannot mint roles of its own — and turns each migration's guarded
/// CREATE into the no-op it was written to be.
///
/// NOLOGIN here because their passwords derive from the encryption key, which
/// does not exist yet. `set_db_logins` gives them their logins once it does.
///
/// `WITH ADMIN OPTION` is load-bearing, not belt-and-braces. Each migration
/// follows its CREATE with `GRANT <role> TO current_user` so the pool can
/// `SET ROLE` into it, and since PG16 a GRANT on a role requires ADMIN OPTION
/// on that role. Without it the roles exist, the CREATE is skipped, and the
/// migration fails one line later on the GRANT — the same outage wearing a
/// more confusing error.
async fn provision_separation_roles() -> Result<()> {
    // Must agree with virtues-core/migrations: 0052 (face reader), 0054
    // (applet writer). A role declared there without a line here reintroduces
    // exactly the failure this function exists to prevent.
    for role in ["virtues_face_reader", "virtues_applet_writer"] {
        if !psql_exists(&format!("SELECT 1 FROM pg_roles WHERE rolname='{role}'")).await? {
            let mut cmd = Command::new("sudo");
            cmd.args(["-u", "postgres", "psql", "-v", "ON_ERROR_STOP=1", "-c"]);
            cmd.arg(format!("CREATE ROLE {role} NOLOGIN"));
            run_step(&format!("Create Postgres role '{role}'"), cmd).await?;
        } else {
            ui::skip(&format!("Postgres role '{role}' already exists"));
        }

        // Unconditional rather than paired with the create above: a cluster
        // that already carries the role may not carry the grant (a box someone
        // unblocked by hand with `ALTER ROLE virtues CREATEROLE`, or a restore
        // from a dump). Re-granting is idempotent.
        let mut cmd = Command::new("sudo");
        cmd.args(["-u", "postgres", "psql", "-v", "ON_ERROR_STOP=1", "-c"]);
        cmd.arg(format!("GRANT {role} TO virtues WITH ADMIN OPTION"));
        run_step(&format!("Grant '{role}' to 'virtues'"), cmd).await?;
    }
    Ok(())
}

/// Durability tuning for the Postgres cluster.
///
/// Two problems this fixes, both invisible until the day power is yanked from
/// a box in someone's home:
///
///  1. **systemd kills recovery.** The default unit gives Postgres 90s to
///     become ready; after an unclean shutdown WAL replay can take minutes on
///     a large index, so systemd SIGKILLs the server mid-recovery — the exact
///     path to "PANIC: could not locate a valid checkpoint record" that fills
///     the Immich/Home-Assistant forums. A drop-in with `TimeoutStartSec=
///     infinity` lets recovery finish. (The PGDG unit is already `Type=notify`,
///     so it reports ready the instant recovery completes — no fixed guess.)
///
///  2. **Flash wear + WAL volume.** `wal_compression=lz4` shrinks the
///     full-page images that dominate WAL on a write-heavy embedding backfill;
///     stretched checkpoints cut the FPI rate further. Cheap CPU, real flash
///     savings on eMMC/SD/consumer-NVMe without power-loss protection.
///
/// Data checksums (the cheap corruption detector for a hard-unplugged box) can
/// only be set at initdb time, so they're not touched here — that belongs in
/// the cluster-creation path (PG18 defaults them on; a note for the golden
/// image). Idempotent: the drop-in is overwritten, the ALTER SYSTEM settings
/// are last-writer-wins, and a reload (not restart) applies them without
/// disturbing a running box.
async fn harden_postgres() -> Result<()> {
    // Resolve the running PG unit name (PGDG ships `postgresql@NN-main`; the
    // distro meta-unit `postgresql.service` pulls it in). The drop-in has to
    // land on the unit that actually runs the postmaster, so ask systemd which
    // postgresql@* instance is active rather than hardcoding a version.
    let instance = active_pg_instance().await;
    let unit = instance.as_deref().unwrap_or("postgresql");
    let dropin_dir = format!("/etc/systemd/system/{unit}.service.d");
    install_file(
        &format!("{dropin_dir}/virtues-durability.conf"),
        "[Service]\n# Never SIGKILL Postgres mid-WAL-replay after an unclean\n# shutdown — recovery can exceed the 90s default on a large index.\nTimeoutStartSec=infinity\n",
    )?;
    systemctl(&["daemon-reload"], "Postgres: recovery-safe startup timeout").await?;

    // WAL/checkpoint tuning via ALTER SYSTEM (writes postgresql.auto.conf).
    // All reloadable — no restart needed.
    for (k, v) in [
        ("wal_compression", "lz4"),
        ("checkpoint_timeout", "15min"),
        ("checkpoint_completion_target", "0.9"),
        ("max_wal_size", "4GB"),
        ("min_wal_size", "1GB"),
    ] {
        let mut cmd = Command::new("sudo");
        cmd.args([
            "-u", "postgres", "psql", "-c",
            &format!("ALTER SYSTEM SET {k} = '{v}'"),
        ]);
        // Best-effort per setting: an older PG without lz4 WAL compression
        // shouldn't abort the whole install — log and continue.
        if run_step(&format!("Postgres: set {k}={v}"), cmd).await.is_err() {
            ui::warn(&format!("Postgres: could not set {k} (older server?) — skipping"));
        }
    }
    let mut cmd = Command::new("sudo");
    cmd.args(["-u", "postgres", "psql", "-c", "SELECT pg_reload_conf()"]);
    run_step("Postgres: reload config", cmd).await
}

/// The active `postgresql@NN-main` instance unit, if the box uses the PGDG
/// multi-version layout. Returns None on distros where `postgresql.service`
/// is itself the running unit (dnf/RHEL), in which case the caller falls back
/// to that name.
async fn active_pg_instance() -> Option<String> {
    let out = Command::new("systemctl")
        .args(["list-units", "--type=service", "--state=active", "--no-legend", "postgresql@*"])
        .output()
        .await
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.split_whitespace()
        .find(|t| t.starts_with("postgresql@") && t.ends_with(".service"))
        .map(|t| t.trim_end_matches(".service").to_string())
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

/// EmbeddingGemma's official asymmetric prompt formats (unchanged in
/// EmbeddingGemma 2). Facts about a MODEL, so they live where models are
/// configured — not inside the box's binary, where they used to be the fallback
/// for *every* endpoint, silently prefixing a foreign model's inputs with
/// Gemma's format.
const GEMMA_QUERY_PROMPT: &str = "task: search result | query: ";
const GEMMA_DOC_PROMPT: &str = "title: none | text: ";

/// The inference-related env keys, per mode. Dragon: mode marker + the loopback
/// daemon URLs. Bundled: the loopback sidecars + Gemma's prompts. Manual: mode
/// marker, the user's endpoint URLs, plus the fingerprint + dims recorded by
/// `mode::validate_manual` — the runtime re-embeds the probe strings at boot
/// and refuses to serve search against a silently-swapped model.
fn inference_env_keys(
    cfg: &InstallConfig,
    mode: &InferenceMode,
    validation: Option<&ValidationReport>,
) -> Vec<(&'static str, String)> {
    match mode {
        // Dragon: `virtues-qnnd` serves the SAME llama-compatible HTTP contract
        // as the llama-server sidecar (gte-small 384-d embed on the Hexagon
        // NPU), so core needs nothing QNN-specific — just the standard URL. No fingerprint pin (it's our compiled model; the
        // runtime's /v1/models + dim probes cover identity).
        // VIRTUES_QNND_MODELS_DIR stays for the resolution report (which
        // context binaries to list) and the daemon's tokenizer default.
        InferenceMode::Dragon => vec![
            ("VIRTUES_INFERENCE", "dragon".to_string()),
            ("VIRTUES_EMBED_URL", "http://127.0.0.1:18181".to_string()),
            (
                "VIRTUES_QNND_MODELS_DIR",
                cfg.qnn_models_dir().display().to_string(),
            ),
        ],
        // Bundled: the portable CPU llama-server sidecars on loopback, serving
        // EmbeddingGemma 2.
        //
        // Its settings are written HERE, as configuration, because they are facts
        // about a model — not about Virtues. They used to be constants inside the
        // binary (`DRAGON_STORED_DIM = 256`, Gemma's prompt formats as the
        // fallback for every endpoint), which meant the box could only ever run
        // the one model those constants described, and any other model silently
        // got Gemma's prompt glued onto its inputs.
        //
        //   DIMS          not written: stored at the native 768. EmbeddingGemma is
        //                 Matryoshka-trained, so VIRTUES_EMBED_DIMS=256 would
        //                 work, but on the personal-data eval it gave up half the
        //                 recall gain over gte-small for a 3× lighter index.
        //   PROMPTS       Gemma is asymmetric; queries and documents take
        //                 different prefixes. The right prefix is a property of
        //                 the model, so it is named alongside the model.
        InferenceMode::Bundled => {
            let mut keys = vec![
                ("VIRTUES_INFERENCE", "bundled".to_string()),
                ("VIRTUES_EMBED_URL", "http://127.0.0.1:18181".to_string()),
                ("VIRTUES_EMBED_QUERY_PROMPT", quote_env_value(GEMMA_QUERY_PROMPT)),
                ("VIRTUES_EMBED_DOC_PROMPT", quote_env_value(GEMMA_DOC_PROMPT)),
            ];
            // An accelerator search isn't using yet: Settings → Search keeps
            // pointing at its guide after install (api::search_status).
            if let Some(a) = crate::accel::detect().first() {
                keys.push(("VIRTUES_ACCELERATOR", quote_env_value(&a.label)));
                keys.push(("VIRTUES_ACCELERATOR_GUIDE", a.kind.guide_url().to_string()));
            }
            keys
        }
        InferenceMode::Manual { embed_url, embed_model } => {
            let mut keys = vec![
                ("VIRTUES_INFERENCE", "manual".to_string()),
                ("VIRTUES_EMBED_URL", embed_url.clone()),
                ("VIRTUES_EMBED_MODEL", embed_model.clone()),
            ];
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
         VIRTUES_PDFIUM_PATH={pdfium_path}\n\
         VIRTUES_APPLETS_DIR={applets_dir}\n\
         VIRTUES_APPLET_STATE_DIR={applet_state_dir}\n\
         VIRTUES_APPLETS_BIN_DIR={applets_bin_dir}\n\
         INSTALL_PREFIX={install_prefix}\n",
        install_prefix = cfg.install_prefix.display(),
        static_dir = cfg.web_dir().display(),
        pdfium_path = cfg.pdfium_lib_path().display(),
        storage_path = cfg.data_dir.join("lake").display(),
        atlas = cfg.atlas_url,
        api = cfg.virtues_api_url,
        models_dir = cfg.models_dir().display(),
        applets_dir = cfg.applets_dir().display(),
        applet_state_dir = cfg.applet_state_dir().display(),
        applets_bin_dir = cfg.applets_bin_dir().display(),
    );
    for (k, v) in inference_env_keys(cfg, mode, validation) {
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
        ("VIRTUES_APPLETS_DIR", cfg.applets_dir().display().to_string()),
        ("VIRTUES_APPLET_STATE_DIR", cfg.applet_state_dir().display().to_string()),
        ("VIRTUES_APPLETS_BIN_DIR", cfg.applets_bin_dir().display().to_string()),
    ];
    want.extend(inference_env_keys(cfg, mode, validation));

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
    run_step("Load env + run virtues bringup", cmd).await?;
    set_db_logins(cfg).await;
    Ok(())
}

/// Give the two restricted Postgres roles their logins before the server
/// first starts. SQL that chat or an applet writes logs in as them, and a box
/// refuses to run that SQL without them (core's `database::restricted`).
///
/// This runs core's own `db-logins` system repair, the one every upgrade
/// runs, so the installer holds no second copy of it. `virtues migrate`
/// started as root runs the system repairs before it drops to the service
/// user, and `--check` applies no migration. It needs the key in the env
/// file, which is why it comes after `write_env_file`, and root, because the
/// server's role has no CREATEROLE. A failure is not fatal, since the box
/// still serves everything else. `health_check` reports it.
async fn set_db_logins(cfg: &InstallConfig) {
    let bin = cfg.binary_path();
    let out = Command::new(&bin)
        .args(["migrate", "--check"])
        .stdin(std::process::Stdio::null())
        .output()
        .await;
    match out {
        Ok(o) => {
            // The repairs print one line each when they change or fail
            // something, in the same shape as this installer's own lines.
            for line in String::from_utf8_lossy(&o.stdout).lines().filter(|l| l.contains("system repair `")) {
                println!("{line}");
            }
            if !o.status.success() {
                let stderr = String::from_utf8_lossy(&o.stderr);
                ui::warn(&format!(
                    "`virtues migrate --check` failed ({}): {}",
                    o.status,
                    stderr.lines().last().unwrap_or("").trim()
                ));
            }
        }
        Err(e) => ui::warn(&format!("could not run {} migrate --check: {e}", bin.display())),
    }
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
    systemctl(&["daemon-reload"], "Install systemd unit").await
}

const SYSTEMD_UNIT_TEMPLATE: &str = r#"[Unit]
Description=Virtues — your data, on your hardware
Documentation=https://virtues.com/docs
After=postgresql.service network-online.target
Wants=postgresql.service network-online.target
# Cap the restart loop. Without it, a box that cannot serve (a bad build, a
# config it won't accept) burns one 120s TimeoutStartSec cycle after another
# forever, with no `failed` unit to see — the exact pathology QNN_UNIT_TEMPLATE
# fixed for one unit and left on the rest. 5 in 600s, then park.
StartLimitIntervalSec=600
StartLimitBurst=5

# The data directory may be its own filesystem (a separate data disk, as on
# boxes provisioned as Dragons). The app must not start without it mounted.
# Otherwise Postgres initdb's a fresh empty cluster onto the boot disk and the
# box looks healthy while being empty. When the data directory is a plain
# directory on the root filesystem, this only orders the unit after that mount.
RequiresMountsFor=__DATA_DIR__

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
# The box is a single-tenant, owner-operated server; that shell IS the admin
# surface and its security boundary is the auth in front of /ws/terminal (a
# paired device with the owner proof, or the console socket's owner), not
# this sandbox. TRADE-OFF: the network-facing app on :8000
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
pub async fn health_check(cfg: &InstallConfig, mode: &InferenceMode) -> Result<u32> {
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

    // Chat and applet SQL, run through the installed binary's own `query`
    // verb, which takes the path chat's `sql_query` takes. A binary that runs
    // that SQL on the restricted logins refuses without them, and its error
    // names the fix. An older binary needs no login, so this reports a
    // problem only where the installed version has one. Env loaded as
    // `run_bringup` loads it.
    let probe = format!(
        "set -a; . '{env_file}'; set +a; exec '{bin}' query 'SELECT 1'",
        env_file = cfg.env_file_path().display(),
        bin = cfg.binary_path().display(),
    );
    let sql = Command::new("sudo")
        .args(["-u", "virtues", "bash", "-c", &probe])
        .stdin(std::process::Stdio::null())
        .output()
        .await?;
    if sql.status.success() {
        ui::ok("Chat and applets can run SQL");
    } else {
        let stderr = String::from_utf8_lossy(&sql.stderr);
        let why = stderr.lines().rev().map(str::trim).find(|l| !l.is_empty()).unwrap_or("no error output");
        ui::warn(&format!(
            "Chat and applets cannot run SQL. `virtues query 'SELECT 1'` failed: {}",
            why.trim_start_matches("error: ")
        ));
        issues += 1;
    }

    if let InferenceMode::Dragon = mode {
        // `install_qnn` declines to install the unit at all when QAIRT is absent,
        // and says so at length there. Repeating it here as "not responding, check
        // journalctl" would send the operator to the logs of a unit that does not
        // exist, so name the real state instead.
        if !Path::new("/etc/systemd/system/virtues-qnnd.service").exists() {
            ui::warn(
                "NPU daemon not installed (no QAIRT runtime on this box) — no embedding \
                 endpoint, so semantic search is unavailable",
            );
            issues += 1;
        } else {
            // NPU daemon serving the HTTP contract on loopback. It takes a few
            // seconds to load both context binaries after `systemctl start` — retry.
            let mut up = false;
            // :18181 is the daemon's HTTP contract listener, which only binds after
            // its internal engine loop came up — so one probe proves the whole chain.
            for _ in 0..10 {
                if tokio::net::TcpStream::connect("127.0.0.1:18181").await.is_ok() {
                    up = true;
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            if up {
                ui::ok("NPU daemon serving the inference contract on :18181/:18182");
            } else {
                ui::warn("virtues-qnnd not responding on :18181 — journalctl -u virtues-qnnd");
                issues += 1;
            }
        }
        // Context binaries + tokenizers on disk.
        let qnn_dir = cfg.qnn_models_dir();
        for f in [cfg.qnn_embed_bin.as_str(), cfg.qnn_rerank_bin.as_str()] {
            if qnn_dir.join(f).is_file() {
                ui::ok(&format!("NPU model present: {f}"));
            } else {
                ui::warn(&format!("NPU model missing: {} — re-run the installer", qnn_dir.join(f).display()));
                issues += 1;
            }
        }
    } else if let InferenceMode::Bundled = mode {
        // The inference sidecar responding. /health returns 200 only once the
        // model is loaded, so this also catches a bad/missing GGUF. Model load
        // can take a few seconds after `systemctl start` — retry briefly.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(3))
            .build()?;
        for (port, unit) in [(18181u16, "virtues-embed")] {
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

        // The GGUF on disk.
        let p = cfg.models_dir().join(&cfg.embed_gguf);
        if p.is_file() {
            ui::ok(&format!("Model present: {}", cfg.embed_gguf));
        } else {
            ui::warn(&format!("Model missing: {} — re-run the installer", p.display()));
            issues += 1;
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

/// Write `install.json` — the box's topology manifest, the single place that
/// records what shape this install is (which inference profile, which sidecar
/// units exist, where models live). `virtues upgrade`/`doctor` READ this
/// instead of sniffing unit files, so what-to-restart is declared, not
/// guessed (the guessing is what once restarted the wrong sidecars and never
/// restarted qnnd). Rewritten on every install run — the installer is the
/// only writer.
///
/// ## Why `units` lives here
///
/// Three consumers used to each keep their own idea of what an install
/// contains, and all three were wrong in different directions: `uninstall.rs`
/// carried a hardcoded unit list naming units long deleted, and `upgrade.rs`
/// restarted a different subset. So the installer, the thing that actually
/// creates the units, declares the full set once, and the others read it. A
/// unit that stops being installed stops being listed.
pub fn write_install_manifest(cfg: &InstallConfig, mode: &InferenceMode) -> Result<()> {
    let (profile, sidecars): (&str, Vec<&str>) = match mode {
        InferenceMode::Dragon => ("dragon", vec!["virtues-qnnd"]),
        InferenceMode::Bundled => ("bundled", vec!["virtues-embed"]),
        InferenceMode::Manual { .. } => ("manual", vec![]),
    };

    // Every unit this installer writes, in the order a teardown should stop
    // them: the server first, then what the server depends on.
    let mut units: Vec<&str> = vec!["virtues"];
    units.extend(sidecars.iter().copied());

    let manifest = serde_json::json!({
        "profile": profile,
        "sidecars": sidecars,
        "units": units,
        "data_dir": cfg.data_dir,
        "models_dir": cfg.models_dir(),
        "written_by": env!("CARGO_PKG_VERSION"),
    });
    let path = cfg.share_virtues_dir().join("install.json");
    fs::write(&path, serde_json::to_vec_pretty(&manifest).expect("manifest serializes"))
        .with_context(|| format!("writing {}", path.display()))?;
    ui::ok(&format!("Wrote topology manifest → {}", path.display()));
    Ok(())
}

/// One-time rescue: move authored applets out of the shipped tree.
///
/// Before the state root existed, chat-authored applets were written into
/// `<share>/virtues/{applets,actions}/user/` — inside the tree the installer
/// replaces on every release. Boxes built that way still have them there, and
/// leaving them puts user data one slot-flip away from being displaced.
///
/// Runs as root, which is what makes it possible at all: the shipped tree is
/// root-owned, so virtues-core (running as `virtues`) cannot move anything out
/// of it itself. Merges rather than clobbers, and never overwrites something
/// already in the state root — a slug present in both means the state copy is
/// the live one.
///
/// REMOVE AFTER: every box installed before 2026-07-21 (when the state root
/// arrived) has run an installer that includes this. Then delete this function
/// and its call.
fn migrate_applets_out_of_shipped_tree(cfg: &InstallConfig) -> Result<()> {
    let dest_root = cfg.applet_state_dir().join("user");
    let mut moved = 0usize;

    for legacy_parent in ["applets", "actions"] {
        let src_root = cfg.share_virtues_dir().join(legacy_parent).join("user");
        if !src_root.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&src_root)
            .with_context(|| format!("reading {}", src_root.display()))?
            .flatten()
        {
            let src = entry.path();
            if !src.is_dir() {
                continue;
            }
            let dest = dest_root.join(entry.file_name());
            if dest.exists() {
                ui::warn(&format!(
                    "applet {} already present in the state dir — leaving the copy at {} alone",
                    dest.display(),
                    src.display()
                ));
                continue;
            }
            fs::create_dir_all(&dest_root)
                .with_context(|| format!("creating {}", dest_root.display()))?;
            fs::rename(&src, &dest)
                .with_context(|| format!("moving {} -> {}", src.display(), dest.display()))?;
            moved += 1;
        }
        // Only removes the now-empty `user/` shell; a non-empty one (something
        // was skipped above) is left for the operator to look at.
        let _ = fs::remove_dir(&src_root);
    }

    if moved > 0 {
        ui::ok(&format!(
            "Moved {moved} authored applet(s) into {}",
            dest_root.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::rank_lib_dirs;

    /// The exact shape the lab Dragon presents: one QAIRT unpack, six copies of
    /// `libQnnHtp.so`, emitted by `find` in filesystem order. The old `head -1`
    /// took whichever came first, so a re-extract could silently hand the daemon
    /// x86_64 or Android libraries.
    #[test]
    fn host_libs_never_resolve_to_the_wrong_platform() {
        let base = "/qairt-extract/qairt/2.42.0.251225/lib";
        // Deliberately worst-case order: the two unusable builds lead.
        let hits = [
            format!("{base}/x86_64-linux-clang/libQnnHtp.so"),
            format!("{base}/aarch64-android/libQnnHtp.so"),
            format!("{base}/aarch64-ubuntu-gcc9.4/libQnnHtp.so"),
            format!("{base}/aarch64-oe-linux-gcc11.2/libQnnHtp.so"),
            format!("{base}/aarch64-oe-linux-gcc9.3/libQnnHtp.so"),
        ];
        assert_eq!(
            rank_lib_dirs(hits.iter().map(String::as_str)),
            Some(format!("{base}/aarch64-oe-linux-gcc11.2")),
            "must pick the variant proven on-device, not the first find hit"
        );
    }

    /// The skel matches no host triple, and boxes accumulate loose copies next
    /// to the context binaries. The SDK's unsigned dir is the canonical one.
    #[test]
    fn skel_prefers_the_sdk_dir_over_a_stray_copy() {
        let hits = [
            "/home/radxa/npu/libQnnHtpV68Skel.so",
            "/qairt-extract/qairt/2.42.0.251225/lib/hexagon-v68/unsigned/libQnnHtpV68Skel.so",
        ];
        assert_eq!(
            rank_lib_dirs(hits.into_iter()),
            Some("/qairt-extract/qairt/2.42.0.251225/lib/hexagon-v68/unsigned".to_string())
        );
    }

    /// An x86-only find result is "no usable libs", which must reach the caller
    /// as None so install_qnn refuses rather than writing a broken unit.
    #[test]
    fn wrong_platform_only_is_not_a_fallback() {
        let hits = ["/opt/qairt/lib/x86_64-linux-clang/libQnnHtp.so"];
        assert_eq!(rank_lib_dirs(hits.into_iter()), None);
    }
}
