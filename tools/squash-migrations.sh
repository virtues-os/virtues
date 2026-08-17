#!/bin/sh
# Collapse the whole migration history into one `0001_initial.sql`, and PROVE
# the result is the same schema before touching the repo.
#
#     tools/squash-migrations.sh            # dry run: build it, prove it, stop
#     tools/squash-migrations.sh --apply    # ...and then rewrite the repo
#
# ## This breaks every box that has already migrated. On purpose.
#
# sqlx records each applied migration's version AND checksum in
# `_sqlx_migrations`, and refuses to start when the two disagree. A squashed
# `0001` has a different checksum from the original `0001`, so any box carrying
# history will refuse — that is `migrate --check` doing its job, not a bug to
# work around. The only recovery is to re-image the box, which is exactly the
# trade taken deliberately pre-launch while one box exists (it is Adam's, and
# it gets re-imaged from the master anyway).
#
# If that stops being true — if a box exists that someone would mind losing —
# this script is the wrong tool and there is no right one. Squashing is a
# one-way door you walk through before you have users.
#
# ## Two traps a plain `pg_dump > 0001_initial.sql` walks straight into
#
# Both were found by doing it (2026-08-17), and the second is fatal-on-arrival:
#
#   1. `SELECT pg_catalog.set_config('search_path', '', false)` — pg_dump emits
#      it near the top. sqlx then cannot resolve its own `_sqlx_migrations`
#      table to record the migration, and every first boot dies with
#      `relation "_sqlx_migrations" does not exist`. EVERY UNIT, on its first
#      boot, from an image that passed every other check. Safe to strip because
#      pg_dump schema-qualifies every object it writes (`public.foo`).
#
#   2. `\restrict` / `\unrestrict` — psql meta-commands (a pg_dump 18 injection
#      guard, carrying a random nonce). sqlx sends file contents to the SERVER,
#      which has never heard of a backslash command. They also make two dumps of
#      the same schema differ, so they are stripped before any comparison too.
#
# ## The proof
#
# A squash is only worth doing because it is supposed to be EQUIVALENT, so this
# refuses to rewrite anything until it has shown that it is: build one database
# from the full history, one from the squashed file (through `sqlx migrate run`,
# the way a box would), dump both, and require the diff to be empty. A squash
# that cannot prove itself is a schema change wearing a cleanup's clothes.

set -eu

say()  { printf '\n\033[1m∴  %s\033[0m\n' "$*"; }
ok()   { printf '  \033[32m✓\033[0m  %s\n' "$*"; }
die()  { printf '\n\033[1;31m✖  %s\033[0m\n\n' "$*" >&2; exit 1; }

APPLY=0
[ "${1:-}" = "--apply" ] && APPLY=1

MIG=virtues-core/migrations
[ -d "$MIG" ] || die "run me from the repo root ($MIG not found)"

# ── Nobody else may be mid-write ────────────────────────────────────────────
# Several agents share this checkout. This deletes ~100 files and rewrites the
# directory, so a migration that is uncommitted or untracked is someone's work
# in progress and would be destroyed with no way back. That is not theoretical:
# on 2026-08-17 a migration sat untracked here for days while its fifteen
# callers were half-staged in the shared index.
DIRTY=$(git status --porcelain -- "$MIG" | wc -l | tr -d ' ')
if [ "$DIRTY" -ne 0 ]; then
    git status --short -- "$MIG"
    die "the migrations directory is not clean.
   Every migration must be COMMITTED before squashing — anything uncommitted
   above is someone's in-flight work and this would delete it."
fi

command -v sqlx    >/dev/null || die "sqlx-cli not installed: cargo install sqlx-cli"
command -v psql    >/dev/null || die "psql not on PATH"
command -v pg_dump >/dev/null || die "pg_dump not on PATH"

# pg_dump refuses to dump from a server newer than itself, and a dump from an
# older one can silently omit newer syntax. Same major, or stop.
SRV=$(psql -d postgres -tAc "SHOW server_version" | cut -d. -f1)
CLI=$(pg_dump --version | sed -E 's/.* ([0-9]+).*/\1/')
[ "$SRV" = "$CLI" ] || die "pg_dump is $CLI and the server is $SRV — same major, or the dump lies"

WORK=$(mktemp -d)
A="sq_full_$$"
B="sq_one_$$"
cleanup() {
    psql -d postgres -q -c "DROP DATABASE IF EXISTS $A" >/dev/null 2>&1 || true
    psql -d postgres -q -c "DROP DATABASE IF EXISTS $B" >/dev/null 2>&1 || true
    rm -rf "$WORK"
}
trap cleanup EXIT

