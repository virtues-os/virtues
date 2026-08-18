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
    // The web terminal runs its shell inside tmux so a closed tab or a dropped
    // connection detaches instead of killing whatever is running. Without tmux
    // it degrades to a bare shell that dies with the websocket.
    apt_install("tmux (terminal sessions)", &["tmux"]).await?;

    systemctl(&["enable", "--now", "postgresql"], "Enable postgresql").await?;
    systemctl(&["enable", "--now", "avahi-daemon"], "Enable avahi-daemon").await?;
    Ok(())
}

async fn install_deps_dnf() -> Result<()> {
    // UNVERSIONED, unlike the apt path's pinned `postgresql-18`, so a Fedora
    // box gets whatever its release ships — 16 or 17 today. That asymmetry is
    // known and tolerated rather than accidental: Fedora is a DIY-only target,
    // its Postgres is recent enough for everything we use, and there is no PGDG
    // equivalent worth carrying for it.
    //
    // The cost lands in ONE place, so it is worth naming: `pg_dump` output from
    // a newer server cannot be read by an older `pg_restore`. A backup taken on
    // an appliance (18) will not restore onto a Fedora DIY box on 16. If that
    // ever needs to work, this is the line to change.
    dnf_install("Postgres + pgvector", &["postgresql-server", "postgresql-contrib", "pgvector"]).await?;
    dnf_install("Avahi (mDNS)", &["avahi", "nss-mdns"]).await?;
    dnf_install("ca-certificates + curl", &["ca-certificates", "curl"]).await?;
    // See the apt path: tmux is what makes web-terminal sessions survive a
    // dropped connection.
    dnf_install("tmux (terminal sessions)", &["tmux"]).await?;

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

/// libpdfium — native PDF text extraction for the `document_extraction`
/// action (researcher-plan D1). Mode-independent (CPU parse; runs the same on
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
/// llama-server sidecars on our board — but serves the SAME llama-compatible
/// HTTP inference contract on :18181/:18182, so the runtime talks to it through
/// `VIRTUES_EMBED_URL`/`VIRTUES_RERANK_URL` like any other endpoint.
///
/// Depends on two things this installer does NOT produce: the `virtues-qnnd`
/// binary in the tarball (CI must build it for aarch64 with `QNN_SDK_ROOT`) and
/// the Qualcomm QAIRT runtime libs on the appliance image (proprietary — see
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
        // baked appliance image, where the libs sit on the default loader path
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
                let mut cmd = Command::new("systemctl");
                cmd.args(["disable", "--now", "virtues-qnnd"]);
                let _ = cmd.output().await;
                ui::warn(&format!(
                    "could not obtain the QAIRT runtime libs ({e}) — NPU daemon NOT installed, \
                     so this box has no embedding or rerank endpoint and semantic search will \
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

    let mut cmd = Command::new("systemctl");
    cmd.arg("daemon-reload");
    run_step("Install NPU daemon unit", cmd).await?;
    let mut cmd = Command::new("systemctl");
    cmd.args(["enable", "virtues-qnnd"]);
    run_step("Enable NPU daemon", cmd).await?;
    let mut cmd = Command::new("systemctl");
    cmd.args(["restart", "virtues-qnnd"]);
    run_step("Start NPU daemon", cmd).await
}

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
/// when the libs are already on the default loader path (appliance image).
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
    // And to `input`, so `maintenance::reset_button` can read the power key.
    //
    // `/dev/input/event0` is `root:input` mode 660 and the service runs as
    // `virtues`, which is in `video` and `render` and was in nothing else — so
    // the watcher opened the device, failed, and returned. The button would
    // have shipped built, wired, and dead, announcing nothing worse than a
    // warning every sixty seconds. Caught on hardware; it is not reproducible
    // anywhere without a real input node.
    //
    // Unconditional, unlike the GPU groups: `input` exists on every systemd
    // host, and a box with no power key simply never finds one to watch.
    // Supplementary groups are fixed at process start, so this only takes
    // effect on the service restart at the end of this install — which is why
    // it sits here rather than beside the appliance profile.
    {
        let mut cmd = Command::new("usermod");
        cmd.args(["-aG", "input", "virtues"]);
        run_step("Grant 'virtues' input access (the case button)", cmd).await?;
    }

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
    for sub in &["lake", "models", "secrets", "applets"] {
        fs::create_dir_all(cfg.data_dir.join(sub))
            .with_context(|| format!("creating {}/{sub}", cfg.data_dir.display()))?;
    }
    migrate_applets_out_of_shipped_tree(cfg)?;
    // chown -R virtues:virtues, EXCEPT the Postgres cluster.
    //
    // The cluster lives at `<data dir>/postgresql` on a relocated appliance, and
    // it belongs to `postgres` — a recursive chown over the whole data dir takes
    // it too, and Postgres then cannot read its own files:
    //
    //     FATAL: could not open file "global/pg_filenode.map": Permission denied
    //
    // Which is exactly what happened on the test box the first time this ran
    // after the relocation landed: the move succeeded, Postgres served, and then
    // this line four steps later broke it. The failure surfaces as `createuser`
    // failing, which reads like a Postgres problem and is really an ownership
    // one — worth the paragraph, because the next person will meet it as a
    // confusing error about a role.
    //
    // `-prune` rather than a chown of each sibling: subdirectories here are not
    // a fixed list (lake, models, secrets, applets, journal, backups, upgrade
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

/// Appliance-durability tuning for the Postgres cluster.
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
    fs::create_dir_all(&dropin_dir)
        .with_context(|| format!("creating {dropin_dir}"))?;
    fs::write(
        format!("{dropin_dir}/virtues-durability.conf"),
        "[Service]\n# Never SIGKILL Postgres mid-WAL-replay after an unclean\n# shutdown — recovery can exceed the 90s default on a large index.\nTimeoutStartSec=infinity\n",
    )
    .context("writing postgres durability drop-in")?;
    let mut cmd = Command::new("systemctl");
    cmd.arg("daemon-reload");
    run_step("Postgres: recovery-safe startup timeout", cmd).await?;

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

/// The inference-related env keys, per mode.
///
/// Dragon: mode marker + the loopback sidecar defaults. Manual: mode marker,
/// EmbeddingGemma-300M's official asymmetric prompt formats. Facts about a
/// MODEL, so they live where models are configured — not inside the box's binary,
/// where they used to be the fallback for *every* endpoint, silently prefixing a
/// foreign model's inputs with Gemma's format.
const GEMMA_QUERY_PROMPT: &str = "task: search result | query: ";
const GEMMA_DOC_PROMPT: &str = "title: none | text: ";

/// the user's endpoint URLs, plus the fingerprint + dims recorded by
/// `mode::validate_manual` — the runtime re-embeds the probe strings at boot
/// and refuses to serve search against a silently-swapped model.
fn inference_env_keys(
    cfg: &InstallConfig,
    mode: &InferenceMode,
    validation: Option<&ValidationReport>,
) -> Vec<(&'static str, String)> {
    match mode {
        // Dragon: `virtues-qnnd` serves the SAME llama-compatible HTTP contract
        // as the llama-server sidecars (gte-small 384-d embed + colbert MaxSim
        // rerank on the Hexagon NPU), so core needs nothing QNN-specific — just
        // the standard URLs. No fingerprint pin (it's our compiled model; the
        // runtime's /v1/models + dim probes cover identity).
        // VIRTUES_QNND_MODELS_DIR stays for the resolution report (which
        // context binaries to list) and the daemon's tokenizer default.
        InferenceMode::Dragon => vec![
            ("VIRTUES_INFERENCE", "dragon".to_string()),
            ("VIRTUES_EMBED_URL", "http://127.0.0.1:18181".to_string()),
            ("VIRTUES_RERANK_URL", "http://127.0.0.1:18182".to_string()),
            (
                "VIRTUES_QNND_MODELS_DIR",
                cfg.qnn_models_dir().display().to_string(),
            ),
        ],
        // Bundled: the portable CPU llama-server sidecars on loopback (the
        // throwaway-trial path), serving EmbeddingGemma-300M.
        //
        // Its settings are written HERE, as configuration, because they are facts
        // about a model — not about Virtues. They used to be constants inside the
        // binary (`DRAGON_STORED_DIM = 256`, Gemma's prompt formats as the
        // fallback for every endpoint), which meant the box could only ever run
        // the one model those constants described, and any other model silently
        // got Gemma's prompt glued onto its inputs.
        //
        //   DIMS 256      EmbeddingGemma is Matryoshka-trained: its 768-d output
        //                 truncates to 256 with minimal loss, for a 3× lighter
        //                 index. Truncating a model that is NOT Matryoshka-trained
        //                 destroys it — so this is opt-in, per model, never a
        //                 default.
        //   PROMPTS       Gemma is asymmetric; queries and documents take
        //                 different prefixes. The right prefix is a property of
        //                 the model, so it is named alongside the model.
        InferenceMode::Bundled => vec![
            ("VIRTUES_INFERENCE", "bundled".to_string()),
            ("VIRTUES_EMBED_URL", "http://127.0.0.1:18181".to_string()),
            ("VIRTUES_RERANK_URL", "http://127.0.0.1:18182".to_string()),
            ("VIRTUES_EMBED_DIMS", "256".to_string()),
            ("VIRTUES_EMBED_QUERY_PROMPT", quote_env_value(GEMMA_QUERY_PROMPT)),
            ("VIRTUES_EMBED_DOC_PROMPT", quote_env_value(GEMMA_DOC_PROMPT)),
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

    install_firstboot_unit(cfg)?;

    let mut cmd = Command::new("systemctl");
    cmd.arg("daemon-reload");
    run_step("Install systemd unit", cmd).await?;

    // Ordered Before=virtues.service, so it must be enabled or the ordering
    // never applies — an enabled-but-inert oneshot is the normal steady state.
    let mut en = Command::new("systemctl");
    en.args(["enable", "virtues-firstboot"]);
    run_step("Enable first-boot unit", en).await
}

/// Turn a general-purpose Linux box into a Virtues appliance.
///
/// Everything here was found by provisioning a Dragon by hand and watching what
/// a stock Radxa image does wrong when it is the *only* thing a person will
/// ever see:
///
/// * **`NetworkManager-wait-online` must go.** It blocks boot until a
///   connection comes up, then fails after 30–60s if none does. On a brand-new
///   appliance there is no configured network — that is the entire premise of
///   the onboarding we're about to run — so it is guaranteed to time out on
///   every first boot, adding a minute and a red FAILED to the one screen the
///   owner is watching hardest. A box whose first job is "help me get online"
///   cannot block on being online. Measured: 11.6s → 9.9s and zero failed units.
/// * **The desktop session must go.** GNOME owns the DRM device, so the kiosk
///   cannot have it. Disabled rather than purged: reversible, and the packages
///   cost disk we have plenty of.
/// * **The display kiosk goes in.** `cage` + WebKit on bare DRM — no X, no
///   session, no snap. See the module docs on `DISPLAY_UNIT_TEMPLATE` for why
///   it is not Chromium.
///
/// Idempotent, like every other step: re-running the installer on a working
/// appliance converges.
pub async fn apply_appliance_profile(cfg: &InstallConfig) -> Result<()> {
    // Kiosk runtime. `libwebkit2gtk-4.1-0` is already the Tauri webview on
    // Linux, so this is the same engine the desktop app uses.
    let mut deps = Command::new("apt-get");
    deps.args([
        "install",
        "-y",
        "-qq",
        "cage",
        "seatd",
        "python3-gi",
        "gir1.2-webkit2-4.1",
        "gir1.2-gtk-3.0",
        // BLE provisioning (maintenance::ble_provision): the Improv service
        // needs bluetoothd running. Radxa's image ships it, but the appliance
        // profile must not depend on that staying true.
        "bluez",
    ]);
    deps.env("DEBIAN_FRONTEND", "noninteractive");
    run_step("Install display runtime (cage + WebKit)", deps).await?;

    // BLE provisioning needs bluetoothd up from boot; installing bluez does
    // not reliably enable it on a server image.
    let mut bt = Command::new("systemctl");
    bt.args(["enable", "--now", "bluetooth"]);
    run_step("Enable bluetooth service", bt).await?;

    let mut seat = Command::new("systemctl");
    seat.args(["enable", "--now", "seatd"]);
    let _ = seat.output().await;

    // Let the service user drive NetworkManager.
    //
    // virtues.service runs as `User=virtues`, and polkit refuses networking
    // control to unprivileged users — so `nmcli device wifi hotspot` fails with
    // "Not authorized to control networking" and the setup AP never rises. On a
    // DIY box that is correct and we leave it alone; on an appliance the box IS
    // the network administrator, and there is no human at a console to
    // authenticate to.
    //
    // Scoped to the three actions onboarding actually needs rather than the
    // whole `org.freedesktop.NetworkManager.*` tree: raise the AP, join a
    // network, and persist the resulting connection.
    fs::create_dir_all("/etc/polkit-1/rules.d").context("mkdir polkit rules.d")?;
    fs::write("/etc/polkit-1/rules.d/50-virtues-network.rules", POLKIT_NETWORK_RULE)
        .context("writing polkit network rule")?;
    ui::ok("NetworkManager control granted to the virtues user");

    // The data disk is real on an appliance, so Postgres must wait for it.
    install_postgres_mount_guard(cfg)?;

    // Hand the power key to us.
    //
    // The button behind the case is the appliance's only physical control, and
    // logind owns it by default — so the first press powers the box off, which
    // is both the wrong action and an unrecoverable one for an owner who has
    // opened the case precisely because they cannot reach their box.
    //
    // `ignore` rather than a different logind action, because none of logind's
    // options is what we want: it can power off, reboot, suspend, hibernate or
    // lock, and cannot run this. `maintenance::reset_button` reads the evdev
    // node itself once logind stops consuming the key.
    //
    // A drop-in, so an apt upgrade of systemd does not overwrite it, and so the
    // reason is legible next to the setting rather than buried in a vendor file.
    fs::create_dir_all("/etc/systemd/logind.conf.d").context("mkdir logind.conf.d")?;
    fs::write("/etc/systemd/logind.conf.d/10-virtues-power-key.conf", LOGIND_POWER_KEY)
        .context("writing the logind power-key drop-in")?;
    // reload-or-restart rather than restart: restarting logind on a box with an
    // active session kills it.
    let mut reload = Command::new("systemctl");
    reload.args(["reload-or-restart", "systemd-logind"]);
    let _ = reload.output().await;
    ui::ok("Power key handed to Virtues (hold 3s to forget devices)");

    // Retire the captive-portal plumbing, on every run.
    //
    // Two artifacts used to go in here so a phone joining the setup AP would
    // have its connectivity probe answered by the box and a captive sheet
    // opened onto `/provision`: a dnsmasq drop-in resolving EVERY name to
    // 10.42.0.1, and a unit that added an iptables :80 → :8000 REDIRECT at
    // boot. Both are gone with `/portal` (see `server/mod.rs`) — the browser
    // flow they served could provision wifi and then strand the owner, because
    // pairing needs a held iroh key that a browser tab does not have.
    //
    // Removed rather than merely not-written, because an appliance built
    // before this shipped still has them, and a reinstall is the moment we can
    // reach them. A wildcard-DNS drop-in and a boot-time NAT rule for a subnet
    // that no longer comes up are two loaded guns aimed at whichever future
    // network happens to reuse 10.42.0.0/24.
    retire_captive_artifacts().await;

    // Boot: no display manager, no waiting on a network we don't have, and no
    // second update channel.
    //
    // `systemd-sysupdate` is the vendor image's own OS auto-updater. It was
    // found ENABLED AND FAILING on the lab board — harmless there only because
    // it has no config to act on. Masked rather than disabled: a distro package
    // update can re-enable a disabled unit, and the entire argument for WebKit
    // over Chromium was refusing to put a self-updating release channel
    // underneath ours. It applies at least as strongly to one that updates the
    // whole operating system.
    for args in [
        vec!["disable", "NetworkManager-wait-online.service"],
        vec!["mask", "systemd-sysupdate.timer"],
        vec!["mask", "systemd-sysupdate.service"],
        vec!["mask", "systemd-sysupdate-reboot.timer"],
        vec!["mask", "systemd-sysupdate-reboot.service"],
        vec!["disable", "gdm"],
        vec!["disable", "gdm3"],
        vec!["disable", "sddm"],
        vec!["disable", "lightdm"],
        vec!["set-default", "multi-user.target"],
    ] {
        let mut c = Command::new("systemctl");
        c.args(&args);
        // Absent units are the normal case — most boxes have exactly one
        // display manager, or none — so a failure here is not interesting.
        let _ = c.output().await;
    }
    ui::ok("Boot trimmed (no desktop session, no wait-online, no vendor auto-update)");

    // The kiosk shim + unit.
    fs::create_dir_all("/usr/local/lib/virtues").context("mkdir /usr/local/lib/virtues")?;
    fs::write("/usr/local/lib/virtues/display.py", DISPLAY_SHIM)
        .context("writing display.py")?;
    fs::write(
        "/etc/systemd/system/virtues-display.service",
        DISPLAY_UNIT_TEMPLATE.replace("__DATA_DIR__", &cfg.data_dir.display().to_string()),
    )
    .context("writing virtues-display.service")?;

    let mut reload = Command::new("systemctl");
    reload.arg("daemon-reload");
    let _ = reload.output().await;

    let mut en = Command::new("systemctl");
    en.args(["enable", "virtues-display"]);
    run_step("Install display kiosk", en).await
}

/// Tear down the captive-portal artifacts an older appliance install left.
///
/// Best-effort throughout: every step is "remove a thing that is probably not
/// there", and a box that never had them must not see an error. The one part
/// that matters is ordering — stop the unit before deleting it, so its
/// `ExecStop` gets to remove the iptables rule it added. Deleting the unit
/// first would strand a NAT rule with nothing left that knows how to undo it.
async fn retire_captive_artifacts() {
    const UNIT: &str = "virtues-captive-redirect";
    const UNIT_PATH: &str = "/etc/systemd/system/virtues-captive-redirect.service";
    const DNSMASQ_CONF: &str =
        "/etc/NetworkManager/dnsmasq-shared.d/00-virtues-captive.conf";

    let existed = std::path::Path::new(UNIT_PATH).exists()
        || std::path::Path::new(DNSMASQ_CONF).exists();

    for args in [vec!["stop", UNIT], vec!["disable", UNIT]] {
        let mut c = Command::new("systemctl");
        c.args(&args);
        let _ = c.output().await;
    }
    let _ = fs::remove_file(UNIT_PATH);
    let _ = fs::remove_file(DNSMASQ_CONF);

    // The ExecStop above only fires if the unit was loaded and active. Clear
    // the rule directly too — an appliance that was hard-powered mid-life
    // never ran it, and the rule is re-added at every boot by a unit we just
    // deleted, so this is the last chance anything will remove it.
    let mut ipt = Command::new("iptables");
    ipt.args([
        "-t", "nat", "-D", "PREROUTING", "-s", "10.42.0.0/24", "-p", "tcp",
        "--dport", "80", "-j", "REDIRECT", "--to-port", "8000",
    ]);
    let _ = ipt.output().await;

    if existed {
        let mut c = Command::new("systemctl");
        c.arg("daemon-reload");
        let _ = c.output().await;
        ui::ok("Removed the retired captive-portal DNS + :80 redirect");
    }
}

/// The drop-in itself. A raw string like `DISPLAY_UNIT_TEMPLATE` and
/// `SYSTEMD_UNIT_TEMPLATE`, rather than a `format!` with `\n\` continuations —
/// those carry the source's indentation into the generated file, and a systemd
/// drop-in whose `[Unit]` header is indented four spaces reads as broken even
/// though systemd strips it.
const PG_MOUNT_GUARD_TEMPLATE: &str = r#"# Installed by virtues-installer.
#
# The Virtues state root is its own filesystem on an appliance (a blank NVMe
# claimed at first boot). fstab carries `nofail` so a missing disk never blocks
# boot — the box must still come up far enough to say so on its display — but
# Postgres must NOT start without it, or it initdb's a fresh empty cluster onto
# the boot card and every check reports healthy while the owner's record sits
# unmounted on a disk nobody asked for.
#
# The template's own `RequiresMountsFor=/var/lib/postgresql/%I` does not cover
# this: that path is a SYMLINK into the data dir here, and the dependency is
# taken on the path as written, not on what it resolves to.
#
# After= the first-boot unit, which is what CREATES the cluster on a freshly
# claimed disk. Without it Postgres races ahead on a virgin unit, finds nothing,
# and fails — recoverably, but with a red unit on the one screen the owner is
# watching hardest.
#
# ExecStartPre fires ONLY when fstab declares a data disk, and that condition is
# the whole correction. The first version of this asked `mountpoint -q` flatly —
# which bricks a board whose ROOT is already the NVMe and whose state root is a
# directory on it. That is not a hypothetical layout; it is what the lab board
# is, and running the installer on it would have left Postgres refusing to start
# with no way for the owner to find out why. Verified before shipping it.
#
# So: fstab entry means "this box was given a data disk", and then the mount is
# required. No entry means the data lives on the root filesystem by design, and
# there is nothing to wait for.
#
# WHAT THIS DELIBERATELY DOES NOT DO is refuse when a disk that SHOULD be here
# is absent, and that is a trade rather than an oversight. virtues.service waits
# on pg_isready, and the panel is served by virtues.service — so a Postgres that
# refuses takes the display with it, and the owner gets a black screen instead
# of the "Storage disconnected" message written for exactly this moment. A box
# running on the wrong disk is recoverable and says so on the glass; a box that
# will not boot says nothing at all. See `crate::data_disk` for the half that
# reports it.
[Unit]
RequiresMountsFor=__DATA_DIR__
After=virtues-firstboot.service

[Service]
ExecStartPre=/bin/sh -c '! grep -qE "[[:space:]]__DATA_DIR__[[:space:]]" /etc/fstab || mountpoint -q __DATA_DIR__'
"#;

/// Stops logind consuming the power key, so `maintenance::reset_button` can
/// read it. Without this the first press of the only button on the product
/// powers the box off.
const LOGIND_POWER_KEY: &str = r#"# Installed by virtues-installer (appliance profile).
#
# The button behind the case forgets this box's paired devices when it is held
# for three seconds. It does NOT power the box off, and it does not erase
# anything: the record, the network, the account and the four-word phrase all
# survive. See maintenance::reset_button and docs/onboarding-paradigm.md.
#
# HandlePowerKeyLongPress is set too, or logind claims the long press even
# while ignoring the short one - which is exactly the gesture we need.
[Login]
HandlePowerKey=ignore
HandlePowerKeyLongPress=ignore
"#;

/// Lets `User=virtues` raise the setup AP and join a network. See
/// `apply_appliance_profile` for why an appliance needs this and a DIY box
/// must not get it.
const POLKIT_NETWORK_RULE: &str = r#"// Installed by virtues-installer (appliance profile).
// The box administers its own network during onboarding; there is no human at
// a console to authenticate to. Scoped to what onboarding needs, not the whole
// NetworkManager action tree.
polkit.addRule(function(action, subject) {
    if (subject.user !== "virtues") { return undefined; }
    switch (action.id) {
        case "org.freedesktop.NetworkManager.network-control":
        case "org.freedesktop.NetworkManager.wifi.share.protected":
        case "org.freedesktop.NetworkManager.settings.modify.system":
            return polkit.Result.YES;
    }
    return undefined;
});
"#;

/// The kiosk unit.
///
/// **Why WebKit and not Chromium.** On Ubuntu 24.04 arm64 `chromium-browser`
/// resolves to a snap transition stub, which would drag snapd — a second,
/// self-updating release channel — onto an appliance whose whole update story
/// is ours. `cog`/WPE has no arm64 candidate. `libwebkit2gtk-4.1-0` is a
/// first-class deb and is what Tauri already links against on Linux, so the
/// display and the desktop app share an engine.
///
/// **The DRM guard.** The same image ships to boxes with and without a screen,
/// so the unit starts only when a connector actually reports one. Checked in
/// ExecStartPre rather than a `Condition`, because the answer lives in the
/// *contents* of the sysfs file, not in its existence.
///
/// **`-s` is not optional.** Without it cage grabs the keyboard and swallows
/// Ctrl+Alt+F<n>, so there is no way to reach a text console — and on an
/// appliance the kiosk is running at exactly the moments you most need one. It
/// cost us a box: while the setup AP was up (so no network) with the kiosk
/// holding the keyboard (so no console), the only remaining recovery was
/// pulling the power. A physically-present owner must always be able to get a
/// login prompt.
const DISPLAY_UNIT_TEMPLATE: &str = r#"[Unit]
Description=Virtues display (cage + WebKit kiosk)
Documentation=https://virtues.com/docs
After=systemd-user-sessions.service seatd.service
Wants=seatd.service

[Service]
Type=simple
Environment=XDG_RUNTIME_DIR=/run/user/0
Environment=LIBSEAT_BACKEND=seatd
Environment=WLR_BACKENDS=drm
Environment=GDK_BACKEND=wayland
EnvironmentFile=-__DATA_DIR__/virtues.env
ExecStartPre=/bin/sh -c "mkdir -p /run/user/0; chmod 700 /run/user/0; grep -qx connected /sys/class/drm/*/status"
ExecStart=/usr/bin/cage -s -- /usr/bin/python3 /usr/local/lib/virtues/display.py
# The box's own server may still be starting; the shim retries, and a crash
# should put the display back rather than leave a black screen.
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"#;

/// The WebKit shim the kiosk runs.
///
/// **The zoom is not cosmetic and must not be "fixed" to 1.0.** The 7" panel
/// reports itself as 53×30 cm in its EDID — a ~24" monitor — when it is
/// physically 15.5×8.7 cm. Every DPI heuristic in the stack believes the EDID,
/// so WebKit computes ~92 DPI against a real 315, sets devicePixelRatio to 1,
/// and renders the whole UI 3.28× too small: body text lands at 1.4 mm tall,
/// which is unreadable at any distance. Measured with a CSS `10cm` rule that
/// came out 3 cm on glass. Never trust EDID-derived DPI on this hardware.
///
/// Python + GTK because it is what the apt-installable WebKit binding gives us
/// and it is ~15 lines. The intended end state is the Tauri app in kiosk mode,
/// which shares this engine.
const DISPLAY_SHIM: &str = r#"#!/usr/bin/env python3
"""Virtues display kiosk — fullscreen WebKit onto the box's own /display route."""
import os
import gi

gi.require_version("Gtk", "3.0")
# Gdk needs its own require_version even though Gtk pulls it in: without this
# the import resolves to Gdk 4.0 and dies with "version '3.0', but '4.0' is
# already loaded", which surfaces as cage failing to start a session — an error
# that reads like a seat/DRM problem and sends you looking in the wrong place.
gi.require_version("Gdk", "3.0")
gi.require_version("WebKit2", "4.1")
from gi.repository import Gdk, GLib, Gtk, WebKit2  # noqa: E402

URL = os.environ.get("VIRTUES_DISPLAY_URL", "http://localhost:8000/display")
# See DISPLAY_SHIM's Rust-side doc comment: the panel's EDID lies about its
# physical size, so the scale factor is pinned, never derived.
ZOOM = float(os.environ.get("VIRTUES_DISPLAY_ZOOM", "3.28"))

window = Gtk.Window()
window.fullscreen()
window.set_decorated(False)

# NO CACHE. Not a tuning knob — the panel showed a THREE-DAY-OLD UI after an
# upgrade, on 2026-08-10, and survived both a service restart and a power cycle.
# The box serves /display with `last-modified` and no `cache-control`, so WebKit
# is free to cache the shell heuristically; it kept the stale shell, and that
# shell names content-hashed JS chunks, so the whole old page came back from
# disk while the box served the new one. Diagnosing it from a photo of the
# screen cost an hour.
#
# DOCUMENT_VIEWER is WebKit's "disable the cache completely" model. A kiosk
# loading one page from localhost has nothing to gain from a cache and
# everything to lose: an appliance whose screen can lie about its own version
# is worse than one that re-fetches 40KB over loopback on every boot.
context = WebKit2.WebContext.get_default()
context.set_cache_model(WebKit2.CacheModel.DOCUMENT_VIEWER)

view = WebKit2.WebView()
view.set_zoom_level(ZOOM)
# Match the page background so the gap before first paint is the panel's own
# black, not WebKit's default white — a white flash on a dark 7" screen in a
# dim room is the most visible thing the box will ever do.
view.set_background_color(Gdk.RGBA(0.043, 0.059, 0.078, 1.0))


def _retry(*_args):
    """The box's server may still be coming up on first boot. Retry rather than
    parking on WebKit's error page, which an owner would rightly read as
    broken. Returning False from the timeout makes it fire once per failure."""
    GLib.timeout_add_seconds(3, lambda: (view.load_uri(URL), False)[1])
    return True  # we handled it; suppress WebKit's own error page


view.connect("load-failed", _retry)
view.load_uri(URL)

window.add(view)
window.connect("destroy", Gtk.main_quit)
window.show_all()
Gtk.main()
"#;

/// The first-boot oneshot: mint this unit's own encryption key.
///
/// Exists because `virtues deprovision` strips `VIRTUES_ENCRYPTION_KEY` before
/// a box is imaged — a key minted on the master would be baked into the image
/// and shared by every clone, so it has to be minted per unit, here, on the
/// customer's first boot.
///
/// Everything else identity-shaped already self-mints: systemd repopulates an
/// empty `machine-id`, sshd regenerates host keys, and the box's iroh secret is
/// created by `load_or_create_secret` when `virtues.service` first starts. The
/// encryption key is the one secret that must exist *before* the service comes
/// up, because the unit reads it from the env file — hence a separate oneshot
/// ordered `Before=virtues.service` rather than folding it into bringup.
///
/// **It mints only when the marker is present.** A box that lost its key some
/// other way — a botched edit, a half-restored backup — must fail loudly, not
/// receive a fresh key: the old ciphertext is still on disk and still parses,
/// so a silent rotation turns every stored credential into undecryptable
/// garbage with nothing in the logs to say why.
fn install_firstboot_unit(cfg: &InstallConfig) -> Result<()> {
    let data_dir = cfg.data_dir.display().to_string();

    let script = FIRSTBOOT_SCRIPT.replace("__DATA_DIR__", &data_dir);
    fs::write("/usr/local/sbin/virtues-firstboot.sh", script)
        .context("writing /usr/local/sbin/virtues-firstboot.sh")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            "/usr/local/sbin/virtues-firstboot.sh",
            fs::Permissions::from_mode(0o750),
        )
        .context("chmod virtues-firstboot.sh")?;
    }

    fs::write(
        "/etc/systemd/system/virtues-firstboot.service",
        FIRSTBOOT_UNIT_TEMPLATE,
    )
    .context("writing /etc/systemd/system/virtues-firstboot.service")?;
    Ok(())
}

/// The Postgres major version this box has a cluster config for, e.g. `18`.
///
/// Read from `/etc/postgresql`, which is where Debian keeps cluster
/// configuration — deliberately not from the data directory, because this is
/// also called when the data directory is the thing that does not exist yet.
/// Highest version wins if a box somehow carries two.
pub fn pg_cluster_version() -> Option<String> {
    let mut versions: Vec<u32> = fs::read_dir("/etc/postgresql")
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().to_string_lossy().parse::<u32>().ok())
        .collect();
    versions.sort_unstable();
    versions.last().map(|v| v.to_string())
}

/// Where a relocated cluster lives, and the symlink that points at it.
pub fn pg_link_path() -> &'static Path {
    Path::new("/var/lib/postgresql")
}
pub fn pg_relocated_dir(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("postgresql")
}
/// The pre-move copy, kept until an operator removes it. See below.
const PG_PRE_MOVE: &str = "/var/lib/postgresql.pre-move";

