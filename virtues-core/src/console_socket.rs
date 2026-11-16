//! The console: the owner's way in from the server's own machine.
//!
//! The server listens on a Unix socket, [`path`], beside its TCP port. Only
//! a request that arrives on this socket can be the console
//! (`middleware::auth`), and only from a user the server recognizes. The
//! kernel reports the connecting process's uid (`SO_PEERCRED`, read through
//! `UnixStream::peer_cred`), which no client can forge, and [`classify`]
//! maps it:
//!
//! - root, the `virtues` service user, and members of the
//!   [`CONSOLE_GROUP`] group are the owner;
//! - `virtues-agent`, the user an outside agent's SSH key runs as
//!   (`cli/agent_key.rs`), reaches only the routes the data verbs use
//!   ([`agent_may`]);
//! - anyone else is disconnected before a byte of the request is read.
//!
//! A TCP connection from `127.0.0.1` is not the console. On a machine a
//! person installed Virtues on themselves, other logins, containers on the
//! host network, and any service that can be made to fetch a loopback URL
//! all connect from there. The plan is agents/plan/local-access-plan.md.
//!
//! The socket's folder is made at boot by a tmpfiles rule the system repair
//! writes (`cli/system_repairs.rs`, `console-socket`), because the server
//! runs as `virtues` and cannot create a folder under `/run` itself. The
//! socket file is world-connectable on purpose: the uid check is the
//! boundary, so `virtues-agent` needs no group to reach it.
//!
//! A browser over SSH reaches the console the same way, with
//! `ssh -L 8000:/run/virtues/console.sock you@server`: sshd connects as the
//! login, so it is the owner only for a login in [`CONSOLE_GROUP`].

/// Where the server listens. `VIRTUES_CONSOLE_SOCKET` overrides it, for a
/// test or a developer who runs a server of their own.
pub const DEFAULT_PATH: &str = "/run/virtues/console.sock";

/// The group whose members are the owner at the console. The system repair
/// creates it and adds the login that ran `sudo virtues migrate` or the
/// installer.
pub const CONSOLE_GROUP: &str = "virtues-console";

/// The service user, and the outside agent's user (`cli/agent_key.rs`).
const SERVICE_USER: &str = "virtues";
const AGENT_USER: &str = "virtues-agent";

/// Who a console connection is, stamped on each of its requests as an
/// extension. Typed and set only by [`spawn`], never read from a header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleCaller {
    Owner,
    Agent,
}

