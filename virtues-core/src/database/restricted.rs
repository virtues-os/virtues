//! Separate Postgres logins for SQL the server did not write.
//!
//! Faces, the assistant's `sql_query` and `sql_write` run SQL that a model or
//! an applet wrote. `SET LOCAL ROLE` to a low-privilege role is not a boundary
//! for that SQL: Postgres lets a session return to any role its *login* is a
//! member of, so `SELECT set_config('role', session_user, true)` inside the
//! query undoes the drop and the rest of the query reads as the server.
//!
//! So that SQL logs in as the low-privilege role itself. `session_user` is
//! then `virtues_face_reader` or `virtues_applet_writer`, and there is no
//! higher role to return to.
//!
//! Each login's password is derived from a box secret ([`password`]), so the
//! server and the root repair that sets it (`cli/system_repairs.rs`) agree
//! without storing it anywhere. The connection goes over TCP to the same
//! host, because a box's own login uses peer auth on the socket and these two
//! roles have no OS user. Debian's default `pg_hba.conf` accepts a password
//! on `127.0.0.1`.
//!
//! When the login is not available (a box that has not upgraded through the
//! repair, a `pg_hba.conf` that refuses it), [`begin`] falls back to
//! `SET LOCAL ROLE` on the main pool and logs an error. That is the old
//! behavior, so a box without the login is no worse than before and keeps
//! working.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Postgres, Transaction};

/// Which low-privilege login a query runs as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Login {
    /// SELECT on `data_*`, `wiki_*` and applet schemas (faces, `sql_query`).
    Reader,
    /// DML inside `applet_*` schemas (`sql_write`).
    Writer,
}

impl Login {
    pub const ALL: [Login; 2] = [Login::Reader, Login::Writer];

    pub fn role(self) -> &'static str {
        match self {
            Login::Reader => "virtues_face_reader",
            Login::Writer => "virtues_applet_writer",
        }
    }
}

/// The login's password: HMAC-SHA256 of the role name under `secret`, as hex.
pub fn password(secret: &str, login: Login) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes a key of any length");
    mac.update(b"virtues-db-login:");
    mac.update(login.role().as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

/// The secret this process derives passwords from: the box's encryption key,
/// or, where there is none (dev, CI), the password in `DATABASE_URL`.
fn secret() -> Option<String> {
    if let Ok(key) = std::env::var("VIRTUES_ENCRYPTION_KEY") {
        if !key.is_empty() {
            return Some(key);
        }
    }
    // PgConnectOptions has no password getter, so read it from the URL.
    let url = url::Url::parse(&std::env::var("DATABASE_URL").ok()?).ok()?;
    let password = urlencoding::decode(url.password()?).ok()?.into_owned();
    (!password.is_empty()).then_some(password)
}

/// Where a sandboxed run finds the Postgres socket. The box's own socket
/// directory; the sandbox leaves it reachable for applet code.
pub const SOCKET_DIR: &str = "/run/postgresql";

/// A URL for `login` over the Postgres socket, for a process with no network
/// (a sandboxed applet run). The box's `pg_hba.conf` takes a password for
/// these logins on the socket; the `db-logins` repair adds that line.
pub fn socket_url(login: Login) -> Option<String> {
    let secret = secret()?;
    let url = url::Url::parse(&std::env::var("DATABASE_URL").ok()?).ok()?;
    let database = url.path().trim_start_matches('/');
    let database = if database.is_empty() { "virtues" } else { database };
    Some(format!(
        "postgres://{}:{}@localhost/{database}?host={SOCKET_DIR}",
        login.role(),
        password(&secret, login)
    ))
}

/// SQL that makes `login` a login role with its derived password. Run by
/// whoever may alter the role: the server in dev and CI (it has CREATEROLE),
/// `postgres` in the box's root repair. The password is hex, so quoting it is
/// safe.
pub fn alter_login_sql(secret: &str, login: Login) -> String {
    format!("ALTER ROLE {} LOGIN PASSWORD '{}'", login.role(), password(secret, login))
}

/// Make both logins usable, where this process is allowed to. On a box the
/// server has no CREATEROLE, so this is refused and the root repair does it
/// instead; that refusal is expected and only logged at debug.
pub async fn ensure_logins(pool: &PgPool) {
    let Some(secret) = secret() else {
        tracing::debug!("no secret to derive restricted Postgres logins from");
        return;
    };
    for login in Login::ALL {
        if let Err(e) = sqlx::query(&alter_login_sql(&secret, login)).execute(pool).await {
            tracing::debug!(role = login.role(), error = %e, "cannot set the restricted login here");
        }
    }
}

/// One pool per (database, login). Keyed on the database so `#[sqlx::test]`'s
/// per-test databases each get their own.
type Key = (String, Login);

struct State {
    pools: HashMap<Key, PgPool>,
    /// When a login last failed to connect. Retried after [`RETRY_AFTER`],
    /// so a box without the login does not pay a failed connect per query.
    failed: HashMap<Key, Instant>,
    warned: bool,
}

const RETRY_AFTER: Duration = Duration::from_secs(60);

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State { pools: HashMap::new(), failed: HashMap::new(), warned: false }))
}