/// Move the Postgres cluster onto the data disk.
///
/// ## Why, in one number
///
/// The lab box carried 3.0 GB of Postgres and 8.9 GB of lake. The lake was
/// already on the data disk; Postgres was not — so the busiest writer on the
/// box, the one doing a WAL flush per transaction forever, was landing on the
/// the boot medium — a microSD card on the Q6A, which is the weakest storage on
/// the board and the one that wears out under database load. `storage.rs` warns
/// about exactly this; getting the writes off it is acting on that warning.
///
/// ## Why a symlink rather than `data_directory`
///
/// Debian's `postgresql.conf` has a `data_directory` setting, and pointing it
/// at the data disk is the obvious move. It is the wrong one. That path is also
/// known to `pg_createcluster`, `pg_dropcluster`, `pg_upgradecluster`, the
/// `postgresql@.service` template's own `RequiresMountsFor`, and every apt
/// maintainer script — and each of those would then need to be told, or would
/// quietly disagree with us at the worst moment (a major-version upgrade).
///
/// Symlinking `/var/lib/postgresql` moves the whole tree and leaves every one
/// of those working on vanilla paths that resolve through it. We verified the
/// unit carries no `ProtectSystem`/`ReadWritePaths` sandbox that a symlink out
/// of `/var/lib` would trip.
///
/// ## Why the original is copied and kept, not moved
///
/// This is the only copy of the owner's database. So: stop, **copy**, swap the
/// symlink in, start, and prove it serves — and only then is the original
/// redundant. It is left at `/var/lib/postgresql.pre-move` for the operator to
/// remove, because a rollback that exists is worth more than the disk it costs.
/// `virtues image-check` reports it as a finding, so it cannot ship inside an
/// image by being forgotten.
pub async fn relocate_postgres_to_data_dir(cfg: &InstallConfig) -> Result<()> {
    let link = pg_link_path();
    let dest = pg_relocated_dir(&cfg.data_dir);

    // Already done. Checked on the symlink itself (`symlink_metadata`), because
    // `Path::is_symlink` on a link whose TARGET is missing must still say yes —
    // which is exactly the state a freshly imaged unit is in.
    if let Ok(md) = fs::symlink_metadata(link) {
        if md.file_type().is_symlink() {
            ui::skip(&format!(
                "Postgres already lives on the data disk ({})",
                dest.display()
            ));
            return Ok(());
        }
    }

    let Some(ver) = pg_cluster_version() else {
        ui::warn("No Postgres cluster config in /etc/postgresql — skipping relocation");
        return Ok(());
    };

    fs::create_dir_all(&cfg.data_dir)
        .with_context(|| format!("mkdir {}", cfg.data_dir.display()))?;

    // Stop it. `postgresql@<ver>-main` is `PartOf=postgresql.service`, so
    // stopping the wrapper propagates to the instance — one call, and it is the
    // one an operator would type.
    let mut stop = Command::new("systemctl");
    stop.args(["stop", "postgresql"]);
    run_step("Stop Postgres for the move", stop).await?;

    // Copy. `-a` carries ownership and modes, and both matter: Postgres refuses
    // to start on a data directory that is group- or world-readable.
    if dest.exists() {
        // A previous interrupted run. The cluster we are about to trust must be
        // a complete copy of the one we have, not a merge with a partial one.
        fs::remove_dir_all(&dest)
            .with_context(|| format!("clearing a partial {}", dest.display()))?;
    }
    fs::create_dir_all(&dest).with_context(|| format!("mkdir {}", dest.display()))?;
    let mut cp = Command::new("cp");
    cp.args(["-a", &format!("{}/.", link.display()), &dest.display().to_string()]);
    run_step(
        &format!("Copy the Postgres cluster to {}", dest.display()),
        cp,
    )
    .await?;

    // Swap. Rename rather than delete — until Postgres has actually served from
    // the copy, the original is the only thing we know works.
    let _ = fs::remove_dir_all(PG_PRE_MOVE);
    fs::rename(link, PG_PRE_MOVE)
        .with_context(|| format!("moving {} aside", link.display()))?;
    std::os::unix::fs::symlink(&dest, link)
        .with_context(|| format!("symlink {} -> {}", link.display(), dest.display()))?;

    // Start, and prove it serves. A failure here is recoverable precisely
    // because the original is still there, so say how.
    let mut start = Command::new("systemctl");
    start.args(["start", &format!("postgresql@{ver}-main")]);
    let started = start.status().await.map(|s| s.success()).unwrap_or(false);
    let serving = started && pg_is_ready().await;
    if !serving {
        // Put it back. An installer that leaves a box without a database
        // because it was tidying disk layout is worse than one that never
        // tried.
        let _ = fs::remove_file(link);
        let _ = fs::rename(PG_PRE_MOVE, link);
        let mut back = Command::new("systemctl");
        back.args(["start", &format!("postgresql@{ver}-main")]);
        let _ = back.status().await;
        return Err(anyhow!(
            "Postgres would not serve from {} — rolled back to {}. \
             The cluster is untouched; check `journalctl -u postgresql@{ver}-main`.",
            dest.display(),
            link.display()
        ));
    }

    ui::ok(&format!("Postgres cluster moved to {}", dest.display()));
    ui::warn(&format!(
        "the pre-move copy is at {PG_PRE_MOVE} — remove it once you're satisfied: rm -rf {PG_PRE_MOVE}"
    ));
    Ok(())
}

