# Plaid — from "never once connected" to a general-purpose finance collector · v1

Written 2026-07-24, on `feat/reliability-and-models`.

## Framing

Plaid is the **general** finance collector. FinanceKit is one Apple-shaped feed
among many: it needs an iPhone, it covers only what Apple covers, and plenty of
people and plenty of banks fall outside it. So Plaid is not a nice-to-have
alongside FinanceKit — it is the path that has to work for everyone else, and
collector overlap is the *normal* condition to design for, not an edge case to
suppress.

That reframing is what makes P3 below a real phase rather than a punt.

## Verified state (2026-07-24 — do not re-derive)

- **Plaid has never once connected.** Zero occurrences of "plaid" in Dragon's
  entire journal (Jul 8 → Jul 24, unrotated). Only `ios_ingest` and
  `credential_refresh` fan out. There is no Plaid credential on that box, and no
  Plaid applet has ever run.
- **The old flow had no return leg.** It launched the *self-hosted* webview Link
  URL with a top-level `redirect_uri`, and the callback only ever read
  `public_token` — no `oauth_state_id`, no `plaidlink://`, nothing. A comment
  pointed at a "resume leg in `callback`" that did not exist. Any OAuth bank
  (most large US banks) died on the return.
- **Rebuilt on this branch** as real Hosted Link: `hosted_link.
  {completion_redirect_uri,url_lifetime_seconds}`, no top-level `redirect_uri`,
  outcome polled from `/link/token/get`, session carried in `plaid_link_session`
  (migration 0007) keyed by a first-party `SameSite=Lax` cookie. Cancel is a
  first-class path. Institution name resolved and stored. 8 new tests.
  `cargo check --workspace` clean; 251 tests pass; migration + session-store SQL
  exercised against a real Postgres.
- **Nothing is deployed.** Staging still carries the broken version; Dragon runs
  `abeea8e` (2026-07-22), which predates even the 02b680b3 fixes.
- **Not verified against Plaid at all.** Every test runs on fixtures written
  from Plaid's docs. No live response has ever been seen.

## P0 — Make the first live attempt debuggable — ✅ DONE 2026-07-24

The one change worth making *before* any deploy.

`extract_public_token` returning `None` currently means "user cancelled" — and
it would also mean "Plaid's field names differ from the docs I read." Those two
must not be indistinguishable on the first live run, and the cancel path
currently logs nothing at all.

- When a session comes back but yields no public_token, log the session's actual
  key set (keys only — never values; these payloads carry tokens).
- Keep the user-facing outcome identical (`connect_cancelled`); this is purely
  an operator breadcrumb.
- Commit the branch.

## P1 — Deploy (~1 hr, mostly waiting)

Two surfaces. **Atlas is not involved** — it is a separate container built by
`make deploy-atlas`, and nothing in this work touches it.

1. ✅ **Plaid dashboard** — `https://auth.virtues.com/plaid/callback` registered
   2026-07-24. Plaid matches it exactly, which is why it carries no per-session
   query param.
2. ✅ **Prod env verified 2026-07-24** (SSM, `i-0a0b34b72dac1ac59` — the live
   host; the instance *named* "virtues", `i-04e515dab909e10ef`, runs nothing).
   Nothing to change: `PLAID_ENV` absent → production; `PLAID_REDIRECT_URI`
   absent → the registered default; client_id/secret present;
   `OAUTH_PROXY_EXCHANGE_SECRET` 48 chars; `VIRTUES_API_DATABASE_URL` present so
   migration 0007 runs at boot. **If you ever do edit that file, the container
   needs a recreate — `docker restart` will not re-read `--env-file`.**
3. **Registry: ECR, not GHCR.** The container runs
   `172349361546.dkr.ecr.us-east-1.amazonaws.com/virtues-api:latest`.
   `docker-build.yml` pushes to GHCR, which **nothing on that host pulls** —
   merging to staging deploys nothing. The real path is
   `make deploy-virtues-api` (builds linux/amd64, pushes ECR `:latest`) followed
   by a container recreate so the new `:latest` is actually pulled.
   *(Worth cleaning up later: CI builds an image no one consumes.)*