pub fn path() -> std::path::PathBuf {
    std::env::var_os("VIRTUES_CONSOLE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(DEFAULT_PATH))
}

/// The uids that matter to [`classify`], looked up when a connection comes
/// in, so a group change takes effect without a restart.
#[derive(Debug, Default)]
pub struct Known {
    pub service: Option<u32>,
    pub agent: Option<u32>,
    pub console_members: Vec<u32>,
}

/// Who `uid` is at the console, or `None` to disconnect it. The agent check
/// comes first so that putting `virtues-agent` in the console group by hand
/// still leaves it an agent.
pub fn classify(uid: u32, known: &Known) -> Option<ConsoleCaller> {
    if known.agent == Some(uid) {
        return Some(ConsoleCaller::Agent);
    }
    if uid == 0 || known.service == Some(uid) || known.console_members.contains(&uid) {
        return Some(ConsoleCaller::Owner);
    }
    None
}

/// The routes an agent caller may use: the console door the data verbs
/// call (`/api/console/...`), the applet switch `virtues applet enable`
/// flips (`PATCH /api/applets/<id>`, which takes only `enabled` from an
/// agent: `server/api/applets.rs`), both from `cli/data.rs`'s `Console`, and
/// the MCP endpoint `virtues mcp` relays to (`cli/mcp.rs`).
pub fn agent_may(method: &axum::http::Method, path: &str) -> bool {
    path.starts_with("/api/console/")
        || path == crate::server::api::mcp::PATH
        || (method == axum::http::Method::PATCH
            && path
                .strip_prefix("/api/applets/")
                .is_some_and(|id| !id.is_empty() && !id.contains('/')))
}

#[cfg(unix)]
mod users {
    use std::ffi::{CStr, CString};

    /// The uid of user `name`, or `None` when there is no such user.
    /// The largest buffer a lookup grows to before it gives up. An entry
    /// that does not fit is reported, and counts as absent: the console
    /// then fails closed for that user.
    const MAX_BUF: usize = 1 << 20;

    pub fn uid_of(name: &str) -> Option<u32> {
        let name = CString::new(name).ok()?;
        let mut len = 16 * 1024;
        loop {
            let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
            let mut buf = vec![0 as libc::c_char; len];
            let mut out: *mut libc::passwd = std::ptr::null_mut();
            // SAFETY: every pointer is valid for the call; `out` is null or `&mut pw`.
            let rc = unsafe { libc::getpwnam_r(name.as_ptr(), &mut pw, buf.as_mut_ptr(), buf.len(), &mut out) };
            if rc == libc::ERANGE && len < MAX_BUF {
                len *= 2;
                continue;
            }
            if rc != 0 {
                tracing::warn!(rc, "console socket: could not look up a user");
            }
            return (rc == 0 && !out.is_null()).then_some(pw.pw_uid);
        }
    }

    /// The uids of the members of group `name`: the users it lists, and
    /// those whose primary group it is. Empty when there is no such group.
    pub fn members_of(name: &str) -> Vec<u32> {
        let Ok(cname) = CString::new(name) else { return vec![] };
        let mut len = 64 * 1024;
        let mut gr: libc::group = unsafe { std::mem::zeroed() };
        let mut buf;
        let mut out: *mut libc::group = std::ptr::null_mut();
        loop {
            buf = vec![0 as libc::c_char; len];
            // SAFETY: as in `uid_of`.
            let rc = unsafe { libc::getgrnam_r(cname.as_ptr(), &mut gr, buf.as_mut_ptr(), buf.len(), &mut out) };
            if rc == libc::ERANGE && len < MAX_BUF {
                len *= 2;
                continue;
            }
            if rc != 0 {
                tracing::warn!(rc, "console socket: could not look up the console group");
            }
            if rc != 0 || out.is_null() {
                return vec![];
            }
            break;
        }
        let gid = gr.gr_gid;
        let mut uids = Vec::new();
        let mut member = gr.gr_mem;
        // SAFETY: `gr_mem` is a null-terminated array of C strings in `buf`.
        unsafe {
            while !member.is_null() && !(*member).is_null() {
                if let Some(uid) = CStr::from_ptr(*member).to_str().ok().and_then(uid_of) {
                    uids.push(uid);
                }
                member = member.add(1);
            }
        }
        uids.extend(primary_members(gid));
        uids
    }

    /// Users whose primary group is `gid`, from `/etc/passwd`. A group's own
    /// member list leaves them out.
    fn primary_members(gid: u32) -> Vec<u32> {
        let Ok(passwd) = std::fs::read_to_string("/etc/passwd") else { return vec![] };
        passwd
            .lines()
            .filter_map(|line| {
                let mut f = line.split(':');
                let uid = f.nth(2)?.parse::<u32>().ok()?;
                let g = f.next()?.parse::<u32>().ok()?;
                (g == gid).then_some(uid)
            })
            .collect()
    }
}

#[cfg(unix)]
fn known() -> Known {
    Known {
        service: users::uid_of(SERVICE_USER),
        agent: users::uid_of(AGENT_USER),
        console_members: users::members_of(CONSOLE_GROUP),
    }
}

/// Listen on [`path`] and serve `app` to the callers [`classify`] admits.
/// When the socket cannot be made (no folder, as in a dev checkout, which
/// reaches the server as the owner over loopback under `ENVIRONMENT=dev`),
/// it says so and the server serves TCP alone.
#[cfg(unix)]
pub fn spawn(app: axum::Router) {
    use std::os::unix::fs::PermissionsExt;

    let path = path();
    // A socket left by the last run refuses the bind. Only a socket is
    // removed, never a file someone put there.
    if std::fs::symlink_metadata(&path).is_ok_and(|m| {
        use std::os::unix::fs::FileTypeExt;
        m.file_type().is_socket()
    }) {
        let _ = std::fs::remove_file(&path);
    }
    let listener = match tokio::net::UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            let dev = crate::middleware::auth::is_dev();
            if dev {
                tracing::info!(path = %path.display(), error = %e, "console socket not opened; the CLI reaches this dev server over loopback");
            } else {
                tracing::warn!(path = %path.display(), error = %e, "console socket not opened: the CLI's data verbs and a browser over SSH can't reach this server. Run `sudo virtues migrate` to create its folder");
            }
            return;
        }
    };
    if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)) {
        tracing::warn!(path = %path.display(), error = %e, "console socket: could not set its mode");
    }
    tracing::info!(path = %path.display(), "console socket listening");
    tokio::spawn(accept_loop(listener, app, |uid| classify(uid, &known())));
    // `known()` reads the user and group databases, which on an LDAP or
    // SSSD machine can block: accept_loop runs it per connection, off the
    // runtime's threads.
}

#[cfg(not(unix))]
pub fn spawn(_app: axum::Router) {}

