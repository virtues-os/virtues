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
//! server and the root repair that sets it (`db-logins` in
//! `cli/system_repairs.rs`) agree without storing it anywhere. The installer
//! and first boot run that repair before the server first starts, and every
//! upgrade runs it again. The connection goes over TCP to the same host,
//! because a box's own login uses peer auth on the socket and these two roles
//! have no OS user. Debian's default `pg_hba.conf` accepts a password on
//! `127.0.0.1`; where a cluster's does not, the repair adds a line that does.
//!
//! When the login does not connect, [`begin`] refuses on a box and returns an
//! error that says how to set the login. It does not drop the main pool into
//! the role instead, because the SQL inside could undo that. Dev
//! (`ENVIRONMENT=dev`) and this crate's tests still fall back to
//! `SET LOCAL ROLE` and log an error: there the dev server and test runs
//! share one cluster and may each set the passwords from a different secret.

use std::collections::{HashMap, HashSet};
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

/// The fix [`begin`] names when a login does not connect on a box.
const FIX: &str = "run `sudo virtues migrate` on the box to set the login, then try again";

/// Connect both logins at server start, and set a login that fails where this
/// process may.
///
/// Only a superuser or a CREATEROLE role may change another role's password.
/// The server's role is one in dev and CI, so there it sets a login that does
/// not connect. On a box it is neither, and Postgres would refuse the
/// `ALTER ROLE` and write the statement, password included, to its log. So on
/// a box this only connects, which also logs the fix for a login that fails.
pub async fn ensure_logins(pool: &PgPool) {
    let Some(secret) = secret() else {
        tracing::debug!("no secret to derive restricted Postgres logins from");
        return;
    };
    let may_alter = match sqlx::query_scalar::<_, bool>(
        "SELECT rolsuper OR rolcreaterole FROM pg_roles WHERE rolname = current_user",
    )
    .fetch_one(pool)
    .await
    {
        Ok(may) => may,
        Err(e) => {
            tracing::warn!(error = %e, "cannot read this role's privileges, so the restricted logins are left as they are");
            false
        }
    };
    let fallback = fallback_allowed();
    for login in Login::ALL {
        let mut connected = pool_for(pool, login, Some(&secret)).await;
        if connected.is_err() && may_alter {
            match sqlx::query(&alter_login_sql(&secret, login)).execute(pool).await {
                Ok(_) => {
                    forget_failure(pool, login);
                    connected = pool_for(pool, login, Some(&secret)).await;
                }
                Err(e) => tracing::warn!(role = login.role(), error = %e, "could not set the restricted login"),
            }
        }
        if let Err(why) = connected {
            report(login, &why, fallback);
        }
    }
}

/// One pool per (database, login). Keyed on the database so `#[sqlx::test]`'s
/// per-test databases each get their own.
type Key = (String, Login);

struct State {
    pools: HashMap<Key, PgPool>,
    /// When a login last failed to connect, and why. Retried after
    /// [`RETRY_AFTER`], so a box without the login does not pay a failed
    /// connect per query.
    failed: HashMap<Key, (Instant, String)>,
    /// Logins whose failure this process has already logged.
    reported: HashSet<Login>,
}

const RETRY_AFTER: Duration = Duration::from_secs(60);

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(State { pools: HashMap::new(), failed: HashMap::new(), reported: HashSet::new() }))
}

fn key(main: &PgPool, login: Login) -> Key {
    (main.connect_options().get_database().unwrap_or_default().to_string(), login)
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

/// The login's pool, or why it does not connect.
async fn pool_for(main: &PgPool, login: Login, secret: Option<&str>) -> Result<PgPool, String> {
    let key = key(main, login);
    {
        let st = state().lock().expect("restricted login state poisoned");
        if let Some(p) = st.pools.get(&key) {
            return Ok(p.clone());
        }
        if let Some((at, why)) = st.failed.get(&key) {
            if at.elapsed() < RETRY_AFTER {
                return Err(why.clone());
            }
        }
    }
    let Some(secret) = secret else {
        return Err("there is no VIRTUES_ENCRYPTION_KEY to derive its password from".into());
    };
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(600))
        .connect_with(options(&main.connect_options(), secret, login))
        .await;
    let mut st = state().lock().expect("restricted login state poisoned");
    match pool {
        Ok(pool) => {
            st.failed.remove(&key);
            st.pools.insert(key, pool.clone());
            Ok(pool)
        }
        Err(e) => {
            let why = e.to_string();
            st.failed.insert(key, (Instant::now(), why.clone()));
            Err(why)
        }
    }
}

