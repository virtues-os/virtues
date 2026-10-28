//! Running applet code that did not ship with the box, in the sandbox.
//!
//! Code from an import or written by the assistant runs in a transient unit
//! from [`crate::sandbox`]: a throwaway user, no network (so no route to the
//! box's API on loopback), and the box's data hidden. Into that unit it gets:
//!
//! - its folder, read-only, at `/skill`, which is also its working
//!   directory;
//! - its own lake folder, `lake/applets/<name>/`, writable at `/lake`, with
//!   `STORAGE_PATH` pointing there. The runner indexes the folder into
//!   `lake_objects` after each run, and refuses a run while the folder is
//!   over [`LAKE_FOLDER_LIMIT_BYTES`];
//! - the whole lake read-only at `/data/lake`, only when its `allowed-tools`
//!   lists `lake`;
//! - the Postgres socket, with `DATABASE_URL` logged in as the shared applet
//!   writer (`virtues_applet_writer`), which writes applet schemas and
//!   nothing else;
//! - its credentials as a file, `$CREDENTIALS_DIRECTORY/credentials.json`
//!   (systemd `LoadCredential=`), which only the run can read. The stdin
//!   input carries `"credentials": null`;
//! - its environment, including declared keys, from a file only PID 1 reads;
//! - no network, unless its file asks for hosts with `net(host)` and the
//!   owner has allowed them on the applet's page ([`allowed_hosts`]). Then it
//!   gets an egress proxy for the run ([`crate::egress`]) that reaches only
//!   those hosts, and every request is recorded on the run.
//!
//! Shipped code keeps running directly as the box user, outside all of this.
//! The design is in agents/plan/applet-shape.md, under "A real boundary for
//! code that did not ship" and "The lake".

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::process::Command;

use crate::error::{Error, Result};
use crate::sandbox::{Bind, Unit};

/// The applet's folder inside the unit.
const FOLDER: &str = "/skill";
/// The applet's own lake folder inside the unit.
const LAKE: &str = "/lake";
/// The whole lake inside the unit, for an applet granted `lake`.
const WHOLE_LAKE: &str = "/data/lake";
/// The credential's name under `$CREDENTIALS_DIRECTORY`.
const CREDENTIALS: &str = "credentials.json";

/// Memory ceiling for a sandboxed applet. Generous, because a sync can be
/// memory-hungry, but bounded, so a runaway import cannot take the box out.
const MEMORY_MAX: &str = "1G";

/// The most an applet's lake folder may hold before its runs are refused.
const LAKE_FOLDER_LIMIT_BYTES: i64 = 2 * 1024 * 1024 * 1024;

/// The most files one index pass reads, so a folder of millions of tiny
/// files cannot hold the runner.
const MAX_INDEXED_FILES: usize = 10_000;

/// Where an applet's code runs.
#[derive(Debug, PartialEq)]
pub(super) enum Placement {
    /// Spawned directly as the box user: shipped code, or a developer
    /// machine without systemd.
    Direct,
    Sandbox,
}

/// Decide where code runs. Shipped code runs directly. Elsewhere code runs
/// in the sandbox; off Linux, or in a debug build without systemd, it runs
/// directly because that is a developer machine. In a release build on Linux
/// a missing `systemd-run` is a refusal, matching `code_interpreter`: the
/// moment the sandbox quietly stops applying is the moment it stops being
/// one.
pub(super) fn placement(shipped: bool) -> Result<Placement> {
    if shipped {
        return Ok(Placement::Direct);
    }
    if !cfg!(target_os = "linux") {
        tracing::warn!("running an unshipped applet unsandboxed: no systemd on this platform (developer machine)");
        return Ok(Placement::Direct);
    }
    if super::which_systemd_run().is_none() {
        if cfg!(debug_assertions) {
            tracing::warn!("systemd-run unavailable; running unsandboxed (debug build only)");
            return Ok(Placement::Direct);
        }
        return Err(Error::Other(
            "applet sandbox (systemd-run) is unavailable; refusing to run imported code unsandboxed".to_string(),
        ));
    }
    Ok(Placement::Sandbox)
}

