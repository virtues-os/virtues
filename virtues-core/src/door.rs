//! Running the door (`virtues-door`), the process strangers holding a link
//! can reach.
//!
//! **It runs only while something is shared.** A box with no live
//! publication answers no one, so the supervisor starts the door when the
//! first link goes live and stops it when the last one is revoked or expires.
//! [`wake`] nudges it after a change instead of waiting for the next check.
//!
//! On a box the door is a transient `systemd-run` unit, created through
//! `sudo` as `api::code` creates its sandbox: a throwaway uid
//! (`DynamicUser`), its key in a systemd-managed state directory, the bundle
//! directory bound read-only at `/pub`, and the box's data, the database
//! socket and loopback all out of reach. It reaches the internet, because it
//! homes on the relay, and nothing on the box but its own files.
//!
//! A release build without `systemd-run` does not start the door at all,
//! rather than run it unsandboxed. A debug build (a dev machine, macOS) runs
//! it as a plain child with an empty environment.
//!
//! The door prints `endpoint-id <id>` once it is up; the supervisor stores
//! that where `api::publications` builds links from.

use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use sqlx::PgPool;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Notify;

use crate::api::publications;

/// The transient unit's name on a box.
const UNIT: &str = "virtues-door";

/// How often the supervisor rechecks whether anything is shared, when
/// nothing wakes it sooner (expiry is the case nothing wakes it for).
const RECHECK: Duration = Duration::from_secs(60);

/// The wait before restarting a door that exited on its own, doubled per
/// consecutive failure up to [`MAX_BACKOFF`].
const MIN_BACKOFF: Duration = Duration::from_secs(5);
const MAX_BACKOFF: Duration = Duration::from_secs(300);

fn notify() -> &'static Notify {
    static N: OnceLock<Notify> = OnceLock::new();
    N.get_or_init(Notify::new)
}

/// Tell the supervisor the set of live publications may have changed.
pub fn wake() {
    notify().notify_one();
}

/// Start the supervisor. Call once, from the server's startup.
pub fn maybe_spawn(pool: PgPool) {
    tokio::spawn(async move { supervise(pool).await });
}

async fn anything_shared(pool: &PgPool) -> bool {
    match sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM app_publications \
         WHERE revoked_at IS NULL AND (expires_at IS NULL OR expires_at > now()))",
    )
    .fetch_one(pool)
    .await
    {
        Ok(b) => b,
        Err(e) => {
            // Not knowing is treated as "nothing shared": the door stays shut
            // until the question can be answered.
            tracing::warn!(error = %e, "door: could not check for live publications");
            false
        }
    }
}

async fn supervise(pool: PgPool) {
    let mut backoff = MIN_BACKOFF;
    loop {
        if !anything_shared(&pool).await {
            let _ = tokio::time::timeout(RECHECK, notify().notified()).await;
            continue;
        }
        let Some(mut child) = start(&pool).await else {
            tokio::time::sleep(MAX_BACKOFF).await;
            continue;
        };
        tracing::info!("door: started");
        let started = std::time::Instant::now();

        // Run until the door exits, or nothing is shared any more.
        let exited = loop {
            tokio::select! {
                status = child.wait() => break Some(status),
                _ = tokio::time::timeout(RECHECK, notify().notified()) => {
                    if !anything_shared(&pool).await {
                        break None;
                    }
                }
            }
        };
        match exited {
            None => {
                stop(&mut child).await;
                tracing::info!("door: stopped, nothing is shared");
                backoff = MIN_BACKOFF;
            }
            Some(status) => {
                tracing::warn!(?status, "door: exited; restarting");
                stop_unit().await;
                if started.elapsed() > MAX_BACKOFF {
                    backoff = MIN_BACKOFF;
                }
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

/// Launch the door and start reading its stdout for its EndpointId.
async fn start(pool: &PgPool) -> Option<Child> {
    let Some(relay) = crate::relay::relay_for_door(pool).await else {
        tracing::info!("door: the relay is off, so shared links cannot be served");
        return None;
    };
    let bundles = publications::bundles_dir();
    if let Err(e) = std::fs::create_dir_all(&bundles) {
        tracing::error!(error = %e, dir = %bundles.display(), "door: cannot create the bundle directory");
        return None;
    }
    let program = crate::applet_runner::resolve_program("virtues-door");
    let mut cmd = match crate::applet_runner::which_systemd_run() {
        Some(_) => {
            // A unit left by a previous run of the server would make the new
            // one fail to start under the same name.
            stop_unit().await;
            sandboxed(&program, &bundles, relay.as_str())
        }
        None if cfg!(debug_assertions) => direct(&program, &bundles, relay.as_str()),
        None => {
            tracing::error!("door: systemd-run is unavailable; refusing to serve links unsandboxed");
            return None;
        }
    };
    cmd.stdout(std::process::Stdio::piped()).kill_on_drop(true);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, program = %program.display(), "door: could not start");
            return None;
        }
    };
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(id) = line.strip_prefix("endpoint-id ") {
                    record_endpoint_id(id.trim());
                }
            }
        });
    }
    Some(child)
}

