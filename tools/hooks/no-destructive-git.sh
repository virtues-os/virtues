#!/usr/bin/env python3
"""PreToolUse(Bash) — enforce CLAUDE.md's "Never" table.

Several agents share one checkout: one HEAD, one index, one stash, one working
tree. CLAUDE.md lists seven git commands that destroy other agents' work and
says every one of them "has already caused a real loss in this repo". That list
has been prose, and prose is a request. This is the same list as a gate.

It also enforces the pathspec rule, which is the one with a documented
casualty: without `-- <paths>`, `git commit` takes whatever anyone else has
staged, and a privacy-claim correction once shipped inside a commit titled
"rename the HTTP surface".

Blocking is exit code 2; stderr goes back to the model, so the message has to
say what to do instead, not just "no".

DELIBERATE NON-GOALS
  - `make commit` is the sanctioned path and contains no `git commit`, so it
    passes untouched.
  - Read-only stash inspection (`git stash list|show`) is allowed; it pockets
    nothing.
  - A human who genuinely needs one of these can run it in their own terminal.
    This gates the agent, not the person.

It also keeps the pre-commit hook in force. tools/githooks/pre-commit runs
CI's ratchets against the tree being committed, and each of these skips it
without a trace: `--no-verify`, `commit -n`, a `-c core.hooksPath=` override,
the GIT_CONFIG_* environment, rewriting core.hooksPath, and `commit-tree`,
which makes a commit no hook ever sees (one such commit raised seven ratchets
at once).

Git's global options (`git -C dir`, `git -c k=v`, `--no-pager`) are parsed off
before matching, so `git -C . reset --hard` is `git reset --hard`.

KNOWN IMPRECISION, stated rather than hidden: the command is matched
syntactically, so a banned verb inside a quoted string or a heredoc can trip
it. That is the safe direction to be wrong in, and `VIRTUES_ALLOW_DESTRUCTIVE=1`
is the escape hatch for the rare real case.
"""
from __future__ import annotations

import json
import os
import re
import shlex
import sys


HEREDOC = re.compile(r"<<-?\s*[\"\']?(\w+)[\"\']?\n(.*?)^\s*\1\s*$",
                     re.DOTALL | re.MULTILINE)


def strip_heredocs(command: str) -> str:
    """Remove heredoc BODIES before looking for commands.

    This is not a nicety. In this repo every commit message is written through
    a heredoc and the messages routinely discuss git — the first real commit
    after this hook shipped was blocked by its own commit message, which
    explained the pathspec rule and therefore contained the string it bans.

    A guard that fires on prose about the guard is a guard people switch off,
    so the body of a heredoc is data and never scanned. The delimiter line and
    everything outside it still are.
    """
    return HEREDOC.sub(lambda m: f"<<{m.group(1)}\n{m.group(1)}", command)


def segments(command: str):
    """Split a shell line into command segments.

    Only the head of a segment can be a command, so this keeps `git reset
    --hard` inside `echo "don't git reset --hard"` from matching: `echo` is the
    head, and the rest is an argument.
    """
    command = strip_heredocs(command)
    return [s.strip() for s in re.split(r"(?:\|\||&&|[|;&\n])", command) if s.strip()]


