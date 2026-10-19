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
         "cd /tmp && git reset --hard",
         # global options no longer hide a rule
         "git -C . reset --hard", "git --no-pager stash", "git -c color.ui=never rebase main",
         # the privacy hooks
         'git commit --no-verify -m x -- a.rs', 'git commit -n -m x -- a.rs',
         'git commit -nm x -- a.rs', 'git commit -m x -n -- a.rs', 'git commit --no-veri -m x -- a.rs',
         "git push --no-verify origin wave", "git merge --no-verify origin/staging",
         "git -c core.hooksPath=/dev/null commit -m x -- a.rs",
         "git -c core.hookspath= push origin wave",
         "git --config-env=core.hooksPath=X commit -m x -- a.rs",
         "git config core.hooksPath /dev/null", "git config --unset core.hooksPath",
         "git config virtues.guardRequired false", "git config unset virtues.guardRequired",
         "git commit-tree HEAD^{tree} -m x",
         "GIT_CONFIG_PARAMETERS=\"'core.hooksPath'=''\" git commit -m x -- a.rs",
         "export GIT_CONFIG_COUNT=1", "env GIT_CONFIG_KEY_0=core.hooksPath git push",
         "VIRTUES_GUARD_DENYLIST=/tmp/none git commit -m x -- a.rs",
         "chmod -x tools/githooks/pre-commit", "chmod 644 tools/githooks/pre-commit",
         "rm tools/githooks/pre-push"]
ALLOW = ['make commit MSG="x" FILES="a.rs"',
         'git add a.rs && git commit -m "x" -- a.rs',
         "tools/with-lock.sh git commit -F /tmp/m.txt -- a.rs",
         "git status --short", "git stash list", "git fetch origin && git merge origin/staging",
         'echo "never run git reset --hard"',
         "grep -rn 'git add -A' CLAUDE.md",
         "probe 'git commit -m \"x\"'",
         'git commit -m "fix: -n flag" -- a.rs', "git commit -m x -- -n.rs",
         'git commit -F /tmp/m.txt -- a.rs', "git log -n 3",
         "git push -n origin wave", "git config core.hooksPath tools/githooks",
         "git config --get core.hooksPath", "git config virtues.guardRequired true",
         "git -C /tmp/x log --oneline", "git -c color.ui=never diff",
         "grep -n VIRTUES_GUARD_DENYLIST tools/privacy-guard.py", "ls tools/githooks",
         "chmod +x tools/githooks/pre-push", "chmod 755 tools/githooks/commit-msg",
         "cat > /tmp/m.txt <<'EOF'\nwe ban git add -A and git commit with no -- paths\nEOF\ngit add a.rs && git commit -F /tmp/m.txt -- a.rs"]
bad = 0
for c in BLOCK:
    if not run(c): print("MISS (should block):", c); bad += 1
for c in ALLOW:
    if run(c): print("FALSE POSITIVE (should allow):", c.replace(chr(10),"\\n")[:70]); bad += 1
print(f"\n{len(BLOCK)} block cases, {len(ALLOW)} allow cases, {bad} failures")
sys.exit(1 if bad else 0)
