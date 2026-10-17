#!/usr/bin/env python3
"""Refuse a commit that carries data from the owner's real record.

The repo is public, and agents with access to a real database have copied rows
out of it into tracked files: message text into a seed generator, a day of GPS
points into a seed, contacts' numbers into tests. Each time it took a history
rewrite to get it back out. This blocks the next one at commit time.

It works from a denylist built off the real database on this machine and kept
OUTSIDE the repo. The denylist holds only keyed hashes, so the checker can say
"this file carries a real phone number" without either the repo or its own
output ever holding the number.

    privacy-guard.py build [--db virtues_real]   # (re)build the denylist
    privacy-guard.py check                       # scan the staged diff (pre-commit)
    privacy-guard.py allow --last                # accept the last check's hits
    privacy-guard.py allow --phrase "text"       # accept a generic phrase's shingles
    privacy-guard.py allow --tree HEAD           # accept everything already committed

What is hashed, and how a line is matched against it:

    phone    last 10 digits of contacts' and senders' numbers
    email    contacts', senders' and attendees' addresses, lowercased
    name     contacts' full names (2-4 words), lowercased
    shingle  every 8-word window of message bodies and recording transcripts
             (a hit needs two overlapping windows: nine words in a row)
    cell     ~100 m grid cells (and their neighbours) around places with 10h+ of visits

A shingle that occurs in many different messages is boilerplate (an airline
announcement, an email footer, a verification-code text) and is left out at
build time. Anything else generic that trips the check goes in allow.json via
`allow`, by hash, never by value.

`allow --tree` is for a fresh denylist on a clean tree: the owner's own writing
(the founder's letter, prompts they also sent as messages) and their public
identity are committed on purpose, and an edit near them must not trip the
check. It accepts what is committed at that revision and nothing after it, so
run it only on a tree you know is clean.

Paths: VIRTUES_GUARD_DENYLIST names denylist.json (default
~/.config/virtues-guard/denylist.json); shingles.bin, allow.json and
last-hits.json sit beside it.
"""

from __future__ import annotations

import argparse
import array
import bisect
import gzip
import hashlib
import json
import os
import re
import secrets
import subprocess
import sys
from collections import Counter
from pathlib import Path

SHINGLE = 8
# A shingle seen in this many distinct messages/transcripts is boilerplate.
GENERIC_MIN_DOCS = 5
VISIT_MINUTES = 600
CELL = 1000  # grid cells per degree: 0.001° ≈ 111 m of latitude
CONSUMER_MAIL = {
    "gmail.com", "googlemail.com", "icloud.com", "me.com", "mac.com", "yahoo.com",
    "hotmail.com", "outlook.com", "live.com", "msn.com", "aol.com", "proton.me",
    "protonmail.com", "pm.me", "fastmail.com", "hey.com", "gmx.com", "ymail.com",
}
# Words that make a contact "name" a business or a label rather than a person.
NOT_A_PERSON = {
    "mom", "dad", "home", "work", "cell", "mobile", "office", "the", "and", "of",
    "support", "team", "store", "bank", "service", "services", "inc", "llc",
    "customer", "care", "help", "info", "no", "reply", "noreply", "apple",
    "google", "amazon", "account", "alerts", "alert", "notifications",
}

ESCAPE = re.compile(r"\\(?:u[0-9a-fA-F]{4}|[nrtbf\"'\\/])")
WORD = re.compile(r"[^\W_]+")
PHONE = re.compile(r"(?<![\w.])(?:\+?\d{1,3}[\s.-]?)?\(?\d{3}\)?[\s.-]?\d{3}[\s.-]?\d{4}(?![\w])")
EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")
NUMBER = re.compile(r"-?\d{1,3}\.\d{3,}")


def guard_paths() -> dict[str, Path]:
    deny = Path(os.environ.get("VIRTUES_GUARD_DENYLIST")
                or Path.home() / ".config/virtues-guard/denylist.json")
    d = deny.parent
    return {"deny": deny, "shingles": d / "shingles.bin",
            "allow": d / "allow.json", "last": d / "last-hits.json"}


