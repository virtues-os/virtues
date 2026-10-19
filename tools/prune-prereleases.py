#!/usr/bin/env python3
"""Delete the server prereleases a stable release has overtaken.

A `vX.Y.Z-staging.N` release is spent once a stable release at or above X.Y.Z
exists: semver orders it below that stable, so `virtues upgrade --pre` never
offers it again. They are cut several times a week and pile up, and the
upgrader and bootstrap.sh read only the newest 30 releases, so enough of them
push the newest stable out of the window those two look in.

Always kept:
  - the KEEP newest server prereleases, whatever their version, so a recent
    build stays installable by tag for a rollback or a bisect;
  - every stable release, `edge`, and every other product's release
    (`mac-*`, `win-*`, `linux-desktop-*`, `models-*`);
  - every git tag. Only the GitHub release and its assets go. The release-tag
    ruleset forbids deleting the tag in any case.

Lists what it would delete unless given --apply. Needs the gh CLI, signed in.

  tools/prune-prereleases.py
  tools/prune-prereleases.py --apply
  tools/prune-prereleases.py --keep 10
"""

import argparse
import json
import re
import subprocess
import sys

SEMVER = re.compile(r"^v(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.]+))?$")


def releases(repo):
    out = subprocess.run(
        ["gh", "api", "--paginate", f"repos/{repo}/releases",
         "--jq", ".[] | {tag_name, prerelease, draft, created_at}"],
        check=True, capture_output=True, text=True,
    ).stdout
    return [json.loads(line) for line in out.splitlines() if line.strip()]


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--repo", default="virtues-os/virtues")
    ap.add_argument("--keep", type=int, default=5,
                    help="newest server prereleases to keep regardless (default 5)")
    ap.add_argument("--apply", action="store_true", help="delete; without it, only list")
    args = ap.parse_args()
    if args.keep < 0:
        ap.error("--keep cannot be negative")

    stable, pre = [], []
    for r in releases(args.repo):
        m = SEMVER.match(r["tag_name"])
        if r["draft"] or not m:
            continue
        base = tuple(int(x) for x in m.group(1, 2, 3))
        if m.group(4) is None and not r["prerelease"]:
            stable.append(base)
        elif m.group(4) is not None and r["prerelease"]:
            pre.append((r["created_at"], r["tag_name"], base))

    if not stable:
        print("no stable server release, so no prerelease is spent; nothing to do")
        return 0
    newest = max(stable)

    pre.sort(reverse=True)  # newest first; created_at is ISO 8601, so it sorts as text
    kept = {tag for _, tag, _ in pre[: args.keep]}
    spent = [tag for _, tag, base in pre if base <= newest and tag not in kept]

    print(f"newest stable: v{'.'.join(map(str, newest))}; "
          f"{len(pre)} server prereleases, newest {args.keep} kept")
    if not spent:
        print("nothing to delete")
        return 0
    for tag in spent:
        if args.apply:
            # No --cleanup-tag: the tag stays, and only the release goes.
            subprocess.run(["gh", "release", "delete", tag, "--yes", "--repo", args.repo], check=True)
            print(f"deleted the release {tag} (its tag stays)")
        else:
            print(f"would delete the release {tag}")
    if not args.apply:
        print("dry run; --apply deletes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