/// The environment a sandboxed run gets. Not the server's: no `DATABASE_URL`
/// of the server's own, no box API key, no paths into the box's data.
pub(super) fn env(run_id: &str, declared: Vec<(String, String)>) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = ["LANG", "LC_ALL", "TZ"]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_string(), v)))
        .collect();
    match crate::database::restricted::socket_url(crate::database::restricted::Login::Writer) {
        Some(url) => out.push(("DATABASE_URL".into(), url)),
        None => tracing::warn!("no secret to derive the applet writer login from; the run gets no DATABASE_URL"),
    }
    out.push(("STORAGE_PATH".into(), LAKE.into()));
    out.push((crate::virtues_api::client::APPLET_RUN_ENV.into(), run_id.into()));
    out.extend(declared);
    out
}

/// The applet's own lake folder on the host, `lake/applets/<name>/`, and the
/// `storage_key` prefix its files are indexed under.
fn lake_folder(applet_id: &str) -> (PathBuf, String) {
    let name = crate::scheduler::applets::applet_slug(applet_id).unwrap_or_else(|| applet_id.to_string());
    let prefix = format!("applets/{name}/");
    (crate::storage::lake::lake_root().join(&prefix), prefix)
}

/// Make the lake folder writable by the run's throwaway user. That user
/// changes every run, so the folder is world-writable inside a lake the
/// unit otherwise cannot see; the sandbox's `UMask=0000` keeps what each run
/// leaves usable by the next.
fn prepare_lake_folder(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir)
        .map_err(|e| Error::Other(format!("could not create the applet's lake folder {}: {e}", dir.display())))?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o777))
        .map_err(|e| Error::Other(format!("could not open the applet's lake folder {}: {e}", dir.display())))
}

/// The hosts this run may reach: the ones its file asks for with `net(host)`
/// that the owner has also allowed. Neither list alone opens anything, so a
/// file the assistant or an import wrote cannot give itself a way out, and
/// the owner cannot hand an applet a host it never asked for. This is the
/// Rule of Two's third leg, which only the owner adds (agents/plan/
/// applet-shape.md, "The Rule of Two, checked at save").
pub(super) async fn allowed_hosts(db: &PgPool, applet_id: &str, config: &serde_json::Value) -> Result<Vec<String>> {
    let requested: Vec<String> = config
        .get("net_hosts")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|h| h.as_str().map(String::from)).collect())
        .unwrap_or_default();
    if requested.is_empty() {
        return Ok(Vec::new());
    }
    let owner: Vec<String> = sqlx::query_scalar("SELECT net_hosts FROM app_applets WHERE id = $1")
        .bind(applet_id)
        .fetch_one(db)
        .await?;
    Ok(requested.into_iter().filter(|h| owner.contains(h)).collect())
}

/// The egress proxy for one run, with its socket in a folder only the box
/// user can open.
pub(super) fn start_proxy(run_id: &str, hosts: &[String]) -> Result<crate::egress::Proxy> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let grant = crate::egress::Grant::new(hosts.iter().map(String::as_str)).map_err(Error::Other)?;
    let dir = crate::data_disk::data_dir().join("egress");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .and_then(|_| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)))
        .map_err(|e| Error::Other(format!("could not create the egress folder {}: {e}", dir.display())))?;
    let path = dir.join(format!("{run_id}.sock"));
    let _ = std::fs::remove_file(&path);
    crate::egress::Proxy::start(&path, grant)
        .map_err(|e| Error::Other(format!("could not start the egress proxy at {}: {e}", path.display())))
}