4. **virtues-api first, then Dragon.** New box + old proxy is still fully
   broken (Hosted Link lives in the proxy). New proxy + old box works on
   success and only 422s on cancel. Box ships by tag: force-push `edge`, then
   `sudo virtues upgrade --pre`.

## P2 — First real connect (~half a day, mostly watching logs)

Sandbox first (`PLAID_ENV=sandbox`), then production. Both legs matter:

- **A credential-only bank** (the path that could theoretically have worked
  before).
- **An OAuth bank** — Chase, Wells Fargo, Capital One. This is the leg that
  never existed, and the whole rebuild is for it.
- **A deliberate cancel**, to confirm it reads as cancelled rather than failed.

Then confirm, in order: a credential row exists and is named after the bank (not
"Plaid account"); accounts land with a real `institution_name`; the 30-minute
transactions cron runs and writes; `investments`/`liabilities` report success
with 0 records rather than going red.

**Stop here if the field names drifted.** P0's logging tells you within one
attempt, and the fix is local to `extract_public_token`.

## P3 — Multi-collector accounts (the real design work, ~1 wk)

The problem: Plaid and FinanceKit write the same two tables in disjoint
namespaces (`plaid:*` vs `apple_finance:*`). Connect Plaid to a bank Apple
already covers and every account and transaction exists twice. As more
collectors arrive, this gets worse, not better.

Solve it at the **account** layer. Then the transaction problem disappears,
because each real-world account has exactly one feed.

### P3.0 — Spike: is there a join key? (~2 hrs, blocking)

Plaid gives `mask`. FinanceKit does not columnize one, though it stores Apple's
full payload under `metadata.raw`.

Query real rows on Dragon: does the Apple payload carry a last-4 or any stable
discriminator? If yes, the key below works today. If no, P3.1 needs a different
discriminator (or account identity becomes user-confirmed rather than derived,
which is still covenant-legal — just more UI).

**This spike gates P3.1's shape. Do not design past it.**

### P3.1 — Deterministic account identity

Add an identity for the *real-world* account — normalized institution + mask +
type + currency — kept separate from `source_stream_id`, which stays the
per-collector id. Derived by rule, never inferred. Backfill existing rows.

### P3.2 — Exclusive ownership per account

When two collectors resolve to the same identity, one is authoritative; the
other's writes for that account are suppressed at write time.

- Deterministic + user-authored, so it stays inside the covenant.
- Default: **first collector to claim an account keeps it.** Not "newest wins",
  which silently churns data every time someone connects a source.
- The collision is surfaced in the UI, and the choice is flippable.

### P3.3 — Explicit non-goal: transaction matching

Do **not** fuzzy-match the same purchase across providers (equal amounts, dates
off by a day for pending-vs-posted, different merchant strings). That is exactly
the semantic ER the Deterministic Covenant killed. Account-level ownership makes
it unnecessary.

## Deferred, deliberately

- **`client_user_id` is the constant `"virtues-user"`** for every box in the
  fleet, so all users look like one user in Plaid's dashboard. Not functional;
  `/plaid/start` has no identity to work with (it is an unauthenticated browser
  GET). Fixing it means giving start a bearer identity.
- **`optional_products: [investments, liabilities]`** stays off until the Plaid
  account is enabled for them. Both applets no-op cleanly until then.
- **Browser-path error display**: the SPA reads neither `?connected=` nor
  `?error=` today. The native path shows a real page; adding browser toasts is
  new UI scope.
- **`arch-lint.sh` fails** on a pre-existing false positive in `pair.rs:697`,
  unrelated to this work.

## Risks

| Risk | Blast radius | Mitigation |
|---|---|---|
| `/link/token/get` field names differ from docs | Every connect reads as "cancelled" | P0 logging; one attempt to diagnose |
| Completion URI not registered / host mismatch | Every connect ends "expired" | P1.1 + P1.2, checked before rolling |
| Rolling the wrong image registry | Silent no-op; looks deployed | P1.3 |
| Plaid connected before P3 lands | Duplicate accounts + transactions in real data | Don't connect a bank FinanceKit already covers until P3.2 |
