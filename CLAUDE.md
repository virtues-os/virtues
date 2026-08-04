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

`main` only moves on release (it last moved for v0.3.0) and is routinely ~100
commits behind `staging`. Never branch from it.

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
- **Claim a migration number before writing the SQL:**

  ```sh
  make migration NAME=add_foo
  ```

  It takes the next number, writes a placeholder, and commits it under the lock
  — so the number is yours before anyone else looks. Two agents reaching for the
  same number is the *default* outcome otherwise, and git will not warn you:
  `sqlx::migrate!` keys on the version, and renumbering after a box has applied
  it breaks that box's upgrades. Migration 52 once killed a box for 3¼ hours.

  **The placeholder is `.sql.pending`, and you must rename it to `.sql` once the
  SQL is written.** `sqlx::migrate!` globs `*.sql`, so a bare placeholder is a
  *valid migration that does nothing* — and any box that boots in the window
  between claiming the number and writing the SQL records it as applied. The
  real SQL then never runs, and its checksum no longer matches what the DB
  stored, so the **next boot refuses to start**. That happened to the shared dev
  box on 2026-08-04. The `.pending` suffix reserves the number (the counter
  reads any filename starting with digits) while keeping it invisible to sqlx
  until you rename it.

  **The counter only sees your own branch.** A number claimed on an unmerged
  branch is invisible here — which is exactly why everyone works on `wave`. If
  you must merge a branch carrying migrations, check for duplicate numbers
  first:

  ```sh
  ls virtues-core/migrations | sed -n 's/^\([0-9]*\).*/\1/p' | sort | uniq -d
  ```
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

**Hotfixes to released code** branch from the release tag (e.g. `v0.3.0`), not
from `main` — the tag stays correct after `main` moves. Merge the fix to both
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
```

**One `make dev` serves every agent** — do not start a second one, and do not
kill the running one. `cargo check` will *block* on the shared target-dir lock
while `make dev` holds it. That is contention, not a hang; wait it out.

## Conventions

- Authored applets (chat/AI-created) are per-box runtime state and live in the
  **state root**, never in `applets/`: `/var/lib/virtues/applets` on a box,
  `.applet-state/` in a dev checkout. `applets/` is shipped, read-only package
  data that the installer replaces wholesale. Both state paths are gitignored
  and must not be committed.
- Model selection goes through the slot system and registry — no model-id
  literals in code.