/// Is Postgres accepting connections?
async fn pg_is_ready() -> bool {
    for _ in 0..30 {
        let ok = Command::new("pg_isready")
            .args(["-q", "-h", "/var/run/postgresql"])
            .status()
            .await
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    false
}

/// Refuse to start Postgres when the data disk is absent.
///
/// `virtues.service` already carries `RequiresMountsFor=<data dir>`, and its
/// comment explains why: without the data disk, Postgres `initdb`s a fresh
/// empty cluster and the box looks perfectly healthy while being empty. But
/// the guard was on the wrong unit. `virtues.service` waits for Postgres
/// (`ExecStartPre` polls `pg_isready`), so Postgres starts FIRST — and by the
/// time our guard declined to run, the empty cluster it was protecting against
/// had already been created.
///
/// A drop-in rather than an edit to the vendor unit: `postgresql@.service` is
/// distro-owned and an apt upgrade would overwrite anything written into it.
///
/// **DIY boxes get nothing.** There is one disk on a self-hosted server by
/// definition, `data_dir` is a plain directory on it, and both checks here
/// would be wrong there — `mountpoint` would fail on a perfectly good install.
/// We are a guest on that machine and its Postgres is not ours to constrain.
fn install_postgres_mount_guard(cfg: &InstallConfig) -> Result<()> {
    let dir = "/etc/systemd/system/postgresql@.service.d";
    fs::create_dir_all(dir).with_context(|| format!("mkdir {dir}"))?;
    let body = PG_MOUNT_GUARD_TEMPLATE
        .replace("__DATA_DIR__", &cfg.data_dir.display().to_string());
    fs::write(format!("{dir}/10-virtues-data-mount.conf"), body)
        .context("writing postgres mount guard drop-in")?;
    ui::ok("Postgres will not start without the data disk");
    Ok(())
}

const FIRSTBOOT_UNIT_TEMPLATE: &str = r#"[Unit]
Description=Virtues first boot — claim the data disk, mint this unit's secrets
Documentation=https://virtues.com/docs
# Before BOTH, and Postgres is the one that is easy to forget. This claims the
# data disk and creates the cluster on it; Postgres starting first would find
# the symlink target missing and fail, and would then need a second, manual
# start after we had fixed it up underneath.
Before=virtues.service postgresql.service
DefaultDependencies=yes

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/local/sbin/virtues-firstboot.sh

[Install]
WantedBy=multi-user.target
"#;

/// Idempotent and self-disarming: it does nothing at all unless
/// `virtues deprovision` left the marker, and it removes the marker once the
/// key is written, so a second boot is a no-op.
const FIRSTBOOT_SCRIPT: &str = r#"#!/bin/sh
# Per-unit first-boot provisioning. Installed by virtues-installer.
#
# Runs on EVERY boot and does nothing on almost all of them. Two independent
# jobs with two independent guards, deliberately NOT sharing one:
#
#   1. claim a blank NVMe   — guarded on "the disk is blank"
#   2. mint an encryption key — guarded on the deprovision marker
#
# They are separate because the risks are opposite. Formatting must never key
# off "this is a fresh unit" (a marker can outlive the state it described, and
# reformatting a disk with data on it is unrecoverable); minting must never key
# off "the key is missing" (that would silently rotate a key a working box
# still needs). Each guard is the narrowest true statement about its own job.
set -eu

DATA_DIR=__DATA_DIR__
ENV_FILE="$DATA_DIR/virtues.env"
MARKER="$DATA_DIR/.needs-firstboot"

# ── 1. Claim a blank NVMe for the data directory ────────────────────────────
# We image the BOOT MEDIUM (a microSD card on the Q6A), not the NVMe, so every
# unit boots with a fresh blank disk and no UUID or LABEL that fstab could have
# been written against. The disk has to be claimed here, on the unit, or Postgres
# and the lake land on the card — which has modest write endurance and is exactly
# what we're trying to keep writes off. See docs/appliance-image.md.
if ! mountpoint -q "$DATA_DIR" 2>/dev/null; then
    for disk in /dev/nvme0n1 /dev/nvme1n1; do
        [ -b "$disk" ] || continue
        # Blank means: no partition table AND no filesystem anywhere on it.
        # `lsblk` over the whole device catches both in one shot; any non-empty
        # output means something is already there and we keep our hands off.
        if [ -z "$(lsblk -no FSTYPE,PTTYPE "$disk" 2>/dev/null | tr -d ' \n')" ]; then
            logger -t virtues-firstboot "claiming blank $disk for $DATA_DIR"
            parted -s "$disk" mklabel gpt mkpart virtues 1MiB 100%
            sleep 2; partprobe "$disk" 2>/dev/null || true; sleep 2
            mkfs.ext4 -q -F -L virtues-data "${disk}p1"
            mkdir -p "$DATA_DIR"
            grep -q '^LABEL=virtues-data' /etc/fstab 2>/dev/null || \
                echo "LABEL=virtues-data $DATA_DIR ext4 defaults,nofail,x-systemd.device-timeout=10s 0 2" >> /etc/fstab
            systemctl daemon-reload
            mount "$DATA_DIR" || logger -t virtues-firstboot "mount $DATA_DIR failed"
            break
        fi
    done
fi

# ── 1b. Send the journal to the data disk ───────────────────────────────────
# journald writes continuously and forever, which makes it the third-largest
# write source on the box after Postgres and the lake — and the only one that
# keeps going when nothing is happening. Left alone it lands in
# /var/log/journal on the boot card: modest endurance, and the one medium we
# cannot let a continuous writer sit on.
#
# A symlink rather than `Storage=` in journald.conf, because the config only
# chooses persistent-vs-volatile, never where. Only when the data dir is really
# mounted — a symlink into an unmounted directory would put the journal on the
# eMMC anyway, under a path that claims otherwise, which is worse than not
# trying. And only when /var/log/journal is not already a symlink, so a
# reboot is a no-op.
if mountpoint -q "$DATA_DIR" 2>/dev/null && [ ! -L /var/log/journal ]; then
    mkdir -p "$DATA_DIR/journal"
    # Move what is already there rather than orphaning it: this runs on the
    # first boot AFTER the disk is claimed, and the boot that claimed the disk
    # logged the claim itself.
    if [ -d /var/log/journal ]; then
        cp -a /var/log/journal/. "$DATA_DIR/journal/" 2>/dev/null || true
        rm -rf /var/log/journal
    fi
    ln -s "$DATA_DIR/journal" /var/log/journal
    systemd-tmpfiles --create --prefix /var/log/journal >/dev/null 2>&1 || true
    systemctl kill --kill-who=main --signal=SIGUSR2 systemd-journald 2>/dev/null || true
    logger -t virtues-firstboot "journal relocated to $DATA_DIR/journal"
fi

# ── 1c. Recreate the Postgres cluster on the claimed disk ───────────────────
# /var/lib/postgresql is a SYMLINK into the data dir on an appliance — the
# installer moved the cluster there so the busiest writer on the box lands on
# the replaceable NVMe rather than the boot card. The image carries the
# symlink; the disk it points at is blank on every unit. So the cluster has to
# be made here, once, on the unit.
#
# The guard is the narrowest true statement about the job, like the other two:
# a symlink whose target holds no cluster. A DIY box has no symlink and skips;
# a second boot has a cluster and skips; a box whose disk failed to mount has
# no symlink target it can write to and skips, leaving Postgres refusing to
# start (see the postgresql@.service drop-in) rather than quietly building a
# fresh empty cluster somewhere nobody meant.
#
# NOT guarded on the first-boot marker. The marker licenses key MINTING, which
# must happen exactly once ever; this must happen once per DISK, and those are
# different events — a replaced NVMe needs a cluster and must not get a new
# encryption key.
PG_VER="$(ls /etc/postgresql 2>/dev/null | sort -n | tail -1)"
PG_LINK=/var/lib/postgresql
if [ -L "$PG_LINK" ] && [ -n "$PG_VER" ] && [ ! -e "$PG_LINK/$PG_VER/main/PG_VERSION" ]; then
    PG_TARGET="$(readlink -f "$PG_LINK" 2>/dev/null || true)"
    if [ -n "$PG_TARGET" ] && mkdir -p "$PG_TARGET" 2>/dev/null; then
        chown postgres:postgres "$PG_TARGET"
        logger -t virtues-firstboot "creating the Postgres cluster on the data disk"
        # Drop first: the image carries /etc/postgresql/$PG_VER/main from the
        # master, and pg_createcluster refuses to write over an existing
        # config. Dropping regenerates it, so the cluster ends up vanilla —
        # same paths, same conf, nothing hand-edited that an apt upgrade could
        # disagree with later.
        pg_dropcluster "$PG_VER" main >/dev/null 2>&1 || true
        if pg_createcluster "$PG_VER" main --start >/dev/null 2>&1; then
            # The role and database the app connects as. Peer auth over the
            # Unix socket maps OS user -> role, so no password exists to set.
            su -s /bin/sh postgres -c "psql -tAc \"SELECT 1 FROM pg_roles WHERE rolname='virtues'\"" \
                2>/dev/null | grep -q 1 || \
                su -s /bin/sh postgres -c "psql -c \"CREATE ROLE virtues WITH LOGIN SUPERUSER\"" >/dev/null 2>&1
            su -s /bin/sh postgres -c "psql -tAc \"SELECT 1 FROM pg_database WHERE datname='virtues'\"" \
                2>/dev/null | grep -q 1 || \
                su -s /bin/sh postgres -c "createdb -O virtues virtues" >/dev/null 2>&1
            su -s /bin/sh postgres -c "psql -d virtues -c 'CREATE EXTENSION IF NOT EXISTS vector'" >/dev/null 2>&1
            # No migrations here. `virtues server` runs them at startup, which
            # keeps ONE migration path for every box rather than a first-boot
            # copy of it that could drift.
            logger -t virtues-firstboot "Postgres cluster $PG_VER/main created on the data disk"
        else
            logger -t virtues-firstboot "pg_createcluster FAILED - the box will not serve until this is fixed"
        fi
    fi
fi

# ── 2. Mint this unit's encryption key ──────────────────────────────────────
[ -e "$MARKER" ] || exit 0

if grep -q '^VIRTUES_ENCRYPTION_KEY=' "$ENV_FILE" 2>/dev/null; then
    # Marker present but a key already exists: do NOT rotate it — that would
    # strand whatever is already encrypted. Just disarm and carry on.
    rm -f "$MARKER"
    logger -t virtues-firstboot "marker present but key already set - disarming, not rotating"
    exit 0
fi

umask 077
KEY="$(openssl rand -base64 32)"
printf 'VIRTUES_ENCRYPTION_KEY=%s\n' "$KEY" >> "$ENV_FILE"
chown virtues:virtues "$ENV_FILE" 2>/dev/null || true
chmod 600 "$ENV_FILE"

rm -f "$MARKER"
logger -t virtues-firstboot "minted per-unit encryption key"
"#;

const SYSTEMD_UNIT_TEMPLATE: &str = r#"[Unit]
Description=Virtues — your data, on your hardware
Documentation=https://virtues.com/docs
After=postgresql.service network-online.target
Wants=postgresql.service network-online.target

# The data directory is its own filesystem on the appliance (a blank NVMe
# claimed at first boot). fstab carries `nofail` so a missing disk never blocks
# boot — the box must still come up far enough to say so on the display — but
# the app must NOT start without it. Otherwise Postgres cheerfully initdb's a
# fresh empty cluster onto the eMMC and the box looks perfectly healthy while
# being empty, which is the same silent-divergence class as a mis-numbered
# migration. `nofail` for the boot, RequiresMountsFor for the app.
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

    if let InferenceMode::Dragon = mode {
        // `install_qnn` declines to install the unit at all when QAIRT is absent,
        // and says so at length there. Repeating it here as "not responding, check
        // journalctl" would send the operator to the logs of a unit that does not
        // exist, so name the real state instead.
        if !Path::new("/etc/systemd/system/virtues-qnnd.service").exists() {
            ui::warn(
                "NPU daemon not installed (no QAIRT runtime on this box) — no embedding or \
                 rerank endpoint, so semantic search is unavailable",
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

/// Write `install.json` — the box's topology manifest, the single place that
/// records what shape this install is (which inference profile, which sidecar
/// units exist, where models live). `virtues upgrade`/`doctor` READ this
/// instead of sniffing unit files, so what-to-restart is declared, not
/// guessed (the guessing is what once restarted the wrong sidecars and never
/// restarted qnnd). Rewritten on every install run — the installer is the
/// only writer.
///
/// ## Why `appliance` and `units` live here
///
/// Three consumers used to each keep their own idea of what an install
/// contains, and all three were wrong in different directions.
/// `setup_ap::is_appliance()` tested for `virtues-display.service` on disk —
/// which gates BLE provisioning, the setup AP and the account requirement off
/// a file that a headless appliance may legitimately not have.
/// `uninstall.rs` carried a hardcoded unit list that still named
/// `virtues-wireguard` (deleted long ago) and had never heard of the display,
/// first-boot or captive units. `upgrade.rs` restarted a third subset.
///
/// So the installer — the thing that actually creates them — declares the
/// full set once, and the others read it. A field added here is a field all
/// three see; a unit that stops being installed stops being listed.
pub fn write_install_manifest(
    cfg: &InstallConfig,
    mode: &InferenceMode,
    appliance: bool,
) -> Result<()> {
    let (profile, sidecars): (&str, Vec<&str>) = match mode {
        InferenceMode::Dragon => ("dragon", vec!["virtues-qnnd"]),
        InferenceMode::Bundled => ("bundled", vec!["virtues-embed", "virtues-rerank"]),
        InferenceMode::Manual { .. } => ("manual", vec![]),
    };

    // Every unit this installer writes, in the order a teardown should stop
    // them: the display first (it renders the server that is about to go),
    // then the server, then what the server depends on.
    let mut units: Vec<&str> = Vec::new();
    if appliance {
        units.push("virtues-display");
    }
    units.push("virtues");
    units.extend(sidecars.iter().copied());
    units.push("virtues-firstboot");

    // Files outside the unit directory that only exist because we put them
    // there. Uninstall needs the list; nothing else should have to know it.
    let mut extra_files: Vec<String> = vec![
        "/usr/local/sbin/virtues-firstboot.sh".to_string(),
    ];
    if appliance {
        extra_files.push("/usr/local/lib/virtues/display.py".to_string());
        extra_files.push("/etc/polkit-1/rules.d/50-virtues-network.rules".to_string());
        extra_files.push(
            "/etc/systemd/system/postgresql@.service.d/10-virtues-data-mount.conf".to_string(),
        );
        extra_files.push("/etc/systemd/logind.conf.d/10-virtues-power-key.conf".to_string());
    }

    let manifest = serde_json::json!({
        "profile": profile,
        // Is this a guided product (our hardware, or `--appliance`) rather
        // than somebody's own Linux server? Decides whether the box may
        // administer its own radio, require an account, and serve Improv.
        "appliance": appliance,
        "sidecars": sidecars,
        "units": units,
        "extra_files": extra_files,
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
