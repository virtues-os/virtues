---
paths:
  - "virtues-core/migrations/**"
---

# Claiming a migration number

Loaded because you are in the migrations directory. Three of the traps below
have each taken a real box down. The placeholder trap makes a box refuse to boot
on its NEXT start, so you do not see the mistake when you make it.

**Claim the number before writing the SQL:**

```sh
make migration NAME=add_foo
```

It takes the next number, writes a placeholder, and commits it under the lock
— so the number is reserved for you before another agent can take it. (The chain was squashed to
a single `0001_initial.sql` on 2026-08-18 and has grown since; the counter
reads the directory, so never pick a number by hand.) Otherwise two agents will usually
pick the same number, and git will not warn you:
`sqlx::migrate!` keys on the version, and renumbering after a box has applied
it breaks that box's upgrades. Migration 52 once killed a box for 3¼ hours.

**A migration number above the directory's highest is a PRE-SQUASH number.**
Comments across the tree cite migrations 0037, 0051, 0071, 0080, 0081, 0101
and others — around forty of them. None of those files exist, and a reader
who goes looking concludes the comment is wrong. They are not: they name
migrations from the 106-file chain that `d34f1e2b` collapsed on 2026-08-18,
and they are readable there:

```sh
git show d34f1e2b^:virtues-core/migrations/ | grep 0081
```

Don't add new ones — describe what changed instead of citing a number nobody
can resolve — and
don't "fix" an old one by deleting the number, which throws away the only
clue to when it happened.

**`make migration` COMMITS the placeholder, so renaming it leaves a tracked
deletion.** Stage that deletion with your migration or the next thing that
checks the tree refuses to run — `tools/squash-migrations.sh` will not touch a
migrations directory with uncommitted changes, which is the right behavior, because it cannot
tell your leftover from another agent's in-flight work. Four such deletions
accumulated before anyone noticed.

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
