//! The sandbox: a transient systemd unit for code the box did not ship.
//!
//! Code written by a model or brought in by an import runs here. Each run
//! is a transient unit created with `sudo -n systemd-run`, which gives:
//!
//! - `PrivateNetwork=yes`: no network. Only `AF_UNIX` is left as an address
//!   family, because asyncio and multiprocessing build socketpairs; the
//!   filesystem sockets the code could otherwise reach (Postgres, D-Bus) are
//!   hidden below. A unit with an egress socket also gets `AF_INET` and
//!   `AF_INET6`, which reach only its own loopback, where [`bridge`] listens.
//! - `InaccessiblePaths` over the box's data: the lake, the state root,
//!   secrets, backups, and those sockets. `ProtectSystem=strict` alone makes
//!   them read-only, not unreadable.
//! - `ProtectProc=invisible` + `ProcSubset=pid`: no other process's command
//!   line.
//! - `MemoryMax` / `MemorySwapMax=0`, `TasksMax`, `CPUQuota`, `LimitFSIZE`:
//!   cgroup and rlimit limits, so a runaway kills the run, not the box.
//! - `RuntimeMaxSec`: a hard timeout enforced by systemd.
//! - `DynamicUser=yes`: a throwaway uid. This is what keeps the code off
//!   root: the unit is created through `sudo`, and a system unit with no user
//!   runs as uid 0. `properties_pin_the_boundary` holds it in place.
//! - `NoNewPrivileges`, `RestrictNamespaces`, `SystemCallFilter`: seccomp.
//!
//! Folders the code needs are bind-mounted in from the host side, so the
//! hidden paths above do not hide them.
//!
//! The way out, when a run has one, is the proxy in [`crate::egress`] on a
//! Unix socket mounted at [`EGRESS_SOCKET`]. Ordinary clients cannot use a
//! Unix-socket proxy, so the unit starts the run under `virtues
//! sandbox-bridge` ([`bridge`]), which listens on the unit's loopback and
//! forwards to the socket, and sets `HTTPS_PROXY` to it.
//!
//! The box user needs passwordless `sudo` (the installer grants it) so that
//! `sudo -n systemd-run` can create a system unit without a polkit agent.

pub mod bridge;

use std::path::PathBuf;

use tokio::process::Command;

/// Where a unit sees its egress socket.
pub const EGRESS_SOCKET: &str = "/run/egress.sock";

/// The loopback port the bridge listens on inside the unit.
pub const BRIDGE_PORT: u16 = 3128;

/// A folder mounted into the unit.
#[derive(Debug, Clone)]
pub struct Bind {
    pub host: PathBuf,
    pub inside: String,
    pub writable: bool,
}

/// One sandboxed run's unit.
#[derive(Debug, Clone)]
pub struct Unit {
    pub timeout_secs: u32,
    /// `MemoryMax`, such as `1G`.
    pub memory_max: &'static str,
    pub working_directory: &'static str,
    pub binds: Vec<Bind>,
    /// The host path of this run's egress proxy socket, when it has one.
    pub egress: Option<PathBuf>,
    /// Leave the Postgres socket reachable. Who the code logs in as is up to
    /// the URL it is given; applet code gets the shared writer login.
    pub postgres: bool,
    /// More properties, such as `EnvironmentFile=` or `LoadCredential=`.
    pub extra: Vec<String>,
}

