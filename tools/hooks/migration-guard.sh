#!/usr/bin/env python3
"""PreToolUse(Bash) — refuse a commit that would leave a migration half-made.

Two states in `virtues-core/migrations/` are silently destructive. Both are
invisible when you create them and surface on somebody else's next boot, which
is exactly why a human reviewing the diff does not catch them.

1. A `.sql` file with no executable SQL in it.
   `sqlx::migrate!` globs `*.sql`, so a comment-only file is a VALID migration
   that does nothing. Any box that boots in the window records it as applied.
   When the real SQL is written the checksum no longer matches what the
   database stored, and the NEXT boot refuses to start. This took the shared
   dev box down on 2026-08-04.

   This is why `make migration` writes `.sql.pending`: the suffix reserves the
   number (the counter reads any filename starting with digits) while keeping
   the file invisible to sqlx until you rename it.

2. Both `NNNN_x.sql` and `NNNN_x.sql.pending` present.
   The rename happened but the placeholder's deletion was never staged.
   `tools/squash-migrations.sh` then refuses to run against the directory —
   correctly, because it cannot tell your leftover from another agent's
   in-flight work. Four of these accumulated before anyone noticed.

A lone `.sql.pending` is NOT flagged: that is the sanctioned state between
claiming a number and writing the SQL, and `make migration` commits it on
purpose.

Fires only on a commit, because that is where the state gets locked in and
shared. Exit 2 blocks; stderr goes back to the model.
"""
import json
import os
import re
import sys
from pathlib import Path

MIGRATIONS = Path("virtues-core/migrations")
COMMITTING = re.compile(r"\b(git\s+commit|make\s+commit)\b")


def has_executable_sql(path: Path) -> bool:
    """True if the file contains anything sqlx would actually run."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        return True  # Unreadable is not our call to make; don't block on it.
    for raw in text.splitlines():
        line = raw.strip()
        if not line or line.startswith("--"):
            continue
        return True
    return False


def number_of(name: str) -> str:
    m = re.match(r"(\d+)", name)
    return m.group(1) if m else ""


def main() -> int:
    if os.environ.get("VIRTUES_SKIP_MIGRATION_GUARD") == "1":
        return 0
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0
    if payload.get("tool_name") not in ("Bash", "PowerShell"):
        return 0
    command = (payload.get("tool_input") or {}).get("command") or ""
    if not COMMITTING.search(command):
        return 0

    root = Path(payload.get("cwd") or ".")
    migrations = root / MIGRATIONS
    if not migrations.is_dir():
        return 0

    empty, doubled = [], []
    sql = {number_of(p.name): p for p in migrations.glob("*.sql")}
    for path in sorted(migrations.glob("*.sql")):
        if not has_executable_sql(path):
            empty.append(path.name)
    for path in sorted(migrations.glob("*.sql.pending")):
        if number_of(path.name) in sql:
            doubled.append((path.name, sql[number_of(path.name)].name))

    if not empty and not doubled:
        return 0

    out = ["BLOCKED: a migration is half-made, and this commit would share it.\n"]
    if empty:
        out.append("A .sql file with no executable SQL — sqlx will apply it as a no-op and")
        out.append("record it. When the real SQL lands the checksum changes and the next boot")
        out.append("REFUSES TO START (this took the dev box down on 2026-08-04):\n")
        out += [f"    {n}" for n in empty]
        out.append("\n  Fix: write the SQL, or rename it back to .sql.pending to keep the number")
        out.append("  reserved while staying invisible to sqlx.\n")
    if doubled:
        out.append("Both the placeholder and the real migration exist — the rename left a")
        out.append("deletion that was never staged, and squash-migrations will refuse to run:\n")
        out += [f"    {p}  alongside  {s}" for p, s in doubled]
        out.append("\n  Fix: stage the deletion with your migration. Plain `git add` on a path")
        out.append("  stages deletions too, so `-A` is unnecessary (and banned here):\n")
        out.append("  git add -- virtues-core/migrations/")
        out.append("  make commit MSG=\"...\" FILES=\"virtues-core/migrations/<both names>\"\n")
    out.append("See .claude/rules/migrations.md.")
    print("\n".join(out), file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main())
