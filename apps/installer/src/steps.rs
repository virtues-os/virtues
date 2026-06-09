//! Discrete install steps — each is an idempotent operation that runs a
//! handful of shell-outs and reports its outcome.
//!
//! The functions in this module are deliberately tiny; the real logic
//! lives in the underlying CLIs (apt, dnf, systemctl, ollama, createdb).
//! We're an orchestration layer + visual identity, not a re-implementation.
//!
//! Every step is idempotent — running the installer twice on a working
//! box must converge, never regress (in particular, never rotate the
//! encryption key, never wipe the lake, never re-issue the box's CA).

use anyhow::{anyhow, Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::ui;

/// Detected target environment.
pub struct Target {
    pub arch: &'static str,
    pub pkg_mgr: PkgMgr,
    pub distro: String,
    pub distro_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkgMgr {
    Apt,
    Dnf,
}

pub fn detect() -> Result<Target> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => return Err(anyhow!("unsupported arch: {other} (need x86_64 or aarch64)")),
    };

    let release = std::fs::read_to_string("/etc/os-release")
        .context("reading /etc/os-release")?;
    let id = field(&release, "ID").unwrap_or_else(|| "unknown".to_string());
    let id_like = field(&release, "ID_LIKE").unwrap_or_default();
    let version = field(&release, "VERSION_ID").unwrap_or_else(|| "0".to_string());

    let pkg_mgr = match id.as_str() {
        "debian" | "ubuntu" => PkgMgr::Apt,
        "fedora" | "rhel" | "centos" | "rocky" | "almalinux" => PkgMgr::Dnf,
        _ => {
            // Best-effort fallback via ID_LIKE.
            if id_like.contains("debian") || id_like.contains("ubuntu") {
                PkgMgr::Apt
            } else if id_like.contains("fedora") || id_like.contains("rhel") {
                PkgMgr::Dnf
            } else {
                return Err(anyhow!(
                    "unsupported distro: {id} (need Debian/Ubuntu/Fedora-family for v1)"
                ));
            }
        }
    };

    Ok(Target {
        arch,
        pkg_mgr,
        distro: id,
        distro_version: version,
    })
}

fn field(s: &str, key: &str) -> Option<String> {
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix(&format!("{key}=")) {
            return Some(rest.trim_matches('"').to_string());
        }
    }
    None
}

/// Run a shell-out with an animated spinner, capturing output to the install
/// log on success. On failure, surface the last 30 lines of output so the
/// user has context without scrolling.
pub async fn run_step(label: &str, mut cmd: Command) -> Result<()> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::with_template("  {spinner:.dim} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    spinner.set_message(label.to_string());
    spinner.enable_steady_tick(Duration::from_millis(100));

    let output = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("spawning {label}"))?;

    spinner.finish_and_clear();
    if output.status.success() {
        ui::ok(label);
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail: Vec<&str> = stderr.lines().rev().take(30).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        eprintln!();
        eprintln!("  ✖ {label} (exit {:?})", output.status.code());
        for line in tail {
            eprintln!("    {line}");
        }
        Err(anyhow!("{label} failed"))
    }
}

/// Stream live progress from a long-running command (e.g. `ollama pull`).
/// Unlike `run_step`, this lets the user see real-time output, then collapses
/// to a single `✓ label` line when done.
pub async fn run_streaming(label: &str, mut cmd: Command) -> Result<()> {
    println!("  ⠋ {label}");
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawning {label}"))?;

    let stdout = child.stdout.take().expect("piped");
    let mut reader = BufReader::new(stdout).lines();
    while let Some(line) = reader.next_line().await? {
        // Re-emit with a leading indent so it visually nests under the label.
        println!("    {line}");
    }

    let status = child.wait().await?;
    if status.success() {
        ui::ok(label);
        Ok(())
    } else {
        Err(anyhow!("{label} failed (exit {:?})", status.code()))
    }
}

/// True iff `cmd` is on $PATH.
pub fn has(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

// Path constants live on `config::InstallConfig` now so advanced operators
// can override via env vars at install time. The const helpers that used
// to live here were dead code after the InstallConfig migration.