fn record_endpoint_id(id: &str) {
    let dir = publications::door_dir();
    let written = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(dir.join("endpoint-id"), id));
    match written {
        Ok(()) => tracing::info!(door = %id, "door: open"),
        Err(e) => tracing::error!(error = %e, "door: could not record its EndpointId"),
    }
}

async fn stop(child: &mut Child) {
    stop_unit().await;
    let _ = child.kill().await;
}

/// Stop the transient unit, if this is a box and one exists.
async fn stop_unit() {
    if crate::applet_runner::which_systemd_run().is_some() {
        let _ = Command::new("sudo")
            .args(["-n", "systemctl", "stop", &format!("{UNIT}.service")])
            .output()
            .await;
    }
}

/// The unit properties, separate so a test can hold the boundary in place.
fn sandbox_properties(bundles: &Path) -> Vec<String> {
    let mut props: Vec<String> = [
        // A throwaway uid: the unit is created through sudo, and a system
        // unit with no user runs as root.
        "DynamicUser=yes",
        // Its key survives restarts here, owned by whatever uid it gets.
        // TODO(2026-10-05): migrate the door key into box backups and restores;
        // losing it ends every link the box has shared.
        "StateDirectory=virtues-door",
        "ProtectSystem=strict",
        "ProtectHome=yes",
        "PrivateTmp=yes",
        "PrivateDevices=yes",
        "ProtectProc=invisible",
        "ProcSubset=pid",
        "NoNewPrivileges=yes",
        "RestrictNamespaces=yes",
        "RestrictRealtime=yes",
        "LockPersonality=yes",
        "SystemCallArchitectures=native",
        "SystemCallFilter=@system-service",
        "SystemCallErrorNumber=EPERM",
        // The internet, for the relay. Netlink lets iroh see interface changes.
        "RestrictAddressFamilies=AF_INET AF_INET6 AF_NETLINK AF_UNIX",
        // Nothing on loopback: not the core's API, not Postgres over TCP.
        "IPAddressDeny=localhost",
        "MemoryMax=256M",
        "TasksMax=64",
        "CPUQuota=100%",
        "Restart=no",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for path in [
        "/var/lib/virtues",
        "/etc/virtues",
        "/var/lib/postgresql",
        "/run/postgresql",
        "/run/dbus",
        "/run/user",
    ] {
        props.push(format!("InaccessiblePaths=-{path}"));
    }
    for path in [
        crate::applet_templates::state_root(),
        crate::storage::lake::lake_root(),
        crate::api::code_env::code_root(),
    ] {
        props.push(format!("InaccessiblePaths=-{}", path.display()));
    }
    // Mounted from the host side, so the hidden paths above do not hide it.
    props.push(format!("BindReadOnlyPaths={}:/pub", bundles.display()));
    props
}

fn sandboxed(program: &Path, bundles: &Path, relay: &str) -> Command {
    let mut cmd = Command::new("sudo");
    cmd.args(["-n", "systemd-run", "--unit", UNIT, "--pipe", "--wait", "--collect", "--quiet"]);
    for p in sandbox_properties(bundles) {
        cmd.args(["-p", &p]);
    }
    cmd.args(["-E", "RUST_LOG=virtues_door=info,iroh=warn", "--"]);
    cmd.arg(program);
    cmd.args(["--root", "/pub", "--key-dir", "/var/lib/virtues-door", "--relay", relay]);
    cmd
}

fn direct(program: &Path, bundles: &Path, relay: &str) -> Command {
    let mut cmd = Command::new(program);
    // Nothing of the server's environment: no database URL, no secrets.
    cmd.env_clear()
        .env("RUST_LOG", "virtues_door=info,iroh=warn")
        .arg("--root")
        .arg(bundles)
        .arg("--key-dir")
        .arg(publications::door_dir())
        .args(["--relay", relay]);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_properties_pin_the_boundary() {
        let props = sandbox_properties(Path::new("/b"));
        for must in [
            "DynamicUser=yes",
            "NoNewPrivileges=yes",
            "ProtectSystem=strict",
            "IPAddressDeny=localhost",
            "InaccessiblePaths=-/var/lib/virtues",
            "InaccessiblePaths=-/run/postgresql",
            "BindReadOnlyPaths=/b:/pub",
        ] {
            assert!(props.iter().any(|p| p == must), "missing {must}");
        }
        assert!(!props.iter().any(|p| p.starts_with("User=")), "a named user replaces the throwaway uid");
        assert!(!props.iter().any(|p| p.starts_with("BindPaths=")), "the door never writes bundles");
    }

    #[test]
    fn the_direct_door_inherits_no_environment() {
        let cmd = direct(Path::new("/bin/virtues-door"), Path::new("/b"), "https://relay.example");
        let envs: Vec<_> = cmd.as_std().get_envs().collect();
        assert!(envs.iter().all(|(k, _)| *k == "RUST_LOG"), "{envs:?}");
    }
}