/// Retry `login` on the next call, after its password was just set.
fn forget_failure(main: &PgPool, login: Login) {
    state().lock().expect("restricted login state poisoned").failed.remove(&key(main, login));
}

/// Close the login pools kept for `main`'s database. A test calls this at the
/// end, because `#[sqlx::test]` cannot drop a database these pools still use.
#[cfg(test)]
pub(crate) async fn close_pools(main: &PgPool) {
    let db = main.connect_options().get_database().unwrap_or_default().to_string();
    let pools: Vec<PgPool> = {
        let mut st = state().lock().expect("restricted login state poisoned");
        let keys: Vec<Key> = st.pools.keys().filter(|(d, _)| *d == db).cloned().collect();
        keys.iter().filter_map(|k| st.pools.remove(k)).collect()
    };
    for pool in pools {
        pool.close().await;
    }
}

/// Whether a login that does not connect may fall back to `SET LOCAL ROLE`
/// on the main pool. Only in dev and in this crate's tests (see the module
/// doc); a box refuses instead.
fn fallback_allowed() -> bool {
    cfg!(test) || crate::middleware::auth::is_dev()
}

/// What a box says when `login` does not connect.
fn refusal(login: Login, why: &str) -> String {
    format!(
        "The Postgres login {} does not connect ({why}), so this box does not run SQL that chat or an applet wrote. To fix it, {FIX}.",
        login.role()
    )
}

/// Log a login that does not connect, once per login per process.
fn report(login: Login, why: &str, fallback: bool) {
    if !state().lock().expect("restricted login state poisoned").reported.insert(login) {
        return;
    }
    if fallback {
        tracing::error!(
            role = login.role(),
            error = %why,
            "restricted Postgres login unavailable; falling back to SET ROLE, which a query can undo. \
             This happens only in dev and tests."
        );
    } else {
        tracing::error!(role = login.role(), "{}", refusal(login, why));
    }
}

/// Begin a transaction as `login`, on its own login. Run nothing that needs
/// the server's own tables in it; read those from `main` first.
///
/// On a box, a login that does not connect is an error that says how to fix
/// it. In dev and tests the transaction falls back to `main` after
/// `SET LOCAL ROLE`.
pub async fn begin(main: &PgPool, login: Login) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    begin_as(main, login, secret().as_deref(), fallback_allowed()).await
}