/// Write what a run's proxy saw onto the run.
pub(super) async fn record_requests(db: &PgPool, run_id: &str, requests: &[crate::egress::Request]) -> Result<()> {
    for r in requests {
        sqlx::query(
            "INSERT INTO app_applet_run_requests (run_id, host, method, sent_bytes, received_bytes, refused_reason)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(run_id)
        .bind(&r.host)
        .bind(&r.method)
        .bind(r.bytes_sent as i64)
        .bind(r.bytes_received as i64)
        .bind(&r.refused)
        .execute(db)
        .await?;
    }
    Ok(())
}

/// The run's command, and the files it needs until it ends.
#[allow(clippy::too_many_arguments)]
pub(super) fn command(
    applet_id: &str,
    program: &Path,
    args: &[String],
    timeout: std::time::Duration,
    env: &[(String, String)],
    credentials: Option<&serde_json::Value>,
    whole_lake: bool,
    egress: Option<&Path>,
) -> Result<(Command, Vec<super::RunFile>)> {
    let folder = crate::applet_templates::dir_for_applet_id(applet_id)
        .map(|d| crate::applet_templates::resolve_applet_dir(&d))
        .filter(|p| p.is_dir());
    let (lake, _) = lake_folder(applet_id);
    prepare_lake_folder(&lake)?;

    let mut binds = vec![Bind { host: lake, inside: LAKE.into(), writable: true }];
    if let Some(folder) = &folder {
        binds.push(Bind { host: folder.clone(), inside: FOLDER.into(), writable: false });
    }
    if whole_lake {
        binds.push(Bind {
            host: crate::storage::lake::lake_root(),
            inside: WHOLE_LAKE.into(),
            writable: false,
        });
    }

    // Secrets reach PID 1 in files rather than on the command line, where
    // any local user could read them in `ps` and sudo writes them to the
    // journal. PID 1 reads them before it builds the unit's namespace, so
    // denying the unit their folder does not stop it.
    let mut files = vec![super::RunFile::write("env", super::env_file_contents(env).as_bytes())?];
    let mut extra = vec![
        format!("EnvironmentFile={}", files[0].path.display()),
        format!("InaccessiblePaths={}", super::RunFile::dir().display()),
    ];
    if let Some(credentials) = credentials {
        let bytes = serde_json::to_vec(credentials)
            .map_err(|e| Error::Other(format!("failed to serialize credentials: {e}")))?;
        let file = super::RunFile::write("json", &bytes)?;
        extra.push(format!("LoadCredential={CREDENTIALS}:{}", file.path.display()));
        files.push(file);
    }

    let unit = Unit {
        timeout_secs: timeout.as_secs() as u32,
        memory_max: MEMORY_MAX,
        working_directory: if folder.is_some() { FOLDER } else { "/tmp" },
        binds,
        egress: egress.map(Path::to_path_buf),
        postgres: true,
        extra,
    };
    // systemd-run resolves argv[0] on the host, before the mounts exist, so
    // the program is reached through env inside the unit.
    let inside = |s: &str| inside_path(s, folder.as_deref());
    let mut argv = vec!["/usr/bin/env".to_string(), inside(&program.to_string_lossy())];
    argv.extend(args.iter().map(|a| inside(a)));
    let cmd = unit
        .command(&argv, &[("HOME", "/tmp")])
        .ok_or_else(|| Error::Other("applet sandbox (systemd-run) is unavailable".to_string()))?;
    Ok((cmd, files))
}

/// A path under the applet's folder on the host, as the unit sees it.
fn inside_path(arg: &str, folder: Option<&Path>) -> String {
    if let Some(folder) = folder {
        if let Ok(rest) = Path::new(arg).strip_prefix(folder) {
            return Path::new(FOLDER).join(rest).to_string_lossy().into_owned();
        }
    }
    arg.to_string()
}

/// A sentence refusing the run while the applet's lake folder is over its
/// limit, or `None`.
pub(super) async fn over_lake_limit(db: &PgPool, applet_id: &str) -> Result<Option<String>> {
    let (_, prefix) = lake_folder(applet_id);
    let used: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(size_bytes), 0)::bigint FROM lake_objects \
         WHERE kind = 'applet' AND storage_key LIKE $1 || '%'",
    )
    .bind(&prefix)
    .fetch_one(db)
    .await?;
    Ok((used > LAKE_FOLDER_LIMIT_BYTES).then(|| {
        format!(
            "This applet's lake folder holds {} MB, over its {} MB limit. Delete files from lake/{prefix} to run it again.",
            used / (1024 * 1024),
            LAKE_FOLDER_LIMIT_BYTES / (1024 * 1024)
        )
    }))
}