# (pattern, what to do instead). Patterns match a segment from its start, after
# an optional `sudo`/env-assignment prefix and an optional wrapper like
# `tools/with-lock.sh`.
RULES = [
    (r"git\s+stash(?!\s+(list|show))",
     "The stash is repo-wide: it pockets everyone's uncommitted work.\n"
     "To set your own work aside, commit it to `wave` and revert later — commits are cheap."),
    (r"git\s+add\s+(-A\b|--all\b|\.(\s|$))",
     "This stages EVERY agent's in-flight edits into your commit.\n"
     "Stage explicit paths: `git add path/one.rs path/two.rs`."),
    (r"git\s+commit\s+(-a\b|--all\b)",
     "`-a` sweeps in everyone's modified files.\n"
     "Use `make commit MSG=\"...\" FILES=\"path/one.rs\"`."),
    (r"git\s+(restore|checkout)\s+(--\s|\S*\.(rs|ts|svelte|sql|md|toml|json))",
     "This destroys another agent's edits, unrecoverably, with no reflog to recover from."),
    (r"git\s+reset\s+--hard",
     "Wholesale destruction of the shared working tree."),
    (r"git\s+clean\s+-[a-z]*f",
     "Deletes untracked files across the whole shared tree."),
    (r"git\s+rebase",
     "Rewrites history other agents have already built on."),
    (r"git\s+push\s+.*(--force\b|--force-with-lease\b|(^|\s)-f(\s|$))",
     "Rewrites published history. If you think you need this, stop and ask the human."),
    (r"git\s+(switch|checkout)\s+(?!-{1,2}\s|\S*\.)(-b\s+)?[\w./-]+$",
     "Switching branches moves the floor under every other agent in this checkout.\n"
     "Everyone works on `wave`; reconcile with `git fetch origin && git merge origin/staging`."),
]

# `git commit` without a pathspec. Checked separately because the remedy is
# specific and the failure is silent rather than loud.
#
# ANCHORED to the head of the segment, like every rule above. It was not, and
# that asymmetry made it fire on any line merely CONTAINING the words — a test
# script listing the commands it expects to block, for instance. A guard that
# cannot be written about is a guard that gets turned off.
COMMIT = re.compile(r"^git\s+commit\b")
HAS_PATHSPEC = re.compile(r"\s--\s+\S")

PREFIX = re.compile(r"^(?:sudo\s+|\w+=\S+\s+|tools/with-lock\.sh\s+|command\s+)+")

# ── the git hooks ────────────────────────────────────────────────────────

# Global options that take a value: `git -C dir`, `git -c key=value`, …
GIT_GLOBAL_WITH_VALUE = {"-C", "-c", "--git-dir", "--work-tree", "--namespace",
                         "--config-env", "--exec-path", "--super-prefix"}
# Config keys an agent may read but never set, with the one value allowed.
GUARDED_KEYS = {"core.hookspath": "tools/githooks"}
# Environment that rewrites git config for one command.
GUARDED_ENV = re.compile(r"(?:^|[\s;(])(?:export\s+|env\s+(?:-\S+\s+)*)?"
                         r"(GIT_CONFIG_PARAMETERS|GIT_CONFIG_COUNT|GIT_CONFIG_KEY_\d+)=")
# `git commit` short options whose value may be glued on (`-mfix`, `-uno`).
COMMIT_VALUE_FLAGS = set("mFCctuS")

HOOKS_REMEDY = (
    "The repo's pre-commit hook runs CI's ratchets against the tree being\n"
    "committed; this skips it. If it refused your commit, fix the count it named,\n"
    "or commit with VIRTUES_SKIP_RATCHETS=1 (CI will still fail).")


def git_argv(head: str):
    """(global options, subcommand argv) for a segment that runs git, else None."""
    try:
        toks = shlex.split(head, posix=True)
    except ValueError:
        toks = head.split()
    if not toks or os.path.basename(toks[0]) != "git":
        return None
    opts, i = [], 1
    while i < len(toks) and toks[i].startswith("-"):
        t = toks[i]
        if t in GIT_GLOBAL_WITH_VALUE:  # `-c key=value`: the value is the next word
            opts.append((t, toks[i + 1] if i + 1 < len(toks) else ""))
            i += 2
            continue
        if t.startswith("-c") and not t.startswith("--"):  # `-ckey=value`
            opts.append(("-c", t[2:]))
        else:  # `--git-dir=x`, `--no-pager`
            name, _, value = t.partition("=")
            opts.append((name, value))
        i += 1
    return opts, toks[i:]