fn options(main: &PgConnectOptions, secret: &str, login: Login) -> PgConnectOptions {
    // A socket path means peer auth, which these roles cannot use: go over
    // TCP to the same machine instead.
    let host = if main.get_socket().is_some() || main.get_host().starts_with('/') {
        "127.0.0.1"
    } else {
        main.get_host()
    };
    let mut opts = PgConnectOptions::new_without_pgpass()
        .host(host)
        .port(main.get_port())
        .username(login.role())
        .password(&password(secret, login))
        .ssl_mode(main.get_ssl_mode())
        .application_name(&format!("virtues-{}", login.role()));
    if let Some(db) = main.get_database() {
        opts = opts.database(db);
    }
    opts
}

/// The login's pool, if it can connect. `None` means use the fallback.
async fn pool_for(main: &PgPool, login: Login) -> Option<PgPool> {
    let main_opts = main.connect_options();
    let key: Key = (main_opts.get_database().unwrap_or_default().to_string(), login);
    {
        let st = state().lock().expect("restricted login state poisoned");
        if let Some(p) = st.pools.get(&key) {
            return Some(p.clone());
        }
        if st.failed.get(&key).is_some_and(|t| t.elapsed() < RETRY_AFTER) {
            return None;
        }
    }
    let secret = secret()?;
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(600))
        .connect_with(options(&main_opts, &secret, login))
        .await;
    let mut st = state().lock().expect("restricted login state poisoned");
    match pool {
        Ok(pool) => {
            st.failed.remove(&key);
            st.pools.insert(key, pool.clone());
            Some(pool)
        }
        Err(e) => {
            if !st.warned {
                st.warned = true;
                tracing::error!(
                    role = login.role(),
                    error = %e,
                    "restricted Postgres login unavailable; falling back to SET ROLE, which a query \
                     can undo. Upgrading the box sets the login (system repair `db-logins`)."
                );
            }
            st.failed.insert(key, Instant::now());
            None
        }
    }
}

/// Begin a transaction as `login`: on its own login when it can connect,
/// otherwise on `main` after `SET LOCAL ROLE`. Run nothing that needs the
/// server's own tables in it; read those from `main` first.
pub async fn begin(main: &PgPool, login: Login) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    if let Some(pool) = pool_for(main, login).await {
        return pool.begin().await;
    }
    let mut tx = main.begin().await?;
    sqlx::query(&format!("SET LOCAL ROLE {}", login.role())).execute(&mut *tx).await?;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each transaction is logged in as its role, not dropped into it, so
    /// there is no higher role for the SQL to return to.
    #[sqlx::test]
    async fn each_login_is_its_own_session_user(pool: PgPool) {
        ensure_logins(&pool).await;
        for login in Login::ALL {
            let mut tx = begin(&pool, login).await.expect("begin");
            let who: String = sqlx::query_scalar("SELECT session_user::text")
                .fetch_one(&mut *tx)
                .await
                .expect("session_user");
            assert_eq!(who, login.role(), "{login:?} fell back to SET ROLE");
        }
    }

    #[test]
    fn passwords_differ_per_login_and_per_secret() {
        let a = password("box-one", Login::Reader);
        assert_eq!(a.len(), 64);
        assert_ne!(a, password("box-one", Login::Writer));
        assert_ne!(a, password("box-two", Login::Reader));
        assert_eq!(a, password("box-one", Login::Reader));
    }
}