/// Index the applet's lake folder into `lake_objects`: a row per file,
/// updated when a file changed, removed when it is gone. Without this a file
/// the code wrote is invisible to the lake's dedupe, retention and replay.
///
/// A file whose size and modified time match its row is not read again.
pub(super) async fn index_lake_folder(db: &PgPool, applet_id: &str) -> Result<usize> {
    let (dir, prefix) = lake_folder(applet_id);
    let files = tokio::task::spawn_blocking(move || list_files(&dir))
        .await
        .map_err(|e| Error::Other(format!("lake index task failed: {e}")))?;

    let known: Vec<(String, i64, serde_json::Value)> = sqlx::query_as(
        "SELECT storage_key, size_bytes, metadata FROM lake_objects WHERE kind = 'applet' AND storage_key LIKE $1 || '%'",
    )
    .bind(&prefix)
    .fetch_all(db)
    .await?;
    let known: std::collections::HashMap<String, (i64, Option<i64>)> = known
        .into_iter()
        .map(|(k, size, meta)| (k, (size, meta.get("mtime").and_then(|m| m.as_i64()))))
        .collect();

    let mut keys = Vec::with_capacity(files.len());
    let mut changed = 0;
    for f in &files {
        let key = format!("{prefix}{}", f.relative);
        keys.push(key.clone());
        if known.get(&key) == Some(&(f.size as i64, Some(f.mtime))) {
            continue;
        }
        let path = f.path.clone();
        let sha256 = tokio::task::spawn_blocking(move || sha256_of(&path))
            .await
            .map_err(|e| Error::Other(format!("lake index task failed: {e}")))?
            .map_err(|e| Error::Other(format!("could not read {}: {e}", f.path.display())))?;
        sqlx::query(
            "INSERT INTO lake_objects (id, kind, storage_key, provider, stream_name, size_bytes, sha256, metadata)
             VALUES ($1, 'applet', $2, 'applet', $3, $4, $5, $6)
             ON CONFLICT (storage_key) DO UPDATE SET
                 size_bytes = EXCLUDED.size_bytes, sha256 = EXCLUDED.sha256, metadata = EXCLUDED.metadata
             WHERE lake_objects.kind = 'applet'",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&key)
        .bind(applet_id)
        .bind(f.size as i64)
        .bind(&sha256)
        .bind(serde_json::json!({ "mtime": f.mtime }))
        .execute(db)
        .await?;
        changed += 1;
    }
    sqlx::query(
        "DELETE FROM lake_objects WHERE kind = 'applet' AND storage_key LIKE $1 || '%' AND storage_key <> ALL($2)",
    )
    .bind(&prefix)
    .bind(&keys)
    .execute(db)
    .await?;
    Ok(changed)
}

struct LakeFile {
    path: PathBuf,
    /// Relative to the applet's lake folder, with `/` separators.
    relative: String,
    size: u64,
    /// Seconds since the epoch.
    mtime: i64,
}

/// Every regular file under `dir`, not following symlinks, up to
/// [`MAX_INDEXED_FILES`].
fn list_files(dir: &Path) -> Vec<LakeFile> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for entry in entries.flatten() {
            let Ok(meta) = entry.path().symlink_metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else if meta.is_file() {
                if out.len() >= MAX_INDEXED_FILES {
                    tracing::warn!(dir = %dir.display(), "applet lake folder has more files than one index pass reads");
                    return out;
                }
                let Ok(relative) = entry.path().strip_prefix(dir).map(|p| p.to_string_lossy().replace('\\', "/")) else {
                    continue;
                };
                let mtime = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs() as i64);
                out.push(LakeFile { path: entry.path(), relative, size: meta.len(), mtime });
            }
        }
    }
    out
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_in_the_applet_folder_is_rewritten_to_where_the_unit_sees_it() {
        let folder = Path::new("/var/lib/virtues/applets/user/tides");
        assert_eq!(inside_path("/var/lib/virtues/applets/user/tides/scripts/fetch.py", Some(folder)), "/skill/scripts/fetch.py");
        assert_eq!(inside_path("python3", Some(folder)), "python3");
        assert_eq!(inside_path("/var/lib/virtues/applets/user/tidesx/a.py", Some(folder)), "/var/lib/virtues/applets/user/tidesx/a.py");
        assert_eq!(inside_path("--flag", None), "--flag");
    }

    #[test]
    fn the_env_has_no_server_secrets_and_points_storage_at_the_lake_folder() {
        let env = env("run_1", vec![("OPENAI_API_KEY".into(), "sk-test".into())]);
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        assert_eq!(get("STORAGE_PATH"), Some("/lake"));
        assert_eq!(get("OPENAI_API_KEY"), Some("sk-test"));
        assert!(get("VIRTUES_ENCRYPTION_KEY").is_none());
        assert!(get("VIRTUES_API_KEY").is_none());
        if let Some(url) = get("DATABASE_URL") {
            assert!(url.starts_with("postgres://virtues_applet_writer:"), "{url}");
            assert!(url.ends_with("?host=/run/postgresql"), "{url}");
        }
    }

    /// A host reaches a run only when the file asks for it and the owner
    /// allowed it.
    #[sqlx::test]
    async fn a_run_reaches_hosts_both_the_file_and_the_owner_name(pool: PgPool) {
        sqlx::query("INSERT INTO app_applets (id, name, owner, net_hosts) VALUES ('applet_tides', 'Tides', 'user', $1)")
            .bind(vec!["api.weather.gov".to_string(), "api.tides.example.com".to_string()])
            .execute(&pool)
            .await
            .unwrap();
        let config = serde_json::json!({ "net_hosts": ["api.weather.gov", "api.other.example.com"] });
        assert_eq!(allowed_hosts(&pool, "applet_tides", &config).await.unwrap(), ["api.weather.gov"]);
        let none = serde_json::json!({});
        assert!(allowed_hosts(&pool, "applet_tides", &none).await.unwrap().is_empty(), "the owner alone opens nothing");
    }

    /// Files the code wrote become rows; a changed file updates its row and
    /// a deleted one loses it. The limit counts what is indexed.
    #[sqlx::test]
    async fn the_lake_folder_is_indexed_after_a_run(pool: PgPool) {
        let id = format!("applet_lake_probe_{}", std::process::id());
        let (dir, prefix) = lake_folder(&id);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("photos")).unwrap();
        std::fs::write(dir.join("notes.txt"), "first").unwrap();
        std::fs::write(dir.join("photos/a.bin"), [0u8; 10]).unwrap();

        assert_eq!(index_lake_folder(&pool, &id).await.unwrap(), 2);
        assert_eq!(index_lake_folder(&pool, &id).await.unwrap(), 0, "unchanged files are not read again");

        std::fs::write(dir.join("notes.txt"), "second, longer").unwrap();
        std::fs::remove_file(dir.join("photos/a.bin")).unwrap();
        assert_eq!(index_lake_folder(&pool, &id).await.unwrap(), 1);

        let rows: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT storage_key, stream_name, size_bytes FROM lake_objects WHERE kind = 'applet' ORDER BY storage_key",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows, vec![(format!("{prefix}notes.txt"), id.clone(), 14)]);
        assert!(over_lake_limit(&pool, &id).await.unwrap().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
