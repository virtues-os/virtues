#!/usr/bin/env python3
"""Check `docs/` — the public manual — for the drift English cannot catch.

WHY THIS EXISTS

Two failures, both real, both cheap to prevent here and expensive anywhere
else:

1. A page listed in manifest.json with no file on disk, or a file nobody
   listed. The website builds its nav, prerender entries, sitemap and
   llms.txt from that manifest, so a mismatch fails the SITE build — in the
   other repo, landing on whoever next deploys it, quite possibly someone
   pushing a marketing change who has never heard of docs/. The person who
   broke it should be the person who sees it.

2. A hardcoded release number in prose. CLAUDE.md asserted "No stable release
   exists right now" for days after v0.1.3 shipped, an agent read it as
   current, and the claim reached a user-facing page telling box owners the
   channel they were running did not exist. Prose has no compiler; this is
   the closest thing available.

The rule for versions: PROSE DESCRIBES SHAPE, FENCES SHOW EXAMPLES. Write
"versioned like `vX.Y.Z-staging.N`" in a sentence, and put the concrete
`sudo virtues upgrade --version v0.1.4` in a code block, where a reader
understands they are seeing one example rather than a claim about what is
current. Fenced blocks are therefore exempt and sentences are not.
Pages under `whats-new/` are exempt too: each is pinned to its release tag.

Deliberately NOT checked: whether every `virtues …` invocation parses against
the clap definitions. That is a parser's worth of work for a fraction of the
value these three checks give.

Usage: tools/check-manual.py [docs_dir]
"""

import json
import re
import sys
from pathlib import Path

VERSION_RE = re.compile(r"\bv\d+\.\d+\.\d+")
FENCE_RE = re.compile(r"^\s*```")
FRONTMATTER_RE = re.compile(r"\A---\n(.*?)\n---\n", re.DOTALL)
LINK_RE = re.compile(r"\[[^\]]*\]\((/docs[^)\s]*)\)")

errors: list[str] = []


def error(where: str, msg: str) -> None:
    errors.append(f"{where}: {msg}")


def strip_fences(text: str) -> str:
    """Blank out fenced code blocks, keeping line numbers intact."""
    out, in_fence = [], False
    for line in text.split("\n"):
        if FENCE_RE.match(line):
            in_fence = not in_fence
            out.append("")
        else:
            out.append("" if in_fence else line)
    return "\n".join(out)


def frontmatter(text: str) -> dict[str, str]:
    m = FRONTMATTER_RE.match(text)
    if not m:
        return {}
    fields = {}
    for line in m.group(1).split("\n"):
        if ":" in line and not line.startswith((" ", "\t", "-")):
            key, _, value = line.partition(":")
            fields[key.strip()] = value.strip()
    return fields


AGENT_DIRS = ("build", "plan", "record", "archive")
ROW_RE = re.compile(r"^\|\s*\[[^\]]+\]\(([^)\s]+?)/?\)", re.M)
MD_LINK_RE = re.compile(r"\]\((?:<([^>#]+)[^>]*>|([^)#\s]+))")
INLINE_CODE_RE = re.compile(r"`[^`\n]*`")


