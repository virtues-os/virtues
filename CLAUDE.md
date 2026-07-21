# virtues

## Branching

**`staging` is the mainline. `main` is a release marker.**

`main` only moves when a release ships (it last moved for v0.2.0). It is
routinely ~100 commits behind `staging`. Branching from it means starting on
released code and owing a large catch-up merge before anything can land.

- **Start every branch from `origin/staging`**, and fetch first:

  ```sh
  git fetch origin && git switch -c <name> origin/staging
  ```

  Use `origin/staging`, not the local `staging` ref, which may be stale.

- **Never commit directly to `staging` or `main`.** Work on a branch and merge.
- Rebase long-lived branches onto `origin/staging` regularly. A branch that
  drifts far enough stops being mergeable: `feat/composability` reached 181
  commits behind, and by then staging had independently rewritten the same
  areas, so most of its work had to be dropped rather than merged.
- **Hotfixes to released code** branch from the release tag (e.g. `v0.2.0`),
  not from `main` — the tag stays correct after `main` moves. Merge the fix to
  both `main` and `staging`.

Branch naming follows `feat/`, `fix/`, `chore/`, `docs/`.

## Builds

The virtues repos share one cargo target directory, set by `.cargo/config.toml`
at each repo root (untracked, listed in `.git/info/exclude`):

```
~/.cargo/shared-target
```

So **there is no `./target` in this repo** — build output lives at the path
above. This exists because `target/` is ~67GB and agent sessions run in git
worktrees; without sharing, each worktree cold-builds its own copy and fills the
disk. Cargo finds the config by walking up from the working directory, so
worktrees nested under the repo inherit it automatically.

Do not move this setting to `~/.cargo/config.toml`. Other unrelated Rust
projects live on this machine, and `cargo clean` in any of them deletes the
whole shared target directory.

Checks:

```sh
cargo check --workspace     # Rust
cd apps/web && pnpm check   # Svelte
```

## Conventions

- `applets/user/` is per-box runtime state. It is gitignored and must not be
  committed.
- Model selection goes through the slot system and registry — no model-id
  literals in code.