#[cfg(unix)]
pub(crate) async fn accept_loop(
    listener: tokio::net::UnixListener,
    app: axum::Router,
    admit: impl Fn(u32) -> Option<ConsoleCaller> + Send + Sync + 'static,
) {
    let admit = std::sync::Arc::new(admit);
    use hyper::server::conn::http1;
    use hyper_util::rt::TokioIo;
    // axum's Router answers through tower's `Service`, and is always ready.
    use tower::Service;

    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                tracing::warn!(error = %e, "console socket: accept failed");
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                continue;
            }
        };
        let uid = match stream.peer_cred() {
            Ok(cred) => cred.uid(),
            Err(e) => {
                tracing::warn!(error = %e, "console socket: could not read the caller's uid; disconnected");
                continue;
            }
        };
        let app = app.clone();
        let admit = admit.clone();
        tokio::spawn(async move {
            // Who it is, decided on a blocking thread, so a slow user
            // database never stalls the next accept.
            let caller = match tokio::task::spawn_blocking(move || admit(uid)).await {
                Ok(Some(caller)) => caller,
                Ok(None) => {
                    tracing::warn!(uid, "console socket: refused a user that is not the owner's; disconnected");
                    return;
                }
                Err(e) => {
                    tracing::warn!(uid, error = %e, "console socket: could not decide who called; disconnected");
                    return;
                }
            };
            let svc = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                let mut app = app.clone();
                async move {
                    let mut req = req.map(axum::body::Body::new);
                    req.extensions_mut().insert(caller);
                    app.call(req).await
                }
            });
            if let Err(e) = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), svc)
                .with_upgrades()
                .await
            {
                tracing::debug!(error = %e, "console connection ended");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;

    fn known() -> Known {
        Known { service: Some(990), agent: Some(991), console_members: vec![1000] }
    }

    #[test]
    fn root_the_service_user_and_the_console_group_are_the_owner() {
        for uid in [0, 990, 1000] {
            assert_eq!(classify(uid, &known()), Some(ConsoleCaller::Owner), "{uid}");
        }
    }

    #[test]
    fn another_login_is_disconnected() {
        assert_eq!(classify(1001, &known()), None);
        assert_eq!(classify(65534, &known()), None);
    }

    #[test]
    fn the_agent_user_stays_an_agent_even_in_the_console_group() {
        let mut k = known();
        k.console_members.push(991);
        assert_eq!(classify(991, &k), Some(ConsoleCaller::Agent));
    }

    #[test]
    fn a_missing_service_user_admits_nobody_extra() {
        let k = Known::default();
        assert_eq!(classify(990, &k), None);
        assert_eq!(classify(0, &k), Some(ConsoleCaller::Owner));
    }

    /// Over a real socket: an admitted caller's request reaches the app with
    /// its role stamped on it, and a refused caller is disconnected with no
    /// answer at all.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_socket_stamps_the_caller_or_hangs_up() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let app = axum::Router::new().route(
            "/who",
            axum::routing::get(|req: axum::extract::Request| async move {
                format!("{:?}", req.extensions().get::<ConsoleCaller>())
            }),
        );
        let ask = |path: std::path::PathBuf| async move {
            let mut s = tokio::net::UnixStream::connect(&path).await.unwrap();
            s.write_all(b"GET /who HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").await.unwrap();
            let mut out = String::new();
            let _ = s.read_to_string(&mut out).await;
            out
        };

        let dir = tempfile::tempdir().unwrap();
        let admitted = dir.path().join("a.sock");
        let listener = tokio::net::UnixListener::bind(&admitted).unwrap();
        tokio::spawn(accept_loop(listener, app.clone(), |_| Some(ConsoleCaller::Agent)));
        assert!(ask(admitted).await.ends_with("Some(Agent)"));

        let refused = dir.path().join("r.sock");
        let listener = tokio::net::UnixListener::bind(&refused).unwrap();
        tokio::spawn(accept_loop(listener, app, |_| None));
        assert_eq!(ask(refused).await, "");
    }

    #[test]
    fn the_agent_reaches_the_data_verbs_routes_only() {
        assert!(agent_may(&Method::POST, "/api/console/tool/sql_query"));
        assert!(agent_may(&Method::POST, "/api/console/pages/convert"));
        assert!(agent_may(&Method::PATCH, "/api/applets/abc123"));
        assert!(agent_may(&Method::POST, "/api/mcp"));
        assert!(!agent_may(&Method::GET, "/api/applets/abc123"));
        assert!(!agent_may(&Method::PATCH, "/api/applets/abc/runs"));
        assert!(!agent_may(&Method::PATCH, "/api/applets/"));
        assert!(!agent_may(&Method::GET, "/ws/terminal"));
        assert!(!agent_may(&Method::POST, "/api/pair/mint"));
        assert!(!agent_may(&Method::GET, "/api/console"));
    }
}
