# Sources as packages

**Shipped 2026-08-04 (P0–P4).** A source stopped being a row in one
compiled-in TOML file and became a *package*: a directory carrying its own
`[[source]]` declaration plus the applets that serve it, discoverable in the
shipped root or the state root. Git is one way to deliver a package, not the
model. What is still open (P5, `oauth_direct`) is
[`../plan/sources-packages-plan.md`](../plan/sources-packages-plan.md).

## Why a package is a git repo

Installing a third party's repo was made a first-class action, not an escape
hatch, for two reasons:

1. **Uniform updatability.** There were four update mechanisms (box release,
   Apple/Sparkle, reconcile-from-disk, nothing at all for imports). A package
   at a pinned ref makes "update" one verb.
2. **It forces the git path to be finished.** An optional path rots; making it
   load-bearing makes the tests and the jail ship-blocking.

Two audiences, one system: the trust tier is a visible property of a package,
and the dangerous path is not reachable by accident. The Catalog stays a
curated shelf with no paste-a-URL box; stranger-repo install is sudo-gated.

## What the 2026-08-04 sweep found

Each finding became a phase. All are fixed; the failure classes are the part
worth keeping.

- **The git importer worked zero percent.** It queried a dropped column
  (`app_applets.dir`) before cloning. `sqlx::query_as` is unchecked, so it
  compiled and failed at runtime on every import, and three pure-function unit
  tests with no integration test hid it.
- **An applet `command` was an unjailed RCE path.** Spawned with no
  `env_clear`, the child inherited `VIRTUES_ENCRYPTION_KEY` and the unscoped
  `DATABASE_URL`; bare names resolved through `PATH`, so `python3` worked; the
  authoring docs taught the interpreter form; and import sat behind the same
  blanket auth as every route while changing a BYO key was sudo-gated. A
  one-line manifest and a `.py` file ran as `virtues` with the vault key in its
  environment.
- **The catalog was closer to package-ready than expected** — already read from
  disk — but shipped-root only, replace-not-merge, no provenance, no duplicate
  check.
- **One missing source aborted reconcile box-wide**, mid-pass, after the orphan
  GC had already deleted rows. Credentials were never reconciled against the
  catalog, so removing a source left un-revokable rows.

## What shipped

| phase | what | commits |
|---|---|---|
| P0 | `env_clear` + passthrough allowlist on applet spawn | `fbd98e1e` |
| P1 | view source everywhere; fork-on-edit into the state root with `forked_from`; first-party `repo`/`repo_ref` pointers on `ios`/`mac` | `df95e4ce`, `9e0e4899`, `ac6841b3` |
| P2 | catalog merges `[[source]]` from every package, last-wins-by-id; provenance on `Source`; source dedup; unknown source is per-template, not a global abort; catalog-less credentials surfaced | `e4a70cb5` |
| P3 | importer fixed with its integration test; provenance columns (`repo_url`, `git_ref`, `commit`, `imported_at`, `forked_from`); slug includes host and owner; URL policy (no `git://`/`http://`, private-range deny, timeout); pin by commit | `340a55bc`, `d04daa7e` |
| P4 | argv policy by provenance; sudo-gated import with an on-screen trust warning; imported `command` packages run in the `systemd-run` jail rather than being banned | `48cee3f2` |

## Decisions that stand

- **Shipped applets are packages pinned to the release version.** Built-ins have
  no independent update verb; they move when the box moves.
- **Fork provenance now, fork UI later.** `forked_from = <url>@<sha>` is
  recorded; diff/rebase is deferred.
- **Build the jail; don't ban the capability.** Native third-party code stays
  supported; it stops being root. The pattern is `code_interpreter`'s
  (`PrivateNetwork`/`MemoryMax`, refuses to run unsandboxed in release).
- **No package-carried global migrations.** Package DDL stays in its private
  `applet_<slug>` schema and never enters the `sqlx::migrate!` lineage; one
  shared counter under a lock already cost a box hours.
- **No registry.** Git URLs; sharing is v2.

## Adjacent items from the same sweep

- The OAuth `exchange_token` was signed but not encrypted — plaintext tokens in
  a browser query parameter. Fixed in `4f7b20d2` (AES-256-GCM sealed, then
  HMAC'd).
- `sql_query` ran on the main pool. It now drops to `virtues_face_reader` with
  `SET LOCAL ROLE` (`tools/sql_query.rs`), default-deny, SELECT on
  `data_*`/`wiki_*` only.
- The face `MAX_ROWS` cap is an outer `LIMIT` wrapper around caller SQL
  (`server/faces.rs`). The read-only transaction and role grants are what hold;
  the wrapper is not a control.