async fn begin_as(
    main: &PgPool,
    login: Login,
    secret: Option<&str>,
    fallback: bool,
) -> Result<Transaction<'static, Postgres>, sqlx::Error> {
    let why = match pool_for(main, login, secret).await {
        Ok(pool) => return pool.begin().await,
        Err(why) => why,
    };
    report(login, &why, fallback);
    if !fallback {
        return Err(sqlx::Error::Configuration(refusal(login, &why).into()));
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
        close_pools(&pool).await;
    }

    /// On a box, a login that does not connect is an error that names the
    /// fix, and the SQL never runs on the main pool. The main pool here
    /// points at a database that does not exist, so the login's connect fails
    /// whatever `pg_hba.conf` allows, and a fallback would fail differently.
    #[sqlx::test]
    async fn a_box_refuses_when_the_login_does_not_connect(pool: PgPool) {
        let missing = (*pool.connect_options()).clone().database("virtues_no_such_database");
        let main = PgPoolOptions::new().connect_lazy_with(missing);
        for login in Login::ALL {
            let err = begin_as(&main, login, Some("this-box-secret"), false)
                .await
                .expect_err("a box must not run this SQL without the login");
            let msg = err.to_string();
            assert!(msg.contains(login.role()), "{msg}");
            assert!(msg.contains(FIX), "{msg}");
        }
    }

    /// Tables that hold secrets or what authenticates the owner.
    const SECRET_TABLES: [&str; 5] = ["credentials", "app_pair_token", "app_setup_phrase", "app_publications", "app_device"];

    /// Neither login holds any privilege on a table that holds secrets, after
    /// the boot grant sweep and after the second sweep every later boot runs.
    ///
    /// Checked for every role the login can act as, not the login alone.
    /// `has_table_privilege` counts only inherited privileges, and since PG16
    /// a membership granted `WITH INHERIT FALSE` still lets the login
    /// `SET ROLE` to its role. `pg_has_role(.., 'MEMBER')` counts both.
    #[sqlx::test]
    async fn neither_login_has_a_privilege_on_secret_tables(pool: PgPool) {
        for _ in 0..2 {
            crate::server::faces::ensure_applet_db_grants(&pool).await.expect("grant sweep");
        }
        for login in Login::ALL {
            let roles: Vec<String> =
                sqlx::query_scalar("SELECT rolname::text FROM pg_roles WHERE pg_has_role($1::name, oid, 'MEMBER')")
                    .bind(login.role())
                    .fetch_all(&pool)
                    .await
                    .expect("roles the login can act as");
            assert!(roles.iter().any(|r| r == login.role()), "{} is missing", login.role());
            for role in &roles {
                for table in SECRET_TABLES {
                    let table = format!("public.{table}");
                    for privilege in ["SELECT", "INSERT", "UPDATE", "DELETE", "TRUNCATE", "REFERENCES", "TRIGGER"] {
                        let has: bool =
                            sqlx::query_scalar("SELECT has_table_privilege($1::name, $2::text, $3::text)")
                                .bind(role)
                                .bind(&table)
                                .bind(privilege)
                                .fetch_one(&pool)
                                .await
                                .expect("has_table_privilege");
                        assert!(!has, "{} can act as {role}, which has {privilege} on {table}", login.role());
                    }
                    let any_column: bool = sqlx::query_scalar(
                        "SELECT has_any_column_privilege($1::name, $2::text, 'SELECT, INSERT, UPDATE, REFERENCES')",
                    )
                    .bind(role)
                    .bind(&table)
                    .fetch_one(&pool)
                    .await
                    .expect("has_any_column_privilege");
                    assert!(!any_column, "{} can act as {role}, which has a column privilege on {table}", login.role());
                }
            }
        }
    }

    /// Resetting the role inside the SQL does not reach `credentials`: the
    /// session user is the login itself, so there is no higher role to return
    /// to, and `SET ROLE` to the server's role is refused. Run as a box runs
    /// it, with no fallback.
    #[sqlx::test]
    async fn neither_login_reaches_credentials_by_resetting_its_role(pool: PgPool) {
        crate::server::faces::ensure_applet_db_grants(&pool).await.expect("grant sweep");
        ensure_logins(&pool).await;
        let secret = secret();
        let read = "SELECT set_config('role', session_user, true) AS r, \
                    query_to_xml('select count(*) as n from public.credentials', true, false, '') AS x";
        let delete = "DO $$BEGIN PERFORM set_config('role', session_user, true); \
                      DELETE FROM public.credentials; END$$";
        let server_role: String = sqlx::query_scalar("SELECT quote_ident(current_user)")
            .fetch_one(&pool)
            .await
            .expect("the server's role");
        let set_role = format!("SET LOCAL ROLE {server_role}");
        for login in Login::ALL {
            for sql in [read, delete, set_role.as_str()] {
                let mut tx = begin_as(&pool, login, secret.as_deref(), false).await.expect("the login connects");
                let err = sqlx::query(sql).execute(&mut *tx).await.expect_err("the escape must be refused");
                assert!(err.to_string().contains("permission denied"), "{login:?}: {err}");
            }
        }
        close_pools(&pool).await;
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
