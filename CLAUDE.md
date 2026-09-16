# virtues

## Branching — read this before any git command

**Several agents work in this one checkout at the same time.** That is the whole
reason this section exists. A checkout has exactly one HEAD, one index, one
stash, and one working tree, and all of them are shared. A git command that is
harmless when one human runs it can destroy another agent's work here.

Worktrees are **not** used (one `make dev` serves everyone, and a stray worktree
has already been swept into someone's commit). So isolation is unavailable, and
discipline replaces it.

### The three branches

| Branch | Moves when | Who writes |
|---|---|---|
| **`wave`** | constantly | every agent — this is where you work |
| **`staging`** | when a slice of `wave` is green | merges only, via PR |
| **`main`** | when a release ships | merges only |

`wave` is permanent. It is **not** deleted after merging — it merges into
`staging` repeatedly through successive PRs, and work continues on it. That
keeps each PR a reviewable slice without anyone ever switching branches.

`main` only moves on release and is routinely far behind `staging`. Never
branch from it.

> **Stable releases exist — check, don't assume.** The version line was reset
> to 0.1.0 on 2026-08-18 (v0.1.0/0.2.0/0.3.0 were deleted), and the line has
> been shipping since: `v0.1.3` on 2026-08-24, `v0.1.4` on 2026-08-25, with
> `v0.1.5-staging.N` prereleases in flight. `virtues.com/sh` serves the stable
> channel; `sh-pre` serves the prerelease one.
>
> This note said "no stable release exists" for days after that stopped being
> true, and an agent copied the claim into a user-facing docs page. **Any
> statement here about what is released is stale by construction** — read it
> off the repo instead:
>
> ```sh
> gh release list --limit 5
> ```
>
> Beware `releases/latest`: this repo publishes several lines (box `vX.Y.Z`,
> `mac-vX.Y.Z`, `win-edge`) into one list, so "latest" is whichever GitHub
> flagged, not necessarily the box's.

Why `wave` exists rather than working on `staging` directly: `staging.N`
prereleases cut from `staging`, and real boxes install them with
`virtues upgrade --pre`. Half-finished work must not land there.

### Never — these destroy other agents' work

| Command | What it does to everyone else |
|---|---|
| `git switch` / `checkout <branch>` | moves the floor under every running agent |
| `git add -A` / `git add .` / `commit -a` | stages **everyone's** in-flight edits into your commit |
| `git stash` | pockets everyone's uncommitted work, repo-wide |
| `git restore` / `checkout -- <path>` | destroys another agent's edits, unrecoverably |
| `git reset --hard`, `git clean -fd` | same, wholesale |
| `git rebase`, `git push --force` | rewrites history others have built on |

These are hard rules, not guidance. If you believe you need one, **stop and ask
the human.** All of them have already caused a real loss in this repo.

To abandon your own work, do not stash — either leave the files alone or commit
them to `wave` and revert later. Commits are cheap; the stash is shared.

### Committing

Stage explicit paths, then commit with an explicit pathspec:

```sh
make commit MSG="fix(applets): the thing" FILES="path/one.rs path/two.rs"
```

`make commit` takes a lock and does the safe thing. If you commit by hand, the
pathspec after `--` is what makes it safe:

```sh
git add <your paths> && git commit -m "..." -- <your paths>
```

With a pathspec, `git commit` **ignores the index entirely** and commits only
those paths — so another agent's concurrently staged work cannot ride along.
Without it, whoever commits first takes everyone's staged changes. This has
already happened: a privacy-claim correction shipped inside a commit titled
"rename the HTTP surface."

Other rules that follow from a shared tree:

- **Check `git status` before editing.** If a file is already modified and you
  did not modify it, another agent is in it — pick something else or ask.
- **The commit message must describe everything in the commit**, not just the
  part you meant. With agents interleaving, `git log` is the only per-topic
  navigation anyone has.
- **Claim a migration number before writing the SQL** (`make migration
  NAME=add_foo`). The procedure, and the three traps that have each cost a box,
  are in `.claude/rules/migrations.md`, which loads when you open anything under
  `virtues-core/migrations/`.
- **Claim verification modestly.** A green `cargo check` on a shared tree may
  reflect another agent's half-finished edits.

### Merging up

From `wave`, when a slice is green: open a PR to `staging`. Never delete
`wave` afterward — keep committing to it.

**Batch slices; don't open a PR per change.** Pushing to `wave` is free — CI
only fires on pull requests and on `main`/`staging` pushes, so agents can commit
all day at no cost. Each PR, by contrast, costs a full Rust build plus a
Postgres service on a paid runner, and then costs it *again* when the merge
lands on `staging`. One PR at the end of a wave of work, not one per commit.
Docs-only changes are exempt (`paths-ignore` in `ci.yml`) — do not add
compiled paths to that list.

If `staging` moves independently (a hotfix, another machine), reconcile from
`wave` without switching:

```sh
git fetch origin && git merge origin/staging
```

**Hotfixes to released code** branch from the release tag (once one exists —
see above), not from `main` — the tag stays correct after `main` moves. Merge the fix to both
`main` and `staging`. This is the one case that leaves `wave`, and it needs the
human.

> **Naming:** `edge` is a release-channel identifier (a git *tag*, and an alias
> users type for the prerelease channel — see `cli/channel.rs`). Never name a
> branch `edge`; the tag/branch ambiguity breaks ref resolution.

## Builds

The virtues repos share one cargo target directory, set by `.cargo/config.toml`
at each repo root (untracked, listed in `.git/info/exclude`):

```
~/.cargo/shared-target
```

So **there is no `./target` in this repo** — build output lives at the path
above. It is shared across the virtues repos so that `target/` (~67GB when it
sprawls) is not duplicated per checkout. Cargo finds the config by walking up
from the working directory.

Do not move this setting to `~/.cargo/config.toml`. Other unrelated Rust
projects live on this machine, and `cargo clean` in any of them deletes the
whole shared target directory.

Checks:

```sh
cargo check --workspace     # Rust
cd apps/web && pnpm check   # Svelte
cargo test -p virtues --lib # the crate is `virtues`, not `virtues-core`
```

**If the suite fails a different random test on every run, it is not your
change.** `#[sqlx::test]` provisions a scratch database per test, and under
parallelism Postgres.app's app-permission gate rejects passwordless (`trust`)
connections from processes it does not recognise — `rejected "trust"
authentication`. A red suite then tells you nothing, which is exactly when a
real regression walks through.

Fixed on this machine by requiring a password for the app role over TCP, which
removes the condition the gate keys on rather than working around it. In
`~/Library/Application Support/Postgres/var-18/pg_hba.conf`, *above* the
general `trust` lines (first match wins):

```
host    all   virtues   127.0.0.1/32   scram-sha-256
host    all   virtues   ::1/128        scram-sha-256
```

then `ALTER ROLE virtues WITH PASSWORD 'virtues';` to match `.env`, and
`SELECT pg_reload_conf();`. Scoped to `virtues` deliberately: `adamjace` and
`postgres` are login roles with no password, so a blanket rule locks them out
of TCP.

**The `virtues` role is NOT a superuser** (since 2026-08-18). It has exactly
`LOGIN CREATEDB CREATEROLE`: CREATEDB for `#[sqlx::test]`'s scratch databases,
CREATEROLE so `server/faces.rs` can provision `virtues_face_reader`. It was
`SUPERUSER` with the password `virtues`, which — with the pg_hba rule above
opening loopback TCP — handed the cluster to any local process that guessed
once. `make db` downgrades an existing role idempotently.

Two consequences worth knowing before you debug a permissions error:

- `pgvector` is not a trusted extension, so `CREATE EXTENSION vector` needs
  superuser. `make db` installs it into `template1` instead, and every database
  created afterwards inherits it — which is what makes migration 0001's
  `CREATE EXTENSION IF NOT EXISTS` a no-op rather than a failure.
- In PG16+ a CREATEROLE role may only grant membership in roles it has ADMIN
  on. `make db` grants `virtues_face_reader`/`virtues_applet_writer` to
  `virtues` WITH ADMIN OPTION for this reason. Without it, faces.rs cannot grant
  them to itself and the symptom reads as an applet permissions bug.

**One `make dev` serves every agent** — do not start a second one, and do not
kill the running one. `cargo check` will *block* on the shared target-dir lock
while `make dev` holds it. That is contention, not a hang; wait it out.

## Where writing goes

Two roots, split by who reads them. Put a document in the wrong one and it
either rots unread or gets published to strangers.

| Path | For | Publishes |
|---|---|---|
| `docs/` | people **running** a box — the manual | yes, `virtues.com/docs` |
| `agents/build/` | whoever is **building** — contracts, vocabularies, style, our runbooks | no |
| `agents/record/` | what happened: audits, measured findings, design records of shipped work | yes, `virtues.com/docs/notes` |
| `agents/plan/` | designs for things being built | no |
| `agents/archive/` | superseded, kept for the reasoning | no |

Two questions place anything: **am I describing or prescribing?** and **will
this stop being true when we ship?** Describing + permanent is a record;
prescribing + permanent is build; prescribing + temporary is a plan. There is
no fourth genre — a description of something temporary is just a record of it.

The rules that keep it from rotting back into the 63-file pile this replaced:

- **Delete a plan when the thing ships.** What survives is a record and a
  manual page, never a plan describing an intention that is now a fact. Nothing
  ever left the old `docs/`, which is exactly why it grew unreadable.
- **Every doc is listed** in its directory's README; `tools/check-manual.py`
  enforces it, and for `agents/record/` an unlisted doc does not publish at all.
- **Write against the code, never against another doc.** On 2026-08-28 three
  audits found docs here wrong in ways that had already reached a user-facing
  page — a config path that does not exist on a real box, SQL against a dropped
  column, a relay privacy claim that was never true.
- **The manual claims only what ships.** It publishes from `main`, so a page
  cannot describe a command no released box has. Prose describes shape
  (`vX.Y.Z`); concrete versions belong in code fences. Enforced by the lint.

## Conventions

- Authored applets (chat/AI-created) are per-box runtime state and live in the
  **state root**, never in `applets/`: `/var/lib/virtues/applets` on a box,
  `.applet-state/` in a dev checkout. `applets/` is shipped, read-only package
  data that the installer replaces wholesale. Both state paths are gitignored
  and must not be committed.
- Model selection goes through the slot system and registry — no model-id
  literals in code.

### Never commit anything from a real life

No real names, phone numbers, addresses, emails, employers, account numbers, or
coordinates — in code, comments, docs, tests, seeds, or **commit messages**. Use
the reserved fictional block the repo already uses: `+1512555xxxx`,
`@example.com`, names like `Nick` and `David Okafor`.

This leaks twice: the repo is public, and code context reaches model providers
at runtime. On 2026-08-18 a third party's real mobile and first name were found
in a doc comment, a test constant, fixture data — and in a commit subject line,
where no file edit can reach them.

Write the FAILURE CLASS, not the incident. Every one of those comments was
valuable because it narrated a real bug, and every one survived
de-identification unchanged.

### Rules that load when they apply

Three bodies of law used to live here and now load only when you open a file
they govern (`.claude/rules/`, `paths:` frontmatter). They are not optional and
not lesser — they are conditional, and an always-on file is where a conditional
rule goes to be ignored:

| Rule | Loads when you touch | What it governs |
|---|---|---|
| `columns.md` | `*.sql`, Rust under `virtues-core/` | the seven-names-for-one-thing law, and the sweep a rename needs because `sqlx::query` is untyped |
| `query-errors.md` | any `*.rs` | why `.ok()` on a `fetch_*` turns a broken query into a plausible number |
| `migrations.md` | `virtues-core/migrations/**` | claiming a number, the pre-squash citations, and the `.sql.pending` rename that has to happen |

If you are editing one of those files, assume the rule is already in front of
you. If you are reasoning about them from elsewhere, go read it.
