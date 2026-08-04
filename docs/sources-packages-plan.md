# Sources as packages

**Status:** proposed, 2026-08-04. Supersedes nothing; extends
`docs/applets-overhaul-plan.md` (§ Git / distribution).

The idea: a source stops being a row in one compiled-in TOML file and becomes a
*package* — a directory carrying its own `[[source]]` declaration plus the
applets that serve it. Git becomes one way to deliver a package, not the model
itself. First-party sources (iOS, Mac, Android) additionally carry a pointer to
the public repo that builds them.

Motivation: adding a provider today means editing `applets/sources.toml`,
cutting a box release, *and* — for OAuth — a code change and deploy of
virtues-api. That is an unbounded centralized bottleneck on an appliance whose
entire pitch is that the owner owns everything.

**Decided:** a package *is* a git repo, and installing a third party's repo is a
first-class supported action — not a power-user escape hatch. Two reasons, and
the second is the stronger one:

1. **Uniform updatability.** There are four update mechanisms today — box
   release for shipped applets, Apple/Sparkle for collectors, reconcile-from-disk
   for chat-authored, and nothing at all for imports. If every package is a repo
   at a pinned ref, "update" becomes one verb with one UI regardless of where the
   package came from.
2. **It forces the git path to be finished.** The importer is half-built
   *because nothing depends on it*: a dead column query, no provenance, no
   persisted SHA, no integration tests. Optional paths rot. Making it
   load-bearing is the forcing function that gets it done properly.

The cost of that bet, stated plainly: once git is load-bearing, a broken git
path is a broken product rather than a broken optional feature. The tests and
the sandbox stop being "should" and become ship-blocking.

Two audiences, one system: the person who forks and reads diffs, and the person
for whom "Apple is kinda hard." Those are not in tension if the trust tier is a
visible property of the package and the dangerous path is not reachable by
accident — which is an IA problem, not a capability problem. Catalog stays a
curated shelf with no paste-a-URL box; stranger-repo install lives one door
over, sudo-gated, exactly like `change_byo_key`.

---

## What the sweep found

Five things changed the shape of this plan. They are load-bearing; read them
before the phases.

### 1. The git importer does not work at all

`applet_git_import.rs:293-299` queries `app_applets.dir`. That column was
dropped by migration `0051_applets_drop_runtime.sql:15`. `sqlx::query_as` is
unchecked, so it compiles and fails at runtime with `column "dir" does not
exist`. The call site is `ids_under_slug`, invoked at `applet_git_import.rs:77`
**before** the clone — so nothing is ever cloned and every import returns 400.

There are three pure-function unit tests and zero integration coverage, which is
exactly why this shipped undetected. **"The git model already half-works" was
wrong. It works zero percent.**

### 2. There is no sandbox, and `command` is not restricted to our binaries

The manifest's `command` is argv, spawned with no `current_dir`, **no
`env_clear`**, no uid change, no rlimit, no namespace
(`applet_runner/mod.rs:633-640`). Consequences, all verified:

- The child inherits the server's whole environment — including
  `VIRTUES_ENCRYPTION_KEY` (the master key for every credential in the vault)
  and `DATABASE_URL` (the unscoped pool). `load_credentials`' doc comment claims
  "the master encryption key never crosses the subprocess boundary"; that is
  true of the explicit stdin payload and false in practice.
- `resolve_program` (`:724-792`) passes a bare name through to the OS PATH
  loader when it matches no workspace binary. `python3`, `node`, `bash` all
  resolve. There is no allowlist.
- `applets/AUTHORING.md:94` and `MANIFEST_SCHEMA.json:43` **actively teach** the
  interpreter form: `command = ["python3", "applets/my_action/main.py"]`.
- The systemd unit deliberately disables hardening
  (`install.rs:1286-1305`, `NoNewPrivileges=false`), and the installer writes
  `virtues ALL=(ALL) NOPASSWD: ALL` to `/etc/sudoers.d/virtues`
  (`install.rs:784-789`).

So a git-delivered package containing one line of TOML and one `.py` file gets
arbitrary code execution as `virtues`, which is one `sudo -n` from root, with
the vault key in its environment. No compiler required. The 21 shipped applets
all happen to be `command = ["<bare_name>"]` compiled bins, which makes the tree
*look* constrained; nothing enforces it.