impl Unit {
    /// The unit's properties. A function of its own so the test can hold the
    /// boundary in place: dropping `DynamicUser` here would run the code as
    /// root.
    pub fn properties(&self) -> Vec<String> {
        let families = if self.egress.is_some() {
            "RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6"
        } else {
            "RestrictAddressFamilies=AF_UNIX"
        };
        let mut props = vec![
            "PrivateNetwork=yes".to_string(),
            families.to_string(),
            format!("MemoryMax={}", self.memory_max),
            "MemorySwapMax=0".to_string(),
            "TasksMax=64".to_string(),
            "CPUQuota=200%".to_string(),
            "LimitFSIZE=256M".to_string(),
            format!("RuntimeMaxSec={}", self.timeout_secs),
            "DynamicUser=yes".to_string(),
            "ProtectSystem=strict".to_string(),
            "ProtectHome=yes".to_string(),
            "PrivateTmp=yes".to_string(),
            "PrivateDevices=yes".to_string(),
            "ProtectProc=invisible".to_string(),
            "ProcSubset=pid".to_string(),
            "NoNewPrivileges=yes".to_string(),
            "RestrictNamespaces=yes".to_string(),
            "RestrictRealtime=yes".to_string(),
            "LockPersonality=yes".to_string(),
            "SystemCallArchitectures=native".to_string(),
            "SystemCallFilter=@system-service".to_string(),
            "SystemCallErrorNumber=EPERM".to_string(),
            // The throwaway uid changes every run; files it leaves must stay
            // usable by the next one and deletable by the server.
            "UMask=0000".to_string(),
            format!("WorkingDirectory={}", self.working_directory),
        ];
        // `-` ignores a path that does not exist, so a dev machine or a box laid
        // out differently still starts the unit.
        for path in ["/var/lib/virtues", "/etc/virtues", "/var/lib/postgresql", "/run/dbus", "/run/user"] {
            props.push(format!("InaccessiblePaths=-{path}"));
        }
        if !self.postgres {
            props.push("InaccessiblePaths=-/run/postgresql".to_string());
        }
        // Wherever this box keeps them, which the fixed list may not name.
        for path in [
            crate::applet_templates::state_root(),
            crate::storage::lake::lake_root(),
            crate::api::code_env::code_root(),
        ] {
            props.push(format!("InaccessiblePaths=-{}", path.display()));
        }
        for b in &self.binds {
            let kind = if b.writable { "BindPaths" } else { "BindReadOnlyPaths" };
            props.push(format!("{kind}={}:{}", b.host.display(), b.inside));
        }
        if let Some(socket) = &self.egress {
            props.push(format!("BindPaths={}:{EGRESS_SOCKET}", socket.display()));
        }
        props.extend(self.extra.iter().cloned());
        props
    }

    /// `sudo -n systemd-run …` running `argv` in the unit, or `None` when
    /// systemd-run is not installed. `env` is set inside the unit; it is on
    /// the command line, so nothing secret goes in it.
    ///
    /// systemd-run resolves `argv[0]` on the host, before the unit's mounts
    /// exist, so a program inside a bind mount is reached through
    /// `/usr/bin/env`.
    pub fn command(&self, argv: &[String], env: &[(&str, &str)]) -> Option<Command> {
        // Behind sudo, systemd-run's absence is a plain non-zero exit, so check
        // for it first to keep the release-build refusal meaningful.
        crate::applet_runner::which_systemd_run()?;
        let mut cmd = Command::new("sudo");
        cmd.args(["-n", "systemd-run"]);
        cmd.args([
            "--pipe",    // wire the unit's stdio to ours
            "--wait",    // block and propagate the exit status
            "--collect", // garbage-collect the transient unit when done
            "--quiet",   // keep systemd-run's own chatter off our stderr
        ]);
        for p in self.properties() {
            cmd.args(["-p", &p]);
        }
        for (k, v) in env {
            cmd.args(["-E", &format!("{k}={v}")]);
        }
        if self.egress.is_some() {
            let proxy = format!("http://127.0.0.1:{BRIDGE_PORT}");
            for k in ["HTTPS_PROXY", "HTTP_PROXY", "https_proxy", "http_proxy"] {
                cmd.args(["-E", &format!("{k}={proxy}")]);
            }
        }
        cmd.arg("--");
        if self.egress.is_some() {
            let exe = std::env::current_exe().ok()?;
            cmd.arg(exe).args(["sandbox-bridge", "--"]);
        }
        cmd.args(argv);
        Some(cmd)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(egress: Option<&str>) -> Unit {
        Unit {
            timeout_secs: 30,
            memory_max: "1G",
            working_directory: "/work",
            binds: vec![
                Bind { host: "/ws".into(), inside: "/work".into(), writable: true },
                Bind { host: "/env".into(), inside: "/opt/py".into(), writable: false },
            ],
            egress: egress.map(PathBuf::from),
            postgres: false,
            extra: Vec::new(),
        }
    }

    #[test]
    fn properties_pin_the_boundary() {
        let props = unit(None).properties();
        for must in [
            "DynamicUser=yes",
            "PrivateNetwork=yes",
            "RestrictAddressFamilies=AF_UNIX",
            "NoNewPrivileges=yes",
            "ProtectSystem=strict",
            "ProtectProc=invisible",
            "MemoryMax=1G",
            "RuntimeMaxSec=30",
            "InaccessiblePaths=-/var/lib/virtues",
            "InaccessiblePaths=-/run/postgresql",
            "UMask=0000",
            "BindPaths=/ws:/work",
            "BindReadOnlyPaths=/env:/opt/py",
        ] {
            assert!(props.iter().any(|p| p == must), "missing {must}");
        }
        assert!(
            !props.iter().any(|p| p.starts_with("User=")),
            "a named user would replace the throwaway uid"
        );
        assert!(!props.iter().any(|p| p.contains(EGRESS_SOCKET)), "no way out unless granted");
    }

    /// The path a client in the unit takes: an ordinary HTTP client with a
    /// proxy setting, through the bridge, into the egress proxy, out to a
    /// listed host. The proxy's own rules are tested in `egress`.
    #[tokio::test]
    async fn a_client_with_a_proxy_setting_reaches_a_listed_host_through_the_bridge() {
        use crate::egress::{Grant, Proxy, Resolver};
        use std::sync::Arc;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let upstream = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = upstream.accept().await.unwrap();
            let mut buf = vec![0u8; 4096];
            let _ = s.read(&mut buf).await.unwrap();
            s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\ntides").await.unwrap();
        });

        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("egress.sock");
        let grant = Grant {
            hosts: ["api.example.com".to_string()].into(),
            resolver: Resolver::Fixed([("api.example.com".to_string(), vec![upstream_addr])].into()),
            check_address: Arc::new(|_| Ok(())),
        };
        let proxy = Proxy::start(&socket, grant).unwrap();