# ── features ──────────────────────────────────────────────────────────────


def tokens(text: str) -> list[str]:
    return WORD.findall(ESCAPE.sub(" ", text).lower())


def phone_key(raw: str) -> str | None:
    digits = re.sub(r"\D", "", raw)
    return digits[-10:] if len(digits) >= 10 else None


def name_key(raw: str) -> str | None:
    words = raw.split()
    if not 2 <= len(words) <= 4:
        return None
    if not all(w[:1].isupper() and w.isalpha() and len(w) >= 2 for w in words):
        return None
    toks = [w.lower() for w in words]
    if any(t in NOT_A_PERSON for t in toks):
        return None
    return " ".join(toks)


def cell_key(lat: float, lon: float) -> str:
    return f"{int(lat * CELL // 1)},{int(lon * CELL // 1)}"


class Hasher:
    def __init__(self, salt: bytes):
        self.salt = salt

    def hex(self, kind: str, value: str) -> str:
        return hashlib.blake2b(f"{kind}\0{value}".encode(), digest_size=16,
                               key=self.salt).hexdigest()

    def shingle(self, words: list[str]) -> int:
        h = hashlib.blake2b(" ".join(words).encode(), digest_size=8, key=self.salt)
        return int.from_bytes(h.digest(), "little")


# ── build ─────────────────────────────────────────────────────────────────


def psql_rows(db: str, sql: str):
    """Stream one text column per row. Newlines inside a value are collapsed in SQL."""
    proc = subprocess.Popen(
        ["psql", "-X", "-At", "-v", "ON_ERROR_STOP=1", "-v", "FETCH_COUNT=5000",
         "-d", db, "-c", sql],
        stdout=subprocess.PIPE, text=True, encoding="utf-8", errors="replace")
    assert proc.stdout
    for line in proc.stdout:
        line = line.rstrip("\n")
        if line:
            yield line
    if proc.wait() != 0:
        sys.exit(f"privacy-guard: query failed against {db}")


