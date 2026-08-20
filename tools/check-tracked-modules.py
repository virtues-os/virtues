#!/usr/bin/env python3
"""Does every `mod` declaration in the COMMITTED tree resolve to a COMMITTED file?

The bug this catches cost three CI runs on 2026-08-17. Several agents share this
checkout, so `git commit -- <file>` on a hot file (`api/mod.rs`, `server/mod.rs`)
takes whatever else is in it. Another agent's `pub mod census;` rode along twice
while `api/census.rs` stayed untracked — so the working tree built perfectly, and
CI, which only has what was committed, could not.

`cargo check` cannot see this class of bug: it compiles the working tree, where
the missing file is sitting right there. The only honest question is what a fresh
clone would have, and `git ls-files` is what answers it.

Run it before a push. Exits non-zero on any dangling declaration.
"""
import re
import subprocess
import sys

DECL = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([a-z0-9_]+)\s*;")
# `#[path = "..."]` points the declaration somewhere else entirely, so the
# name tells us nothing about the filename. Skipping these is what keeps the
# check quiet enough to be believed — vendor/noq-udp and the plaid applets use
# them legitimately, and a check that cries wolf is a check nobody reads.
PATH_ATTR = re.compile(r'^\s*#\[\s*path\s*=')


def git(*args: str) -> str:
    return subprocess.run(
        ["git", *args], capture_output=True, text=True, check=False
    ).stdout


def main() -> int:
    tracked = set(git("ls-files").splitlines())
    roots = [f for f in tracked if f.endswith(("/mod.rs", "/lib.rs", "/main.rs"))]

    findings = []
    for root in roots:
        # HEAD, not the working tree: the whole point is what was committed.
        body = git("show", f"HEAD:{root}")
        parent = root.rsplit("/", 1)[0]
        pathed = False
        for line in body.splitlines():
            if PATH_ATTR.match(line):
                pathed = True
                continue
            m = DECL.match(line)
            if not m:
                # Attributes and comments may sit between `#[path]` and the
                # declaration; anything else ends its reach.
                if line.strip() and not line.lstrip().startswith(("#[", "//")):
                    pathed = False
                continue
            if pathed:
                pathed = False
                continue
            name = m.group(1)
            if (
                f"{parent}/{name}.rs" not in tracked
                and f"{parent}/{name}/mod.rs" not in tracked
            ):
                findings.append((root, name, parent))

    if not findings:
        print("  ✓  every committed mod declaration has a committed file")
        return 0

    for root, name, parent in findings:
        print(f"  ✖  {root} declares `mod {name}`, but neither")
        print(f"       {parent}/{name}.rs nor {parent}/{name}/mod.rs is tracked")
    print()
    print("  A fresh clone — and CI — would fail to build.")
    print("  Commit the missing file, or drop the declaration if it is not yours.")
    return 1


if __name__ == "__main__":
    sys.exit(main())