        let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = tcp.local_addr().unwrap().port();
        let bridge_socket = socket.clone();
        std::thread::spawn(move || bridge::serve(tcp, &bridge_socket));

        let client = reqwest::Client::builder()
            .proxy(reqwest::Proxy::http(format!("http://127.0.0.1:{port}")).unwrap())
            .build()
            .unwrap();
        let body = client.get("http://api.example.com/today").send().await.unwrap().text().await.unwrap();
        assert_eq!(body, "tides");

        let refused = client.get("http://elsewhere.example.com/").send().await.unwrap();
        assert_eq!(refused.status(), 403);
        drop(client);
        for _ in 0..100 {
            if proxy.requests().len() >= 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let hosts: Vec<_> = proxy.requests().into_iter().map(|r| (r.host, r.refused.is_some())).collect();
        assert!(hosts.contains(&("api.example.com".to_string(), false)), "{hosts:?}");
        assert!(hosts.contains(&("elsewhere.example.com".to_string(), true)), "{hosts:?}");
    }

    /// Applet code reaches Postgres through its socket, and only through it:
    /// the network stays private.
    #[test]
    fn postgres_leaves_only_the_socket_reachable() {
        let mut u = unit(None);
        u.postgres = true;
        let props = u.properties();
        assert!(!props.iter().any(|p| p == "InaccessiblePaths=-/run/postgresql"));
        assert!(props.iter().any(|p| p == "PrivateNetwork=yes"));
        assert!(props.iter().any(|p| p == "InaccessiblePaths=-/var/lib/postgresql"), "the data directory stays hidden");
    }

    /// An egress socket adds the inet families, which reach only the unit's
    /// own loopback, and mounts the socket. The network stays private.
    #[test]
    fn an_egress_socket_is_the_one_way_out() {
        let props = unit(Some("/run/virtues/egress/run1.sock")).properties();
        assert!(props.iter().any(|p| p == "PrivateNetwork=yes"));
        assert!(props.iter().any(|p| p == "RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6"));
        assert!(props.iter().any(|p| p == &format!("BindPaths=/run/virtues/egress/run1.sock:{EGRESS_SOCKET}")));
    }
}