# Strip psql meta-commands and the nonce, so two dumps of the same schema
# compare equal. Used for BOTH sides of the proof and for the file we emit.
launder() { grep -vE '^\\(restrict|unrestrict|connect)' "$1"; }

COUNT=$(find "$MIG" -name '*.sql' | wc -l | tr -d ' ')
say "Squashing $COUNT migrations"

# ── 1. The schema, as the full history builds it ────────────────────────────
psql -d postgres -q -c "DROP DATABASE IF EXISTS $A" -c "CREATE DATABASE $A" >/dev/null 2>&1
DATABASE_URL="postgres:///$A" sqlx migrate run --source "$MIG" >/dev/null \
    || die "the existing migrations do not apply cleanly — fix that before squashing"
ok "applied $COUNT migrations to a scratch database"

pg_dump --schema-only --no-owner --no-privileges --no-comments \
        --exclude-table='_sqlx_migrations' -d "$A" > "$WORK/full.raw"
launder "$WORK/full.raw" > "$WORK/full.sql"

# ── 2. The squashed file, with both traps removed ───────────────────────────
mkdir -p "$WORK/one"
{
    cat <<'HDR'
-- ---------------------------------------------------------------------------
-- The schema, entire. Generated by tools/squash-migrations.sh — do not
-- hand-edit; add a new numbered migration instead.
--
-- This replaced the full migration history at the 0.1.0 reset. The history is
-- not lost: it is in git, and `git log -- virtues-core/migrations` reads it in
-- order. What is gone is the requirement to replay ~100 files to learn what a
-- column means, which is the whole reason to do this.
--
-- Two things are deliberately absent, and both would break a box if restored:
-- pg_dump's `SELECT pg_catalog.set_config('search_path', '', false)` (sqlx then
-- cannot find `_sqlx_migrations` and every first boot dies), and its
-- `\restrict`/`\unrestrict` psql meta-commands (sqlx talks to the server, which
-- has no backslash commands).
-- ---------------------------------------------------------------------------
HDR
    launder "$WORK/full.raw" \
      | grep -vE "^SELECT pg_catalog\.set_config\('search_path'"
} > "$WORK/one/0001_initial.sql"
ok "wrote a candidate 0001_initial.sql ($(wc -l < "$WORK/one/0001_initial.sql") lines)"

# ── 3. The proof: through sqlx, exactly as a box would ──────────────────────
psql -d postgres -q -c "DROP DATABASE IF EXISTS $B" -c "CREATE DATABASE $B" >/dev/null 2>&1
DATABASE_URL="postgres:///$B" sqlx migrate run --source "$WORK/one" >/dev/null \
    || die "the squashed migration does not apply through sqlx — see the traps at the top of this file"
ok "squashed migration applies through sqlx"

pg_dump --schema-only --no-owner --no-privileges --no-comments \
        --exclude-table='_sqlx_migrations' -d "$B" > "$WORK/one.raw"
launder "$WORK/one.raw" > "$WORK/one.sql"

if ! diff -q "$WORK/full.sql" "$WORK/one.sql" >/dev/null; then
    say "NOT EQUIVALENT — the squash would change the schema"
    diff "$WORK/full.sql" "$WORK/one.sql" | head -60
    die "refusing to touch the repo"
fi
ok "schemas are identical — $COUNT migrations == 1 file"

if [ "$APPLY" -eq 0 ]; then
    # Deliberately NOT into the repo. Several agents share this checkout, and a
    # stray file in the root is how one ends up inside somebody's commit — it
    # has happened here before, with a worktree. Copied OUT of $WORK, which the
    # exit trap removes, into a path that survives this process.
    OUT="${TMPDIR:-/tmp}/0001_initial.candidate.sql"
    cp "$WORK/one/0001_initial.sql" "$OUT"
    say "Dry run — the repo was not touched"
    cat <<EOF

  The proven file is at:
      $OUT

  Read it, then rewrite the repo with:
      tools/squash-migrations.sh --apply

EOF
    exit 0
fi

# ── 4. Rewrite the repo ─────────────────────────────────────────────────────
say "Applying"
find "$MIG" -name '*.sql' -delete
cp "$WORK/one/0001_initial.sql" "$MIG/0001_initial.sql"
ok "$MIG now holds one migration"

cat <<'EOF'

  NOT DONE YET — two things this cannot do for you:

  1. Regenerate the query cache. Every `sqlx::query!` was verified against the
     old schema and its cache entry is keyed to it:

         cargo sqlx prepare --workspace

  2. Re-image the box. Any box carrying the old history has checksums that no
     longer match and will refuse to migrate. That is the deliberate cost —
     see the header of this script.

  Then commit the whole directory in ONE commit: a half-squashed history is
  worse than either end of it.

EOF