def check_agents(agents_root: Path) -> None:
    """The rules in agents/README.md that a machine can hold.

    1. Every doc in build/, plan/, record/ and archive/ has a row in its
       directory's README. An unlisted doc is one nobody finds.
    2. Every relative link in agents/ resolves. Docs move between genres as
       they ship; a move without a link sweep strands every citation of the
       doc, and the reader who follows one concludes the doc was deleted.
    3. Only plan/ carries a Status column. A build doc is maintained or
       deleted, a record is dated, so "Current" there is a claim nobody
       re-checks.

    None of this publishes (the site stopped syncing agents/ on 2026-08-28),
    so these are internal rules, but the repo is public.
    """
    if not agents_root.exists():
        return
    for d in AGENT_DIRS:
        readme = agents_root / d / "README.md"
        if not readme.exists():
            error(f"agents/{d}", "has no README.md index")
            continue
        text = readme.read_text()
        listed = {m.split("/")[0] for m in ROW_RE.findall(text)}
        for path in sorted((agents_root / d).iterdir()):
            if path.name == "README.md" or path.name.startswith("."):
                continue
            if path.name not in listed:
                error(f"agents/{d}/{path.name}", f"not listed in agents/{d}/README.md — add a row")
        if d != "plan" and re.search(r"^\|\s*Doc\s*\|\s*Status\s*\|", text, re.M):
            error(f"agents/{d}/README.md", "has a Status column — status belongs to plan/ only")

    for path in sorted(agents_root.rglob("*.md")):
        prose = INLINE_CODE_RE.sub("", strip_fences(path.read_text()))
        for i, line in enumerate(prose.split("\n"), 1):
            for angled, bare in MD_LINK_RE.findall(line):
                target = angled or bare
                if re.match(r"^[a-z]+:", target) or target.startswith("/"):
                    continue
                dest = re.sub(r":\d+(-\d+)?$", "", target)
                if not (path.parent / dest).exists():
                    error(f"{path.relative_to(agents_root.parent)}:{i}", f"link '{target}' does not resolve")


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "docs")
    agents_root = root.parent / "agents"
    manifest_path = root / "manifest.json"
    if not manifest_path.exists():
        print(f"check-manual: no manifest at {manifest_path}", file=sys.stderr)
        return 2

    manifest = json.loads(manifest_path.read_text())
    listed: dict[str, dict] = {}

    def walk(entries: list, depth: int, where: str) -> None:
        """The manifest is a tree: an entry is a page or a group of entries.

        Three levels is the cap (section -> group -> page). Deeper than that
        means the outline is wrong, not that the schema is too shallow, so it
        is an error rather than a silently-accepted nesting.
        """
        for entry in entries:
            if "pages" in entry:
                if depth >= 2:
                    error("manifest.json", f"group '{entry['title']}' nests deeper than section → group → page")
                walk(entry["pages"], depth + 1, f"{where} → {entry['title']}")
                continue
            if entry["slug"] in listed:
                error("manifest.json", f"duplicate slug '{entry['slug']}'")
            listed[entry["slug"]] = entry

    for section in manifest["sections"]:
        walk(section["pages"], 1, section["title"])

    published = {s: p for s, p in listed.items() if not p.get("planned")}

    # 1. Manifest and disk agree. A planned page must NOT have a file — that
    #    means someone wrote it and forgot to drop the flag, so it stays
    #    invisible on the site for no reason.
    for slug, page in listed.items():
        path = root / f"{slug}.md"
        if page.get("planned") and path.exists():
            error(f"{slug}.md", "file exists but manifest still marks it planned — drop the flag")
        elif not page.get("planned") and not path.exists():
            error("manifest.json", f"'{slug}' is listed but {path} does not exist")

    for path in sorted(root.rglob("*.md")):
        if path.name == "README.md" and path.parent == root:
            continue  # the contract for authors, not a published page
        slug = str(path.relative_to(root))[: -len(".md")]
        if slug not in listed:
            error(str(path.relative_to(root)), "not listed in manifest.json — it will not publish")

    # 2 & 3. Per-page checks, published pages only.
    for slug in sorted(published):
        path = root / f"{slug}.md"
        if not path.exists():
            continue
        text = path.read_text()
        where = f"{slug}.md"

        meta = frontmatter(text)
        for field in ("title", "description"):
            if not meta.get(field):
                error(where, f"frontmatter is missing '{field}'")

        # A release note is pinned to its tag, so its version is a fact about
        # that release, not a claim about what is current.
        prose = "" if slug.startswith("whats-new/") else strip_fences(text)
        for i, line in enumerate(prose.split("\n"), 1):
            for match in VERSION_RE.finditer(line):
                error(
                    f"{where}:{i}",
                    f"hardcoded version '{match.group()}' in prose — describe the shape "
                    f"(vX.Y.Z) and keep concrete examples in code blocks",
                )

        for target in LINK_RE.findall(text):
            dest = target.split("#")[0].removesuffix(".md")
            dest_slug = "index" if dest == "/docs" else dest.removeprefix("/docs/")
            if dest_slug not in published:
                error(where, f"link to '{target}' does not resolve to a published page")

    check_agents(agents_root)

    if errors:
        print(f"check-manual: {len(errors)} problem(s)\n", file=sys.stderr)
        for e in errors:
            print(f"  {e}", file=sys.stderr)
        return 1

    written = len(published)
    planned = len(listed) - written
    agent_docs = sum(1 for p in agents_root.rglob("*.md") if p.name != "README.md") if agents_root.exists() else 0
    print(f"check-manual: ok — {written} published, {planned} planned, {agent_docs} agent docs")
    return 0


if __name__ == "__main__":
    sys.exit(main())
