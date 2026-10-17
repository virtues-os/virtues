#!/usr/bin/env python3
"""No phone number or email address outside the fictional blocks, in committed text.

The repo is public, and real contact details have reached it in code comments,
test constants, fixtures and seeds. Fiction has a reserved block, so anything
outside it is refused:

    phones   +1 512 555 xxxx only
    emails   example.com / virtues.com (and their subdomains), plus the
             reserved example.org/.net and .example/.test/.invalid names

A legitimate service address (a vendor's support inbox, a crate author's
contact in vendored code) goes in tools/pii-allowlist.txt, one per line, or as
`@domain` for a whole domain. Keep that file short; every line is a claim that
the address belongs to no private person.

This reads the COMMITTED tree (`git ls-tree`, default HEAD), never the working
tree: in the shared checkout the working tree holds every agent's in-flight
edits, and CI reads the commit.

    tools/check-pii.py [REV]

The pre-commit hook's tools/privacy-guard.py is the other half: it knows the
owner's real values (by hash) and catches a real number inside the fictional
shape, or message text, which no pattern can.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ALLOWLIST = ROOT / "tools/pii-allowlist.txt"

FICTIONAL_DOMAINS = ("example.com", "virtues.com", "example.org", "example.net")
RESERVED_TLDS = (".example", ".test", ".invalid", ".localhost", ".local")

# A phone number with a phone's punctuation: +E.164, (512) 555-0101, 512-555-0101,
# 512.555.0101. A bare run of ten digits is a timestamp or an id far more often
# than a number, so it is not matched.
PHONE = re.compile(
    r"(?<![\w.+/-])(?:"
    r"\+\d{10,14}"
    r"|\+?1?[ .-]?\(\d{3}\)[ .-]?\d{3}[ .-]\d{4}"
    r"|(?:\+?1[ .-])?\d{3}([.-])\d{3}\1\d{4}"
    r")(?![\w-])"
)
EMAIL = re.compile(r"(?<![\w.%+-])[A-Za-z0-9][A-Za-z0-9._%+-]*@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,})\b")

# Files whose bytes are not prose anyone wrote here. THIRD_PARTY_NOTICES carries
# upstream authors' published licence contacts verbatim.
SKIP = re.compile(r"(^|/)(pnpm-lock\.yaml|Cargo\.lock|package-lock\.json|THIRD_PARTY_NOTICES\.txt)$"
                  r"|\.(min\.js|map|svg)$")
# `icon@2x.png`, `pkg@3.1.0.patch`: an asset or a package spec, not a mailbox.
NOT_A_TLD = {"png", "jpg", "jpeg", "gif", "webp", "svg", "pdf", "patch", "js", "ts", "json", "css", "rs", "md"}


def fictional_phone(raw: str) -> bool:
    digits = re.sub(r"\D", "", raw)
    if len(digits) == 10:
        digits = "1" + digits
    return digits.startswith("1512555") and len(digits) == 11


def fictional_email(domain: str) -> bool:
    d = domain.lower()
    return any(d == f or d.endswith("." + f) for f in FICTIONAL_DOMAINS) or d.endswith(RESERVED_TLDS)


def load_allowlist() -> tuple[set[str], set[str]]:
    exact, domains = set(), set()
    if ALLOWLIST.exists():
        for line in ALLOWLIST.read_text().splitlines():
            line = line.split("#", 1)[0].strip().lower()
            if line.startswith("@"):
                domains.add(line[1:])
            elif line:
                exact.add(line)
    return exact, domains


def git(*args: str) -> bytes:
    return subprocess.run(["git", "-C", str(ROOT), *args], check=True, capture_output=True).stdout


def main() -> int:
    rev = sys.argv[1] if len(sys.argv) > 1 else "HEAD"
    exact, domains = load_allowlist()
    paths = [p for p in git("ls-tree", "-r", "-z", "--name-only", rev).decode().split("\0")
             if p and not SKIP.search(p) and p != "tools/pii-allowlist.txt"]

    # One cat-file process for every blob, not one `git show` per file.
    spec = "".join(f"{rev}:{p}\n" for p in paths).encode()
    out = subprocess.run(["git", "-C", str(ROOT), "cat-file", "--batch"], input=spec,
                         check=True, capture_output=True).stdout
    hits = []
    pos = 0
    for path in paths:
        header_end = out.index(b"\n", pos)
        header = out[pos:header_end].split()
        size = int(header[2]) if len(header) == 3 else 0
        blob = out[header_end + 1:header_end + 1 + size]
        pos = header_end + 1 + size + 1
        if b"\0" in blob[:8000]:
            continue
        for no, line in enumerate(blob.decode("utf-8", "replace").splitlines(), 1):
            for m in PHONE.finditer(line):
                if not fictional_phone(m.group(0)) and m.group(0) not in exact:
                    hits.append((path, no, "phone", m.group(0)))
            for m in EMAIL.finditer(line):
                addr, domain = m.group(0).lower(), m.group(1).lower()
                if domain.rsplit(".", 1)[1] in NOT_A_TLD:
                    continue
                if fictional_email(domain) or addr in exact or domain in domains:
                    continue
                hits.append((path, no, "email", m.group(0)))

    if not hits:
        return 0
    for path, no, kind, value in hits:
        print(f"{path}:{no}: {kind} outside the fictional block: {value}")
    print(f"""
✖ {len(hits)} contact detail(s) outside the fictional blocks.
  Use +1512555xxxx and @example.com. If one is a real SERVICE address (no private
  person behind it), add it to tools/pii-allowlist.txt.""", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
