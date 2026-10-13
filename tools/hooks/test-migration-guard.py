"""The migration guard, run against throwaway repositories: what it blocks
and what it lets through. Run from the repository root:

    python3 tools/hooks/test-migration-guard.py
"""
import json, pathlib, subprocess, sys, tempfile

GUARD = pathlib.Path(__file__).resolve().parent / "migration-guard.sh"
PLACEHOLDER = "-- 0002_add_foo\n-- Number claimed; SQL to follow.\n"
SQL = "-- 0002_add_foo\nALTER TABLE foo ADD COLUMN bar text;\n"


def git(repo, *args):
    subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True)


def repo():
    root = pathlib.Path(tempfile.mkdtemp())
    git(root, "init", "-q")
    git(root, "config", "user.email", "test@example.com")
    git(root, "config", "user.name", "Test")
    migrations = root / "virtues-core" / "migrations"
    migrations.mkdir(parents=True)
    (migrations / "0001_initial.sql").write_text("CREATE TABLE foo (id text);\n")
    (migrations / "0002_add_foo.sql.pending").write_text(PLACEHOLDER)
    git(root, "add", "--", "virtues-core/migrations")
    git(root, "commit", "-q", "-m", "claim 0002", "--", "virtues-core/migrations")
    return root, migrations


def blocks(root, command='git commit -m "x" -- virtues-core/migrations'):
    r = subprocess.run(
        ["python3", str(GUARD)],
        input=json.dumps({"tool_name": "Bash", "tool_input": {"command": command}, "cwd": str(root)}),
        capture_output=True,
        text=True,
    )
    return r.returncode == 2


def renamed_then_written():
    """The placeholder renamed and staged, its SQL written after: the index holds the placeholder."""
    root, m = repo()
    git(root, "mv", "virtues-core/migrations/0002_add_foo.sql.pending", "virtues-core/migrations/0002_add_foo.sql")
    (m / "0002_add_foo.sql").write_text(SQL)
    return root


def written_then_staged():
    root = renamed_then_written()
    git(root, "add", "--", "virtues-core/migrations/0002_add_foo.sql")
    return root


def renamed_unwritten():
    root, _ = repo()
    git(root, "mv", "virtues-core/migrations/0002_add_foo.sql.pending", "virtues-core/migrations/0002_add_foo.sql")
    return root


def claimed():
    return repo()[0]


BLOCK = {
    "a staged placeholder whose SQL is written on disk only": renamed_then_written,
    "a renamed placeholder with no SQL": renamed_unwritten,
}
ALLOW = {
    "a written migration staged as written": written_then_staged,
    "a claimed number, still pending": claimed,
}

bad = 0
for name, make in BLOCK.items():
    if not blocks(make()):
        print("MISS (should block):", name)
        bad += 1
for name, make in ALLOW.items():
    if blocks(make()):
        print("FALSE POSITIVE (should allow):", name)
        bad += 1
if blocks(renamed_then_written(), "git status --short"):
    print("FALSE POSITIVE (should allow): a command that commits nothing")
    bad += 1
print(f"\n{len(BLOCK)} block cases, {len(ALLOW) + 1} allow cases, {bad} failures")
sys.exit(1 if bad else 0)
