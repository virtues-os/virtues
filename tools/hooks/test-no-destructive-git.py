import json, subprocess, sys
def run(cmd):
    r = subprocess.run(["python3","tools/hooks/no-destructive-git.sh"],
        input=json.dumps({"tool_name":"Bash","tool_input":{"command":cmd}}),
        capture_output=True, text=True)
    return r.returncode == 2
BLOCK = ["git add -A", "git add .", "git stash", "git commit -a -m x",
         "git reset --hard", "git clean -fd", "git rebase main",
         "git push --force origin wave", "git switch staging",
         "git restore src/main.rs", 'git commit -m "no pathspec"',
         "cd /tmp && git reset --hard"]
ALLOW = ['make commit MSG="x" FILES="a.rs"',
         'git add a.rs && git commit -m "x" -- a.rs',
         "tools/with-lock.sh git commit -F /tmp/m.txt -- a.rs",
         "git status --short", "git stash list", "git fetch origin && git merge origin/staging",
         'echo "never run git reset --hard"',
         "grep -rn 'git add -A' CLAUDE.md",
         "probe 'git commit -m \"x\"'",
         "cat > /tmp/m.txt <<'EOF'\nwe ban git add -A and git commit with no -- paths\nEOF\ngit add a.rs && git commit -F /tmp/m.txt -- a.rs"]
bad = 0
for c in BLOCK:
    if not run(c): print("MISS (should block):", c); bad += 1
for c in ALLOW:
    if run(c): print("FALSE POSITIVE (should allow):", c.replace(chr(10),"\\n")[:70]); bad += 1
print(f"\n{len(BLOCK)} block cases, {len(ALLOW)} allow cases, {bad} failures")
sys.exit(1 if bad else 0)