def cmd_build(args) -> int:
    p = guard_paths()
    db = args.db
    p["deny"].parent.mkdir(parents=True, exist_ok=True)
    os.chmod(p["deny"].parent, 0o700)

    # Keep the salt across rebuilds so allow.json stays valid.
    salt = None
    if p["deny"].exists():
        salt = bytes.fromhex(json.loads(p["deny"].read_text())["salt"])
    salt = salt or secrets.token_bytes(32)
    h = Hasher(salt)

    def jsonb_strings(table: str, col: str, where: str = "true") -> str:
        return (f"select regexp_replace(v, '\\s+', ' ', 'g') from {table}, "
                f"jsonb_array_elements_text(case when jsonb_typeof({col})='array' "
                f"then {col} else '[]'::jsonb end) v where {where}")

    phones: set[str] = set()
    emails: set[str] = set()
    names: set[str] = set()

    def take_identifier(v: str):
        v = v.strip()
        if "@" in v:
            m = EMAIL.search(v)
            if m:
                emails.add(m.group(0).lower())
        else:
            k = phone_key(v)
            if k:
                phones.add(k)

    for v in psql_rows(db, jsonb_strings("wiki_people", "phones")):
        k = phone_key(v)
        if k:
            phones.add(k)
    for v in psql_rows(db, jsonb_strings("wiki_people", "emails")):
        take_identifier(v)
    for sql in (
        "select from_identifier from data_communication_message where from_identifier is not null",
        "select from_handle from data_communication_message where from_handle is not null",
        jsonb_strings("data_communication_message", "to_identifiers"),
        jsonb_strings("data_calendar_event", "attendee_identifiers"),
        "select organizer_identifier from data_calendar_event where organizer_identifier is not null",
    ):
        for v in psql_rows(db, sql):
            take_identifier(v)
    # Mail traffic is mostly services; only consumer mailboxes are people.
    for sql in (
        "select from_email from data_communication_email where from_email is not null",
        jsonb_strings("data_communication_email", "to_emails"),
        jsonb_strings("data_communication_email", "cc_emails"),
        jsonb_strings("data_communication_email", "bcc_emails"),
    ):
        for v in psql_rows(db, sql):
            m = EMAIL.search(v)
            if m and m.group(0).lower().rsplit("@", 1)[1] in CONSUMER_MAIL:
                emails.add(m.group(0).lower())

    for sql in ("select regexp_replace(name, '\\s+', ' ', 'g') from wiki_people where name is not null",
                "select distinct regexp_replace(from_name, '\\s+', ' ', 'g') from data_communication_message "
                "where from_name is not null"):
        for v in psql_rows(db, sql):
            k = name_key(v.strip())
            if k:
                names.add(k)

    # Shingles: count the distinct documents each one appears in.
    docs: Counter[int] = Counter()
    for sql in (
        "select regexp_replace(body, '\\s+', ' ', 'g') from data_communication_message "
        "where body is not null and body <> ''",
        "select regexp_replace(text, '\\s+', ' ', 'g') from data_communication_transcription "
        "where text is not null and text <> ''",
    ):
        for v in psql_rows(db, sql):
            t = tokens(v)
            docs.update({h.shingle(t[i:i + SHINGLE]) for i in range(len(t) - SHINGLE + 1)})
    shingles = sorted(k for k, n in docs.items() if n < GENERIC_MIN_DOCS)
    generic = sum(1 for n in docs.values() if n >= GENERIC_MIN_DOCS)

    # Places: total visit time per grid cell; a place with 10h+ claims its 3×3 block.
    minutes: Counter[tuple[int, int]] = Counter()
    for v in psql_rows(db, "select latitude || ' ' || longitude || ' ' || "
                           "coalesce(duration_minutes, extract(epoch from ended_at - started_at)/60, 0) "
                           "from data_location_visit where latitude is not null and longitude is not null"):
        lat, lon, mins = (float(x) for x in v.split())
        minutes[(int(lat * CELL // 1), int(lon * CELL // 1))] += mins
    cells: set[str] = set()
    for (la, lo), mins in minutes.items():
        if mins >= VISIT_MINUTES:
            for dla in (-1, 0, 1):
                for dlo in (-1, 0, 1):
                    cells.add(f"{la + dla},{lo + dlo}")

    deny = {
        "salt": salt.hex(),
        "built_from": db,
        "phone": sorted(h.hex("phone", v) for v in phones),
        "email": sorted(h.hex("email", v) for v in emails),
        "name": sorted(h.hex("name", v) for v in names),
        "cell": sorted(h.hex("cell", v) for v in cells),
    }
    tmp = p["deny"].with_suffix(".tmp")
    tmp.write_text(json.dumps(deny))
    os.chmod(tmp, 0o600)
    tmp.replace(p["deny"])
    tmp = p["shingles"].with_suffix(".tmp")
    with open(tmp, "wb") as f:
        array.array("Q", shingles).tofile(f)
    os.chmod(tmp, 0o600)
    tmp.replace(p["shingles"])

    print(f"privacy-guard: built {p['deny']} — {len(phones)} phones, {len(emails)} emails, "
          f"{len(names)} names, {len(cells)} cells, {len(shingles)} shingles "
          f"({generic} boilerplate shingles left out)")
    return 0


# ── check ─────────────────────────────────────────────────────────────────


class Denylist:
    def __init__(self, p: dict[str, Path]):
        d = json.loads(p["deny"].read_text())
        self.h = Hasher(bytes.fromhex(d["salt"]))
        self.sets = {k: set(d[k]) for k in ("phone", "email", "name", "cell")}
        self.shingles = array.array("Q")
        with open(p["shingles"], "rb") as f:
            self.shingles.frombytes(f.read())
        allow = json.loads(p["allow"].read_text()) if p["allow"].exists() else {}
        for k, s in self.sets.items():
            s.difference_update(allow.get(k, []))
        self.allow_shingles = set(allow.get("shingle", []))

    def has_shingle(self, v: int) -> bool:
        i = bisect.bisect_left(self.shingles, v)
        return i < len(self.shingles) and self.shingles[i] == v and v not in self.allow_shingles


def scan(lines: list[tuple[int, str]], deny: Denylist):
    """Yield (line, kind, hash) for every denylisted feature in these added lines."""
    h = deny.h
    for no, text in lines:
        for m in PHONE.finditer(text):
            k = phone_key(m.group(0))
            if k and (x := h.hex("phone", k)) in deny.sets["phone"]:
                yield no, "phone", x
        for m in EMAIL.finditer(text):
            if (x := h.hex("email", m.group(0).lower())) in deny.sets["email"]:
                yield no, "email", x
        t = WORD.findall(ESCAPE.sub(" ", text).lower())
        for n in (2, 3, 4):
            for i in range(len(t) - n + 1):
                if (x := h.hex("name", " ".join(t[i:i + n]))) in deny.sets["name"]:
                    yield no, "name", x

    # Shingles and coordinates run over the added text as one stream, so a
    # sentence wrapped across lines or a lat/lon pair split over two still matches.
    stream: list[tuple[str, int]] = []
    nums: list[tuple[float, int]] = []
    for no, text in lines:
        stream.extend((w, no) for w in WORD.findall(ESCAPE.sub(" ", text).lower()))
        nums.extend((float(m.group(0)), no) for m in NUMBER.finditer(text))
    # One matching 8-word window alone is too often a stock phrase ("let me know
    # if you have any questions"); two overlapping ones, nine words in a row,
    # are a copied sentence.
    words = [w for w, _ in stream]
    found = [v if deny.has_shingle(v) else None
             for v in (h.shingle(words[i:i + SHINGLE]) for i in range(len(words) - SHINGLE + 1))]
    for i, v in enumerate(found):
        if v is not None and ((i > 0 and found[i - 1] is not None)
                              or (i + 1 < len(found) and found[i + 1] is not None)):
            yield stream[i][1], "message text", f"{v:016x}"
    for i, (a, no) in enumerate(nums):
        for b, _ in nums[i + 1:i + 4]:
            for lat, lon in ((a, b), (b, a)):
                if abs(lat) <= 90 and abs(lon) <= 180:
                    if (x := h.hex("cell", cell_key(lat, lon))) in deny.sets["cell"]:
                        yield no, "place", x


def git(*args: str, binary: bool = False):
    r = subprocess.run(["git", *args], capture_output=True)
    if r.returncode != 0:
        return None
    return r.stdout if binary else r.stdout.decode("utf-8", "replace")


def staged_added() -> dict[str, list[tuple[int, str]]]:
    """Added lines per staged file. GIT_INDEX_FILE (set by `git commit -- paths`) is honoured."""
    out: dict[str, list[tuple[int, str]]] = {}
    diff = git("-c", "core.quotepath=off", "diff", "--cached", "--no-color", "--no-ext-diff", "-U0",
               "--diff-filter=ACMR", "--text", "--", ".", ":(exclude)*.gz") or ""
    path = None
    no = 0
    for line in diff.splitlines():
        if line.startswith("+++ "):
            path = line[6:].rstrip("\t") if line.startswith("+++ b/") else None
        elif line.startswith("@@"):
            m = re.search(r"\+(\d+)", line)
            no = int(m.group(1)) if m else 0
        elif line.startswith("+") and path:
            out.setdefault(path, []).append((no, line[1:]))
            no += 1

    gz = git("diff", "--cached", "--name-only", "-z", "--diff-filter=ACMR", "--", "*.gz") or ""
    for path in filter(None, gz.split("\0")):
        new = git("show", f":{path}", binary=True)
        if new is None:
            continue
        old = git("show", f"HEAD:{path}", binary=True)
        try:
            new_lines = gzip.decompress(new).decode("utf-8", "replace").splitlines()
            old_lines = set(gzip.decompress(old).decode("utf-8", "replace").splitlines()) if old else set()
        except (OSError, EOFError):
            continue
        out[path] = [(i + 1, l) for i, l in enumerate(new_lines) if l not in old_lines]
    return out


def tree_lines(rev: str):
    """Every text line committed at `rev`, .gz files decompressed."""
    for path in filter(None, (git("ls-tree", "-r", "-z", "--name-only", rev) or "").split("\0")):
        b = git("show", f"{rev}:{path}", binary=True) or b""
        if path.endswith(".gz"):
            try:
                b = gzip.decompress(b)
            except (OSError, EOFError):
                continue
        elif b"\0" in b[:8000]:
            continue
        yield path, list(enumerate(b.decode("utf-8", "replace").splitlines(), 1))


def cmd_check(args) -> int:
    p = guard_paths()
    if not p["deny"].exists() or not p["shingles"].exists():
        # A contributor's clone has no real database and no denylist; nothing to compare.
        if "VIRTUES_GUARD_DENYLIST" in os.environ or p["deny"].parent.exists():
            print(f"privacy-guard: no denylist at {p['deny']} — run `tools/privacy-guard.py build`",
                  file=sys.stderr)
        return 0
    deny = Denylist(p)
    hits = []
    for path, lines in staged_added().items():
        for no, kind, x in scan(lines, deny):
            hits.append({"file": path, "line": no, "kind": kind, "hash": x})
    if not hits:
        return 0

    p["last"].write_text(json.dumps(hits))
    os.chmod(p["last"], 0o600)
    seen = set()
    print("\n✖ commit refused: staged content matches the owner's real record.", file=sys.stderr)
    for hit in hits:
        key = (hit["file"], hit["line"], hit["kind"])
        if key not in seen:
            seen.add(key)
            print(f"    {hit['file']}:{hit['line']}  {hit['kind']}", file=sys.stderr)
    print("""
  Demo and fixture data come from tools/demo only. Replace the matched content
  with fictional values (+1512555xxxx, @example.com, names like David Okafor).
  If a hit is genuinely generic, the owner can accept it by hash:
      tools/privacy-guard.py allow --last""", file=sys.stderr)
    return 1


# ── allow ─────────────────────────────────────────────────────────────────


def cmd_allow(args) -> int:
    p = guard_paths()
    allow = json.loads(p["allow"].read_text()) if p["allow"].exists() else {}
    added = 0
    if args.last:
        for hit in json.loads(p["last"].read_text()):
            kind = "shingle" if hit["kind"] == "message text" else \
                "cell" if hit["kind"] == "place" else hit["kind"]
            v = int(hit["hash"], 16) if kind == "shingle" else hit["hash"]
            if v not in allow.setdefault(kind, []):
                allow[kind].append(v)
                added += 1
    if args.tree:
        deny = Denylist(p)
        for path, lines in tree_lines(args.tree):
            for _, kind, x in scan(lines, deny):
                kind = "shingle" if kind == "message text" else "cell" if kind == "place" else kind
                v = int(x, 16) if kind == "shingle" else x
                if v not in allow.setdefault(kind, []):
                    allow[kind].append(v)
                    added += 1
    if args.phrase:
        h = Hasher(bytes.fromhex(json.loads(p["deny"].read_text())["salt"]))
        t = tokens(args.phrase)
        for i in range(len(t) - SHINGLE + 1):
            v = h.shingle(t[i:i + SHINGLE])
            if v not in allow.setdefault("shingle", []):
                allow["shingle"].append(v)
                added += 1
    p["allow"].write_text(json.dumps(allow, indent=1))
    os.chmod(p["allow"], 0o600)
    print(f"privacy-guard: {added} hash(es) added to {p['allow']}")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--db", default="virtues_real")
    sub.add_parser("check")
    a = sub.add_parser("allow")
    a.add_argument("--last", action="store_true")
    a.add_argument("--phrase")
    a.add_argument("--tree", metavar="REV")
    args = ap.parse_args()
    return {"build": cmd_build, "check": cmd_check, "allow": cmd_allow}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