The honest framing: **`import-git` is not currently a feature with a security
gap. It is a remote-code-execution endpoint with a UI.** It is gated only by the
same blanket `AuthUser` as every other route — `/api/admin/` is naming, not
middleware — while changing a BYO API key *is* sudo-gated. That asymmetry is the
clearest thing to fix.

### 3. The catalog is closer to package-ready than expected

`load_catalog()` already reads `sources.toml` from disk
(`applet_templates/mod.rs:389-397`), falling back to the `include_str!` bake
only if the file is unreadable. But: shipped root only, **replace not merge**,
no per-source provenance, and no duplicate-id check (actions get one, sources
don't). The state root — where imports and chat-authored applets live —
contributes zero sources.

The merge shape to copy already exists ten lines below, in the action overlay
(`mod.rs:425-432`).

### 4. A missing source aborts reconcile box-wide

`mod.rs:699-704` — a template referencing an unknown source id is a hard `Err`
that returns from `reconcile_templates` **mid-pass**, after the orphan GC has
already deleted rows and before the system GC runs. Every remaining template is
skipped. That error path is shared by boot, the admin reconcile, the OAuth
callback, and pair-consume.

So one package that removes a source but leaves a manifest behind bricks
reconcile for the whole box. This must become per-template before anything
outside our release can contribute sources.

Related: credentials are **never** reconciled against the catalog. Remove a
source and its credential rows survive with a `source_id` that resolves to
nothing — invisible in the UI, un-revokable, `auth_type` `"unknown"`, and their
fan-out applets keep running.

### 5. `oauth_direct` is small, and the hard part isn't code

The proxy is bolted on at exactly two seams — `proxy_exchange` and
`proxy_refresh` — each `(source_id, token) → one normalized struct`. Everything
else (AES-256-GCM vault, state HMAC, `expires_at`/`next_refresh_at`, the
JIT-refresh mutex, the `credential_refresh` cron, the applet secret contract) is
already provider- and proxy-agnostic. `secrets` is `serde_json::Value` end to
end, so a `{client_id, client_secret, access_token, refresh_token}` blob stores
with **zero migration**, and `settings_byo.rs` is a working precedent for
"user's own credential as a synthetic credentials row."

Two useful corrections to my earlier framing:

- **The proxy is a client-secret custodian, not a token custodian.** The box
  already holds raw provider refresh tokens at rest. Going direct does not
  change the box's exposure to tokens; it only adds a client_secret.
- `oauth2 = "4.4"` is already in `virtues-core/Cargo.toml:110` and **referenced
  by nothing** — a dead dependency whose PKCE support is already paid for.

The real obstacle is `redirect_uri`. The box lives at `.local` / `127.0.0.1`,
and Google will not accept a `.local` redirect. Loopback redirects are accepted
for "desktop app" client types — which also makes the client_secret concern
mostly moot, since those secrets are non-confidential by design. That is a
product question about registration friction, not an engineering one.

One coupling to check before assuming BYO can bypass the proxy freely:
`docs/networking-relay-tee.md:209` reuses the OAuth proxy as a Sybil-resistance
chokepoint for relay payment gating.

---

## What a package is

A directory, discoverable in either root, containing:

```
<slug>/
  sources.toml         # zero or more [[source]] rows        (new: now scanned)
  <applet>/manifest.toml
  <applet>/face/       # optional
```

The scanner rule is unchanged: a dir with `manifest.toml` is an applet, a dir
without one is a namespace descended one level. Ids stay
`applet_<dir with / → __>`, which is what keeps packages from colliding.

Delivery is orthogonal: shipped in the release, cloned by git, written by chat,
or copied in by hand. `state_root` wins over `shipped_root`, so a package can
shadow a built-in and deleting it reverts cleanly — that precedence already
exists and is documented as a feature.

---

## Phases

Ordered so that each phase is independently shippable and no phase opens a door
before the lock for it exists.

### P0 — `env_clear` on applet spawn *(do this regardless)*

Add `.env_clear()` plus an explicit passthrough allowlist at
`applet_runner/mod.rs:633`. Applets receive their credentials on stdin by
design; there is no reason for `VIRTUES_ENCRYPTION_KEY` or the unscoped
`DATABASE_URL` to be in their environment.

Costs nothing, needs no design, closes the gap between `load_credentials`' doc
comment and its behavior, and is worth doing whether or not any of the rest
happens. **Independent of this plan; ship it first.**

### P1 — Read and fork *(cheap, and the feature that makes "open" felt)*

Three things that are almost free and serve both audiences at once. None needs a
git remote.

- **View source, everywhere.** Every applet — shipped, source-created,
  AI-authored — gets a read-only source surface. The technical reader uses it;
  the non-technical one is reassured it exists without opening it. Zero risk,
  and it is the strongest trust artifact available for the price.
- **Fork on edit.** Editing a shipped applet writes a copy into the state root,
  which already shadows shipped and already reverts cleanly on delete. Record
  `forked_from`. This is what "fork, change, run it" means for the 90% case —
  our code, their change — and it is safe by construction in a way that
  installing a stranger's repo is not. Ranked above stranger-import for that
  reason, not instead of it.
- **First-party repo pointers.** An optional `repo` / `repo_ref` on `Source`,
  populated for `ios` and `mac`, surfaced on the Catalog row as "read the code."
  Provenance, not an update mechanism — those collectors ship through the App
  Store, a notarized DMG, and the Tauri updater, and no git ref can install
  them. For an appliance whose pitch is verifiability, "here is the exact source
  for the collector you just paired" is worth more than most features.

### P2 — Merge the catalog *(the keystone)*

1. `load_catalog()` merges `[[source]]` rows from shipped root, then each
   package dir, last-wins-by-id — mirroring the action overlay at `mod.rs:425`.
2. Add `dir` / provenance to `Source` so a row is attributable to a package.
3. Extend the dedup pass to sources; today uniqueness is asserted only in a test.
4. Make the unknown-source error **per-template**, not a global abort: skip that
   template, log it loudly, keep reconciling.
5. Reconcile credentials against the catalog — at minimum, surface
   catalog-less credentials in the UI so they can be revoked.
6. Add `shipped_source_count` if any source-driven deletion is introduced; the
   existing `shipped_count` guard counts actions only and gives sources no
   protection.

After P2 a new provider is a directory, no release required — which is most of
the value, and it needs no git at all.

### P3 — Fix and instrument the importer

1. Fix `ids_under_slug` to key on the `applet_<slug>` id prefix instead of the
   dropped `dir` column. Add the integration test whose absence hid this.
2. Add the provenance columns `docs/applets-overhaul-plan.md:137` already names:
   `repo_url`, `git_ref`, `commit`, `imported_at`, `forked_from`. Persist the
   resolved SHA — today it is computed, returned in JSON, and dropped by the TS
   client.
3. Slug must include host and owner. Today `github.com/alice/tools` and
   `evil.com/mallory/tools` both become `tools`, and a re-import silently
   re-fetches the *original* remote while reporting success.
4. URL policy: drop `git://` and `http://`, add a deny-list for link-local and
   private ranges, follow-redirect checks, and a subprocess timeout. There is
   none of this today.
5. Pin by commit. `git clone --branch <sha>` does not work, so first-import
   pinning needs `init` + `fetch` + `checkout`.

### P4 — Execution policy *(the gate)*

Nothing user-facing should install a package until this exists. Decide, in
order of increasing effort:

- **argv policy by provenance.** Shipped packages may name workspace binaries;
  imported packages may not spawn arbitrary interpreters. This is a reconcile
  time check and it is cheap.
- **Sudo-gate import**, matching `change_byo_key`. A trust warning in
  `GitImportModal` — today the caveat exists only as a source comment and never
  reaches the screen.
- **Build the jail; don't ban the capability.** Face-only and agent-only
  packages are already genuinely bounded (opaque-origin iframe + read-only PG
  role; a 12-tool allowlist). A `command` package is not bounded at all — but
  the answer is the `systemd-run` jail (`docs/applets-overhaul-plan.md:129`),
  not a prohibition. `code_interpreter` (`api/code.rs:87-90`) already runs
  untrusted code under `PrivateNetwork`/`MemoryMax` and refuses to run
  unsandboxed in release builds; that is the pattern, and applying it to
  imported `command` packages moves the guarantee from policy back to
  structural, which is what the product's doctrine wants. Native third-party
  code stays supported; it just stops being root.

### P5 — `oauth_direct` spike

One provider, self-serve registration, loopback redirect. Prove the variant end
to end, then decide whether it generalizes. Prerequisites: a
`SourceAuth::OauthDirect` variant, `direct_{start,exchange,refresh}` in
`virtues-helpers/auth`, and refactoring the `msg.contains("upstream 4")`
reauth detection (`refresh.rs:131`) into a typed error — two call sites
currently depend on a substring of an error message.

Independently worth doing: a capability endpoint on virtues-api listing
supported providers. Today the box and the proxy are coupled by convention and
drift silently into a 404.

---

## Explicitly not doing

- **Package-carried global migrations.** Applets own private `applet_<slug>`
  schemas; package DDL stays inside them and never enters the `sqlx::migrate!`
  lineage. Migration 52 killed a box for 3¼ hours when *one* team shared a
  counter under a lock; N third-party packages sharing one is strictly worse,
  and git gives no coordination.
- **Treating `MANIFEST_SCHEMA.json` as a stable public API yet.** It is already
  public and tracked — that was never the issue. The cost is that breaking it
  starts costing other people, and we should not accept that until P2–P4 have
  settled the shape.
- **A registry.** `docs/applets-overhaul-plan.md:139` is right: git URLs, no
  registry, sharing is v2.
- **Self-service OAuth for the hosted proxy.** A package can declare a
  `via_proxy` source, but the proxy must have a route and a registered app; that
  stays gated until P5 removes the dependency.

---

## Resolved

**Relay Sybil resistance does not constrain P5.** `networking-relay-tee.md:209`
reuses "the OAuth-proxy / virtues-api / wallet credential spine" — the anchor is
the *billing relationship* (account → wallet → api_key), not the OAuth token
exchange. `oauth_direct` moves where a provider's client credentials live; it
does not decommission virtues-api, which remains the AI gateway, wallet, and
relay control plane. A user who brings their own Google app still has a Virtues
account.

**Shipped applets are packages, pinned to the release version.** Uniform model,
atomic release semantics preserved, no per-built-in repo. Consequence: built-ins
have no independent update verb — they move when the box moves.

**Fork provenance now, fork UI later.** Recording `forked_from = <url>@<sha>` is
foundation (a column, and the thing that makes a later answer possible at all).
The diff/rebase experience is not, and is explicitly deferred.

## Open questions
1. Which providers accept a loopback `redirect_uri` under self-serve
   registration? Determines whether P5 generalizes or stays a one-provider
   escape hatch. Genuine research; everything else about P5 is small.

## Update policy

Because shipped applets follow the box, an auto-update toggle only ever applies
to third-party packages — one rule, not two:

- **Built-ins:** no toggle. They move when the box moves, under the release
  channel's tested migration lineage and rollback.
- **Third-party:** default **notify, don't apply**. Per-package opt-in to follow
  a ref.
- **Auto-follow is available only to face-only and agent-only packages, or to
  jailed `command` packages.** Silently pulling and running unjailed native code
  against the whole lake is precisely what P4 exists to prevent, and it must not
  be the default anywhere.

Note "update available" is impossible until P3 persists the resolved SHA — today
the box cannot answer the question at all.

## P5 shape: one seam, two implementations

The elegant version is *not* a parallel OAuth flow. The proxy attaches at
exactly two seams — `proxy_exchange` and `proxy_refresh`, each
`(source_id, token) → one normalized token set` — so `oauth_direct` is a second
implementation behind a seam that already exists. Vault, expiry math, the JIT
refresh mutex, the cron sweep, and the applet secret contract are untouched.
One catalog variant, two functions, one match.

**The over-engineering trap is provider quirks.** The proxy special-cases
per-provider behaviour in four places (authorize params — Google's
`access_type=offline`, Strava's comma-separated scopes; token-request shape —
Notion's HTTP Basic; response and refresh normalization). A fully generic
`oauth_direct` would have to express all of that in TOML, which means inventing
a configuration language for OAuth dialects.

Don't. Split by capability instead:

- **Proxy** — curated providers, quirks live in code where they belong, zero
  user setup.
- **Direct** — standards-compliant RFC-6749 + PKCE only. `authorize_url`,
  `token_url`, `scopes`, and `auth_style` (basic vs body) as the single
  concession to reality. A provider needing more than that belongs in the proxy.

Client credentials go in the same encrypted `secrets` blob as the tokens — it is
already `serde_json::Value`, so one credential row carries everything one
connection needs with zero migration. Use `oauth2 = "4.4"` for PKCE rather than
hand-rolling; it is already a (currently dead) dependency. Redirect to
`http://127.0.0.1:<port>/oauth/callback` under a desktop-app client type, which
also makes the client_secret non-confidential by design and dissolves the
"the box now holds a secret" worry.