def hook_bypass(opts, argv) -> str | None:
    """Why this git invocation skips or rewires the repo's hooks, or None."""
    for name, value in opts:
        key = value.split("=", 1)[0].lower()
        if name in ("-c", "--config-env") and key in GUARDED_KEYS:
            return f"`{name} {key}=…` overrides the repo's hook configuration for this command"
    if not argv:
        return None
    sub, args = argv[0], argv[1:]
    if sub == "commit-tree":
        return "`git commit-tree` makes a commit that no hook ever sees"
    before_dashdash = args[:args.index("--")] if "--" in args else args
    # Git accepts any unambiguous prefix of a long option: `--no-veri` is `--no-verify`.
    if any(a.startswith("--no-veri") for a in before_dashdash):
        return "`--no-verify` skips the hooks"
    if sub == "commit":
        skip = False
        for a in before_dashdash:
            if skip:
                skip = False
                continue
            if a.startswith("--") or not a.startswith("-") or a == "-":
                continue
            for j, ch in enumerate(a[1:]):
                if ch == "n":
                    return "`git commit -n` is `--no-verify`: it skips the hooks"
                if ch in COMMIT_VALUE_FLAGS:
                    # The rest of this cluster, or the next word, is the value.
                    skip = j == len(a) - 2 and ch in "mFCct"
                    break
    if sub == "config":
        lowered = [a.lower() for a in args]
        for key, allowed in GUARDED_KEYS.items():
            if key not in lowered:
                continue
            if any(a in ("--get", "--get-all", "--get-regexp", "-l", "--list", "get", "list")
                   for a in lowered):
                return None
            k = lowered.index(key)
            if lowered[k + 1:k + 2] == [allowed] and "--unset" not in lowered and "--unset-all" not in lowered:
                return None
            return f"this rewrites `{key}`, which keeps the repo's hooks in force"
    return None


def hook_tamper(seg: str) -> str | None:
    """A non-git command that disables a hook file or rewires git config."""
    m = GUARDED_ENV.search(seg)
    if m:
        return f"setting {m.group(1)} rewires git config for the command"
    if "githooks" in seg and (re.match(r"^(rm|mv|unlink)\b", seg) or (
            re.match(r"^chmod\b", seg) and not re.search(r"\s(?:[ugoa]*\+x|[0-7]?[157][0-7][0-7])\s", seg))):
        return "git silently skips a hook that is missing or not executable"
    return None


def main() -> int:
    if os.environ.get("VIRTUES_ALLOW_DESTRUCTIVE") == "1":
        return 0
    try:
        payload = json.load(sys.stdin)
    except Exception:
        return 0  # Never fail closed on a malformed event; this is a guard, not a gate on the tool itself.
    if payload.get("tool_name") not in ("Bash", "PowerShell"):
        return 0
    command = (payload.get("tool_input") or {}).get("command") or ""
    if isinstance(command, list):
        command = " ".join(command)

    for seg in segments(command):
        why = hook_tamper(seg)
        head = PREFIX.sub("", seg)
        parsed = git_argv(head)
        if parsed and not why:
            why = hook_bypass(*parsed)
        if why:
            print(f"BLOCKED: {why}.\n\n{head[:100]}\n\n{HOOKS_REMEDY}", file=sys.stderr)
            return 2
        if parsed:
            # Match the rules against `git <subcommand> …`, global options removed.
            head = " ".join(["git", *(shlex.quote(a) if " " in a else a for a in parsed[1])])
        for pattern, remedy in RULES:
            if re.search(r"^" + pattern, head):
                print(
                    f"BLOCKED by CLAUDE.md's Never table: {head[:80]}\n\n{remedy}\n\n"
                    "Several agents share this checkout — see the Branching section of CLAUDE.md.",
                    file=sys.stderr,
                )
                return 2
        if COMMIT.search(head) and not HAS_PATHSPEC.search(head):
            print(
                f"BLOCKED: `git commit` without a pathspec.\n\n{head[:100]}\n\n"
                "Without `-- <paths>`, git ignores your intent and commits whatever is in the\n"
                "shared index — including other agents' staged work. This has already shipped a\n"
                "privacy-claim correction inside a commit titled \"rename the HTTP surface\".\n\n"
                "Use:  make commit MSG=\"fix(x): thing\" FILES=\"path/one.rs\"\n"
                "Or:   git add <paths> && git commit -m \"...\" -- <paths>",
                file=sys.stderr,
            )
            return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
