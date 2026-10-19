#!/usr/bin/env python3
"""Refuse a commit that carries data from the owner's real record.

The repo is public, and agents with access to a real database have copied the
record into tracked files eight different ways: message text into a seed
generator, a day of GPS points into a seed, real timings shifted by two weeks
into the demo, step counts and purchase amounts into docs, contacts' numbers
into tests, a family member's name onto the website. Each time it took a
history rewrite to get it back out. This blocks the next one at write, commit
and push time.

It works from a denylist built off the real database on this machine and kept
OUTSIDE the repo. The denylist holds only keyed hashes, so the checker can say
"this file carries a real phone number" without either the repo or its own
output ever holding the number.

    privacy-guard.py build [--db virtues_real]   # (re)build the denylist
    privacy-guard.py check                       # the staged diff (pre-commit)
    privacy-guard.py check --message FILE        # a commit message (commit-msg)
    privacy-guard.py check --push                # the commits a push sends (pre-push)
    privacy-guard.py check --range A..B          # every commit in a range
    privacy-guard.py check --hook                # a Claude/Codex PreToolUse event (write time)
    privacy-guard.py audit [REV]                 # every committed line; reports, allows nothing
    privacy-guard.py fact "text"                 # add an owner-written fact, hashed
    privacy-guard.py fact --remove "text"        # ... or take one out
    privacy-guard.py allow --last                # accept the last check's hits
    privacy-guard.py allow --phrase "text"       # accept a generic phrase's shingles
    privacy-guard.py allow --tree HEAD           # accept everything already committed
    privacy-guard.py status                      # what is built, how old, whether required

What is hashed, and what refuses a file:

    phone      last 10 digits of contacts' and senders' numbers            one hit
    email      contacts', senders' and attendees' addresses                one hit
    name       contacts' full names (2-4 words)                            one hit
    shingle    every 8-word window of every text the record holds:
               messages, transcripts, email, chats, pages, wiki
               articles/days/notes/people, calendar, memories            nine words in a row
    place      ~100 m cells around places with 10h+ of visits              one hit
    point      ~11 m and ~1.1 m cells of every location point, read as
               decimal pairs, E7 integers or encoded polylines, at the
               precision the text carries                                  3 distinct cells
    timestamp  the UTC second (and millisecond, where the sample has one)
               of every location point, health sample, transaction and
               message, in ISO, Unix (s/ms/us/ns) and Apple-2001 (s/ns)
               forms, as written or shifted by whole days                 2 distinct
    pair       (date, daily steps) and (date, transaction amount)          2 distinct
    fact       short facts the owner writes in with `fact`: family first
               names, a pet, a hometown, a condition, a trip city.
               Matched in docs, seeds, fixtures, tests, prompts, agents/
               and commit messages, or everywhere with
               `git config virtues.factsScope all` (the website)          one hit

The counted kinds are tested against chance as well as counted. A seed of
invented timestamps in a month the owner's phone logged every few seconds hits
real seconds by accident, and invented coordinates in a city the owner walks
hit real cells. So a file is refused when its hits stand out from what its own
size would produce at random (Poisson, p < 1e-3), using how dense the real
record is near each value: per hour for times, per ~111 m block for points. A
copied millisecond or 1-metre cell stands out at two or three; a whole second in
an hour the phone logged every few seconds needs a few more.

A shingle that occurs in many different documents is boilerplate (an airline
announcement, an email footer) and is left out at build time. Bulk mail
(Gmail's Promotions, Updates, Social and Forums) is not read at all: it is
public text the repo may quote, and services like GitHub quote the repo back.
Anything else generic that trips the check goes in allow.json via `allow`, by
hash, never by value.

`allow --tree` is for a fresh denylist on a clean tree: the owner's own writing
(the founder's letter, prompts they also sent as messages) and their public
identity are committed on purpose, and an edit near them must not trip the
check. It accepts what is committed at that revision and nothing after it, and
it prints every file:line it accepts, so run it only on a tree you know is
clean and read what it printed.

FAIL CLOSED. With `git config virtues.guardRequired true` (set it on every
machine that holds the real record) `check` refuses when the denylist is
missing or older than 7 days. Without it, a clone with no real database and no
denylist passes, since there is nothing to compare against. Rebuild weekly:
the denylist only knows the record as of its last build.

Paths: VIRTUES_GUARD_DENYLIST names denylist.json (default
~/.config/virtues-guard/denylist.json); every other file sits beside it.
"""

from __future__ import annotations

import argparse
import array
import bisect
import calendar
import hashlib
import importlib.util
import json
import math
import os
import re
import secrets
import subprocess
import sys
import time
from collections import Counter, defaultdict
from pathlib import Path

SHINGLE = 8
# A shingle seen in this many distinct documents is boilerplate.
GENERIC_MIN_DOCS = 5
VISIT_MINUTES = 600
CELL = 1000         # place cells per degree: 0.001° ≈ 111 m of latitude
POINT_CELL = 10000  # point cells per degree: 0.0001° ≈ 11 m
POINT_FINE = 100000 # ... and 0.00001° ≈ 1.1 m, for coordinates written to 6+ decimals
BLOCK = 1000        # density blocks per degree: 0.001° ≈ 111 m, a route's own width
POINT_MIN = 3
TIME_MIN = 2
PAIR_MIN = 2
SHIFT_MIN = 4       # copies at one whole-day shift
SHIFT_DAYS = 1100   # ±3 years of whole-day shifts
SHIFT_SAMPLE = 120
ALPHA = 1e-3
MAX_AGE_DAYS = 7
APPLE_EPOCH = 978307200  # 2001-01-01T00:00:00Z
CONSUMER_MAIL = {
    "gmail.com", "googlemail.com", "icloud.com", "me.com", "mac.com", "yahoo.com",
    "hotmail.com", "outlook.com", "live.com", "msn.com", "aol.com", "proton.me",
    "protonmail.com", "pm.me", "fastmail.com", "hey.com", "gmx.com", "ymail.com",
}
# Notification mail that quotes this repo's own commits, issues and code back.
DEV_MAIL = {"github.com", "linear.app", "vercel.com", "sentry.io", "gitlab.com", "atlassian.net"}
# Mailboxes no person reads; they turn up as message and calendar identifiers.
NO_PERSON_MAILBOX = re.compile(r"^(no-?reply|do-?not-?reply|notifications?|mailer-daemon|postmaster|bounces?)([+.-]|@)")
# Words that make a contact "name" a business or a label rather than a person.
NOT_A_PERSON = {
    "mom", "dad", "home", "work", "cell", "mobile", "office", "the", "and", "of",
    "support", "team", "store", "bank", "service", "services", "inc", "llc",
    "customer", "care", "help", "info", "no", "reply", "noreply", "apple",
    "google", "amazon", "account", "alerts", "alert", "notifications",
}
# Where a fact (a family first name, a hometown) is a leak rather than a word:
# prose and data, never code identifiers.
FACT_SCOPE = re.compile(
    r"(^|/)(docs?|seeds?|fixtures?|tests?|__tests__|testdata|demo|agents|prompts?|examples?|static)/"
    r"|prompt|(^|/)README|\.(md|mdx|txt|html|svelte|sql)$|_test\.|\.test\.|\.spec\.|(^|/)test_"
    r"|^\(commit message\)$", re.I)

ESCAPE = re.compile(r"\\(?:u[0-9a-fA-F]{4}|[nrtbf\"'\\/])")
WORD = re.compile(r"[^\W_]+")
PHONE = re.compile(r"(?<![\w.])(?:\+?\d{1,3}[\s.-]?)?\(?\d{3}\)?[\s.-]?\d{3}[\s.-]?\d{4}(?![\w])")
EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")
NUMBER = re.compile(r"-?\d{1,3}\.(\d{3,})")
E7 = re.compile(r"(?<![\w.])-?\d{8,10}(?![\w.])")
POLYLINE = re.compile(r"[?-~]{20,}")
POLY_MARK = re.compile(r"[?@\[\\\]^_`{|}~]")
ISO_TS = re.compile(r"(?<![\d-])(\d{4})-(\d{2})-(\d{2})[T ](\d{2}):(\d{2}):(\d{2})(?:[.,](\d{1,9}))?"
                    r" ?(Z|[+-]\d{2}(?::?\d{2})?)?(?![\d:])")
NUM_TS = re.compile(r"(?<![\w.])(\d{9,19})(?:\.(\d{1,9}))?(?![\w])")
DATE = re.compile(r"(?<![\d-])(\d{4})-(\d{2})-(\d{2})(?![\d-])")
VALUE = re.compile(r"(?<![\w.:/-])(\$?)(\d{1,3}(?:,\d{3})+|\d+)(?:\.(\d{2}))?(?![\w.:/])")


def guard_paths() -> dict[str, Path]:
    deny = Path(os.environ.get("VIRTUES_GUARD_DENYLIST")
                or Path.home() / ".config/virtues-guard/denylist.json")
    d = deny.parent
    return {"deny": deny, "shingles": d / "shingles.bin", "points": d / "points.bin",
            "times": d / "times.bin", "pairs": d / "pairs.bin", "density": d / "density.json",
            "facts": d / "facts.json", "allow": d / "allow.json", "last": d / "last-hits.json"}


def load_check_pii():
    """tools/check-pii.py, for the one way this repo reads commits and diffs."""
    spec = importlib.util.spec_from_file_location("check_pii", Path(__file__).with_name("check-pii.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def git_config(key: str) -> str:
    r = subprocess.run(["git", "config", "--get", key], capture_output=True, text=True)
    return r.stdout.strip().lower()


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


def fine(v: float, per_degree: int = POINT_CELL) -> int:
    # The epsilon keeps 30.2671 (stored as 30.26709999…) in the cell it names.
    return math.floor(v * per_degree + 1e-6)


def block_key(lat: float, lon: float) -> str:
    return f"{math.floor(lat * BLOCK)},{math.floor(lon * BLOCK)}"


def distinctive_second(t: int) -> bool:
    """On-the-minute times are how dates and schedules are written, not samples."""
    return t % 60 != 0


class Hasher:
    def __init__(self, salt: bytes):
        self.salt = salt

    def hex(self, kind: str, value: str) -> str:
        return hashlib.blake2b(f"{kind}\0{value}".encode(), digest_size=16,
                               key=self.salt).hexdigest()

    def int(self, kind: str, value: str) -> int:
        h = hashlib.blake2b(f"{kind}\0{value}".encode(), digest_size=8, key=self.salt)
        return int.from_bytes(h.digest(), "little")

    def shingle(self, words: list[str]) -> int:
        h = hashlib.blake2b(" ".join(words).encode(), digest_size=8, key=self.salt)
        return int.from_bytes(h.digest(), "little")


def poisson_sf(m: int, lam: float) -> float:
    """P(X >= m) for X ~ Poisson(lam)."""
    if m <= 0:
        return 1.0
    if lam <= 0:
        return 0.0
    if m <= lam:
        return 1.0
    total, i = 0.0, m
    while True:
        term = math.exp(-lam + i * math.log(lam) - math.lgamma(i + 1))
        total += term
        if term < 1e-18 * max(total, 1e-300) or i > m + 10000:
            return min(total, 1.0)
        i += 1


def stands_out(m: int, lam: float, floor: int, alpha: float = ALPHA) -> bool:
    return m >= floor and poisson_sf(m, max(lam, 1e-3)) < alpha


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


def write_ints(path: Path, values) -> int:
    a = array.array("Q", sorted(set(values)))
    tmp = path.with_suffix(".tmp")
    with open(tmp, "wb") as f:
        a.tofile(f)
    os.chmod(tmp, 0o600)
    tmp.replace(path)
    return len(a)


def write_json(path: Path, obj) -> None:
    tmp = path.with_suffix(".tmp")
    tmp.write_text(json.dumps(obj))
    os.chmod(tmp, 0o600)
    tmp.replace(path)


def flat(col: str) -> str:
    return f"regexp_replace(coalesce({col}, ''), '\\s+', ' ', 'g')"


# Every text the record holds, one document per row. Order does not matter.
TEXT_SOURCES = [
    f"select {flat('body')} from data_communication_message where body <> ''",
    f"select {flat('text')} || ' ' || {flat('summary')} from data_communication_transcription",
    # Correspondence, not bulk mail: a newsletter or a product announcement is
    # public text the repo may well quote. Gmail's category labels say which is which.
    f"select {flat('subject')} || ' ' || {flat('body')} from data_communication_email "
    f"where lower(coalesce(from_email, '')) !~ '[@.]({{dev}})$' and (coalesce(labels, '[]') ? 'SENT' "
    f"or not coalesce(labels, '[]') ?| array['CATEGORY_PROMOTIONS', 'CATEGORY_UPDATES', "
    f"'CATEGORY_SOCIAL', 'CATEGORY_FORUMS'])",
    f"select {flat('content')} from app_chat_messages where content <> ''",
    f"select {flat('title')} || ' ' || {flat('content')} from app_pages",
    f"select {flat('content_preview')} from app_page_versions",
    f"select {flat('machine_text')} from wiki_articles",
    f"select {flat('prose')} from wiki_day_prose",
    f"select {flat('body')} from wiki_notes",
    f"select {flat('content')} from wiki_people",
    f"select {flat('content')} from wiki_orgs",
    f"select {flat('content')} from wiki_places",
    f"select {flat('title')} || ' ' || {flat('summary')} from wiki_chapters",
    f"select {flat('title')} || ' ' || {flat('summary')} from wiki_stories",
    f"select {flat('title')} || ' ' || {flat('summary')} from wiki_years",
    f"select {flat('event_summary')} || ' ' || {flat('user_notes')} || ' ' || {flat('user_label')} from wiki_events",
    f"select {flat('title')} || ' ' || {flat('description')} || ' ' || {flat('location_name')} from data_calendar_event",
    f"select {flat('body')} from app_assistant_memories",
    f"select {flat('note_md')} from app_marginalia",
    f"select {flat('content')} from data_content_conversation",
    f"select {flat('content')} from data_content_document",
]
# Every timestamp of a sample the owner's devices or accounts recorded.
TIME_SOURCES = [
    ("data_location_point", "occurred_at"), ("data_health_steps", "occurred_at"),
    ("data_health_heart_rate", "occurred_at"), ("data_health_hrv", "occurred_at"),
    ("data_health_active_energy", "occurred_at"), ("data_health_distance", "occurred_at"),
    ("data_health_sleep", "started_at"), ("data_health_sleep", "ended_at"),
    ("data_health_workout", "started_at"), ("data_health_workout", "ended_at"),
    ("data_financial_transaction", "occurred_at"), ("data_communication_message", "occurred_at"),
    ("data_communication_email", "occurred_at"), ("data_communication_transcription", "started_at"),
    ("data_location_visit", "started_at"), ("data_location_visit", "ended_at"),
]


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
            if m and not NO_PERSON_MAILBOX.match(m.group(0).lower()):
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
    dev = "|".join(re.escape(d) for d in sorted(DEV_MAIL))
    docs: Counter[int] = Counter()
    for sql in TEXT_SOURCES:
        for v in psql_rows(db, sql.replace("{dev}", dev)):
            t = tokens(v)
            docs.update({h.shingle(t[i:i + SHINGLE]) for i in range(len(t) - SHINGLE + 1)})
    shingles = [k for k, n in docs.items() if n < GENERIC_MIN_DOCS]
    generic = len(docs) - len(shingles)
    del docs

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

    # Points: every ~11 m and ~1.1 m cell the record has a sample in, and how
    # much of each ~111 m block those cells cover: the chance an invented point
    # there lands in one by accident.
    fine_cells: set[tuple[int, int]] = set()
    finest: set[tuple[int, int]] = set()
    for v in psql_rows(db, "select latitude || ' ' || longitude from data_location_point "
                           "where latitude is not null and longitude is not null"):
        lat, lon = (float(x) for x in v.split())
        fine_cells.add((fine(lat), fine(lon)))
        finest.add((fine(lat, POINT_FINE), fine(lon, POINT_FINE)))
    point_hashes = [h.int("point", f"{a},{b}") for a, b in fine_cells] + \
                   [h.int("point5", f"{a},{b}") for a, b in finest]
    blocks: dict[str, list[int]] = defaultdict(lambda: [0, 0])
    for a, b in fine_cells:
        blocks[f"{a * BLOCK // POINT_CELL},{b * BLOCK // POINT_CELL}"][0] += 1
    for a, b in finest:
        blocks[f"{a * BLOCK // POINT_FINE},{b * BLOCK // POINT_FINE}"][1] += 1

    # Timestamps: the UTC second of every sample, and how many per hour.
    seconds: set[int] = set()
    for table, col in TIME_SOURCES:
        for v in psql_rows(db, f"select floor(extract(epoch from {col}))::bigint from {table} "
                               f"where {col} is not null"):
            t = int(v)
            if distinctive_second(t):
                seconds.add(t)
    time_hashes = [h.int("time", str(t)) for t in seconds]
    # The millisecond too, where the sample has one: a copy carries it, and an
    # invented time almost never matches it.
    for table, col in TIME_SOURCES:
        for v in psql_rows(db, f"select floor(extract(epoch from {col}) * 1000)::bigint from {table} "
                               f"where {col} is not null and extract(microseconds from {col})::bigint % 1000000 <> 0"):
            time_hashes.append(h.int("time_ms", v))
    hours: Counter[int] = Counter(t // 3600 for t in seconds)
    data_through = max(seconds) if seconds else 0

    # Pairs: a day's steps (by local and by UTC day, in total and per source)
    # and each transaction's amount in cents, against its day.
    pairs: set[int] = set()
    for zone in ("", " at time zone 'UTC'"):
        for sql in (
            f"select date(occurred_at{zone}) || ' ' || sum(step_count) from data_health_steps "
            f"group by date(occurred_at{zone})",
            f"select date(occurred_at{zone}) || ' ' || sum(step_count) from data_health_steps "
            f"group by date(occurred_at{zone}), source_stream_id",
            f"select date(occurred_at{zone}) || ' ' || abs(amount) from data_financial_transaction",
        ):
            kind = "steps" if "step_count" in sql else "amount"
            for v in psql_rows(db, sql):
                day, value = v.split()
                if int(value) >= 100:
                    pairs.add(h.int(kind, f"{day}|{value}"))

    deny = {
        "salt": salt.hex(),
        "built_from": db,
        "built_at": int(time.time()),
        "data_through": data_through,
        "phone": sorted(h.hex("phone", v) for v in phones),
        "email": sorted(h.hex("email", v) for v in emails),
        "name": sorted(h.hex("name", v) for v in names),
        # First words alone, so a scan hashes an n-gram only where a name could start.
        "name_first": sorted({h.hex("name1", v.split()[0]) for v in names}),
        "cell": sorted(h.hex("cell", v) for v in cells),
    }
    density = {
        "block": {h.hex("block", k): n for k, n in blocks.items()},
        "hour": {h.hex("hour", str(k)): n for k, n in hours.items()},
    }
    n_shingles = write_ints(p["shingles"], shingles)
    n_points = write_ints(p["points"], point_hashes)
    n_times = write_ints(p["times"], time_hashes)
    n_pairs = write_ints(p["pairs"], pairs)
    write_json(p["density"], density)
    write_json(p["deny"], deny)  # last: its built_at says the rest is in place

    print(f"privacy-guard: built {p['deny']} — {len(phones)} phones, {len(emails)} emails, "
          f"{len(names)} names, {len(cells)} place cells, {n_points} point cells, "
          f"{n_times} timestamps, {n_pairs} day pairs, {n_shingles} shingles "
          f"({generic} boilerplate shingles left out)")
    return 0


# ── the denylist, loaded ──────────────────────────────────────────────────


class IntSet:
    """A sorted file of 8-byte hashes, minus what allow.json accepts."""

    def __init__(self, path: Path, allow: set[int]):
        self.a = array.array("Q")
        if path.exists():
            with open(path, "rb") as f:
                self.a.frombytes(f.read())
        self.allow = allow

    def __contains__(self, v: int) -> bool:
        i = bisect.bisect_left(self.a, v)
        return i < len(self.a) and self.a[i] == v and v not in self.allow


class Denylist:
    def __init__(self, p: dict[str, Path]):
        d = json.loads(p["deny"].read_text())
        self.meta = {k: d.get(k) for k in ("built_from", "built_at", "data_through")}
        self.h = Hasher(bytes.fromhex(d["salt"]))
        allow = json.loads(p["allow"].read_text()) if p["allow"].exists() else {}
        self.sets = {k: set(d.get(k, [])) - set(allow.get(k, [])) for k in ("phone", "email", "name", "cell")}
        self.name_first = set(d["name_first"]) if "name_first" in d else None
        self._first: dict[str, bool] = {}
        self.shingles = IntSet(p["shingles"], set(allow.get("shingle", [])))
        self.points = IntSet(p["points"], set(allow.get("point", [])))
        self.times = IntSet(p["times"], set(allow.get("time", [])))
        self.pairs = IntSet(p["pairs"], set(allow.get("pair", [])))
        dens = json.loads(p["density"].read_text()) if p["density"].exists() else {}
        self.blocks = dens.get("block", {})
        self.hours = dens.get("hour", {})
        self.facts = Facts(p["facts"])

    def has_shingle(self, v: int) -> bool:
        return v in self.shingles

    def may_start_name(self, word: str) -> bool:
        if self.name_first is None:
            return True
        if word not in self._first:
            self._first[word] = self.h.hex("name1", word) in self.name_first
        return self._first[word]

    def time_hash(self, t: int) -> int:
        return self.h.int("time", str(t))

    def hour_density(self, t: int) -> float:
        """The chance an invented second near t is a real one."""
        return self.hours.get(self.h.hex("hour", str(t // 3600)), 0) / 3600

    def block_density(self, lat: float, lon: float) -> tuple[float, float]:
        """The chance an invented point near (lat, lon) is in a real ~11 m, and a real ~1.1 m, cell."""
        n4, n5 = self.blocks.get(self.h.hex("block", block_key(lat, lon)), (0, 0))
        return n4 / (POINT_CELL // BLOCK) ** 2, n5 / (POINT_FINE // BLOCK) ** 2


class Facts:
    """Owner-written facts, hashed with their own salt so they need no build."""

    def __init__(self, path: Path):
        self.path = path
        d = json.loads(path.read_text()) if path.exists() else {}
        self.salt = bytes.fromhex(d["salt"]) if "salt" in d else None
        self.hashes = set(d.get("facts", []))
        self.lengths = sorted(set(d.get("lengths", [])))
        self.first = set(d.get("first", []))
        self._first: dict[str, bool] = {}

    def key(self, words: list[str]) -> str:
        return hashlib.blake2b(" ".join(words).encode(), digest_size=16, key=self.salt).hexdigest()

    def may_start(self, word: str) -> bool:
        if word not in self._first:
            self._first[word] = self.key(["\0first", word]) in self.first
        return self._first[word]

    def save(self, lengths: list[int], first: set[str]) -> None:
        write_json(self.path, {"salt": self.salt.hex(), "facts": sorted(self.hashes), "lengths": lengths,
                               "first": sorted(first)})


# ── matching ──────────────────────────────────────────────────────────────


def to_ms(t: int, fraction: str | None) -> int | None:
    """The millisecond, when the text gives one (a fraction of three digits or more, not .000)."""
    if not fraction or len(fraction) < 3 or int(fraction[:3]) == 0:
        return None
    return t * 1000 + int(fraction[:3])


def iso_seconds(m) -> tuple[tuple[int, int | None], ...]:
    y, mo, d, hh, mi, ss = (int(m.group(i)) for i in range(1, 7))
    if not (1 <= mo <= 12 and 1 <= d <= 31 and hh < 24 and mi < 60 and ss < 60):
        return ()
    t = calendar.timegm((y, mo, d, hh, mi, ss))
    fraction, zone = m.group(7), m.group(8)
    if zone is None:
        # No zone written: as UTC, and as this machine's local time.
        local = int(time.mktime((y, mo, d, hh, mi, ss, 0, 0, -1)))
        ts = (t, local) if local != t else (t,)
    elif zone == "Z":
        ts = (t,)
    else:
        sign = -1 if zone[0] == "-" else 1
        digits = zone[1:].replace(":", "")
        ts = (t - sign * (int(digits[:2]) * 3600 + (int(digits[2:4]) * 60 if len(digits) >= 4 else 0)),)
    return tuple((x, to_ms(x, fraction)) for x in ts)


def numeric_seconds(digits: str, fraction: str | None) -> tuple[int, int | None] | None:
    v, n = int(digits), len(digits)
    if n == 10 and 1_000_000_000 <= v < 2_100_000_000:
        return v, to_ms(v, fraction)
    if n == 13:
        return v // 1000, v if v % 1000 else None
    if n == 16:
        return v // 10 ** 6, v // 1000 if v // 1000 % 1000 else None
    if n == 19:
        return v // 10 ** 9, v // 10 ** 6 if v // 10 ** 6 % 1000 else None
    if n == 9 and 300_000_000 <= v < 1_000_000_000:
        t = v + APPLE_EPOCH  # Core Data / CFAbsoluteTime seconds
        return t, to_ms(t, fraction)
    if n == 18:
        ms = v // 10 ** 6 + APPLE_EPOCH * 1000  # Messages' nanoseconds since 2001
        return ms // 1000, ms if ms % 1000 else None
    return None


def timestamps(lines) -> list[tuple[int, tuple[tuple[int, int | None], ...]]]:
    """(line, readings) for each distinct timestamp written in these lines. A
    reading is (UTC second, millisecond or None); a zoneless time has two."""
    seen: set = set()
    out = []
    for no, text in lines:
        found = [iso_seconds(m) for m in ISO_TS.finditer(text)]
        for m in NUM_TS.finditer(text):
            r = numeric_seconds(m.group(1), m.group(2))
            if r is not None:
                found.append((r,))
        for rs in found:
            rs = tuple(r for r in rs if 1_300_000_000 <= r[0] <= 2_000_000_000
                       and (r[1] is not None or distinctive_second(r[0])))
            if rs and rs not in seen:
                seen.add(rs)
                out.append((no, rs))
    return out


def decode_polyline(s: str, precision: int):
    coords, lat, lon, i, factor = [], 0, 0, 0, 10 ** precision
    try:
        while i < len(s):
            vals = []
            for _ in range(2):
                shift = result = 0
                while True:
                    b = ord(s[i]) - 63
                    i += 1
                    result |= (b & 0x1F) << shift
                    shift += 5
                    if b < 0x20:
                        break
                vals.append(~(result >> 1) if result & 1 else result >> 1)
            lat += vals[0]
            lon += vals[1]
            if abs(lat) > 90 * factor or abs(lon) > 180 * factor:
                return []
            coords.append((lat / factor, lon / factor))
    except IndexError:
        pass
    return coords


def coordinates(lines):
    """(line, lat, lon, decimals) candidates: decimal pairs, E7 integer pairs, polylines."""
    nums: list[tuple[float, int, int]] = []
    for no, text in lines:
        for m in NUMBER.finditer(text):
            nums.append((float(m.group(0)), no, len(m.group(1))))
        for m in E7.finditer(text):
            v = int(m.group(0)) / 1e7
            if abs(v) <= 180:
                nums.append((v, no, 7))
        for m in POLYLINE.finditer(text):
            tok = m.group(0)
            if POLY_MARK.search(tok):
                for prec in (5, 6):
                    for lat, lon in decode_polyline(tok, prec):
                        yield no, lat, lon, prec
    for i, (a, no, da) in enumerate(nums):
        for b, _, db in nums[i + 1:i + 4]:
            for lat, lon in ((a, b), (b, a)):
                if abs(lat) <= 90 and abs(lon) <= 180:
                    yield no, lat, lon, min(da, db)


def day_values(lines):
    """(line, date, kind, value) for each number written near a date: the nearest
    date before or after it on its line (within 200 characters), or the last date
    of the three lines above."""
    carried, since = None, 0
    for no, text in lines:
        dates = [(m.start(), m.end(), m.group(0)) for m in DATE.finditer(text)]
        starts = [d[0] for d in dates]
        for m in VALUE.finditer(text):
            i = bisect.bisect_right(starts, m.start())
            near = set()
            if i and dates[i - 1][1] <= m.start() and m.start() - dates[i - 1][1] <= 200:
                near.add(dates[i - 1][2])
            if i < len(dates) and dates[i][0] - m.end() <= 200:
                near.add(dates[i][2])
            if not dates and carried and no - since <= 3:
                near.add(carried)
            if not near or (i and m.start() < dates[i - 1][1]):
                continue
            dollars, digits, cents = m.group(1), m.group(2).replace(",", ""), m.group(3)
            n = int(digits)
            values = []
            if cents is not None:
                values.append(("amount", n * 100 + int(cents)))
            else:
                if n >= 100:
                    values += [("steps", n), ("amount", n)]
                if dollars:
                    values.append(("amount", n * 100))
            for date in near:
                for kind, value in values:
                    yield no, date, kind, value
        if dates:
            carried, since = dates[-1][2], no


def shift_day(date: str, days: int) -> str:
    t = calendar.timegm(time.strptime(date, "%Y-%m-%d")) + days * 86400
    return time.strftime("%Y-%m-%d", time.gmtime(t))


def scan(lines: list[tuple[int, str]], deny: Denylist, path: str, facts_everywhere: bool = False):
    """Yield (line, kind, hash) for every denylisted feature in one file's added lines."""
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
        for i in range(len(t) - 1):
            if deny.may_start_name(t[i]):
                for n in (2, 3, 4):
                    if i + n <= len(t) and (x := h.hex("name", " ".join(t[i:i + n]))) in deny.sets["name"]:
                        yield no, "name", x
        if deny.facts.salt and (facts_everywhere or FACT_SCOPE.search(path)):
            for i in range(len(t)):
                if deny.facts.may_start(t[i]):
                    for n in deny.facts.lengths:
                        if i + n <= len(t) and (x := deny.facts.key(t[i:i + n])) in deny.facts.hashes:
                            yield no, "fact", x

    # Shingles and coordinates run over the added text as one stream, so a
    # sentence wrapped across lines or a lat/lon pair split over two still matches.
    stream: list[tuple[str, int]] = []
    for no, text in lines:
        stream.extend((w, no) for w in WORD.findall(ESCAPE.sub(" ", text).lower()))
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

    # Places and points.
    point_hits: dict[int, int] = {}
    lam = 0.0
    for no, lat, lon, decimals in coordinates(lines):
        if (x := h.hex("cell", cell_key(lat, lon))) in deny.sets["cell"]:
            yield no, "place", x
        if decimals < 4:
            continue
        d4, d5 = deny.block_density(lat, lon)
        if not d4:
            continue  # no real point within this ~111 m block
        # Match at the precision the text carries; a copy is truncated or
        # rounded at its last decimal, so try the cell below as well.
        per, kind, density = (POINT_FINE, "point5", d5) if decimals >= 6 else (POINT_CELL, "point", d4)
        tried = {(a, b) for a in (fine(lat, per), fine(lat, per) - 1) for b in (fine(lon, per), fine(lon, per) - 1)}
        lam += len(tried) * density
        for a, b in tried:
            if (v := h.int(kind, f"{a},{b}")) in deny.points:
                point_hits.setdefault(v, no)
    if stands_out(len(point_hits), lam, POINT_MIN):
        for v, no in point_hits.items():
            yield no, "point", f"{v:016x}"

    # Timestamps, as written.
    stamps = timestamps(lines)
    time_hits: dict[int, int] = {}
    lam = 0.0
    for no, readings in stamps:
        for t, ms in readings:
            # A millisecond is matched as a millisecond (± the last digit's
            # rounding), a whole second as a second, and chance counted to match.
            if ms is None:
                lam += deny.hour_density(t)
                tried = [deny.time_hash(t)]
            else:
                lam += 2 * deny.hour_density(t) / 1000
                tried = [h.int("time_ms", str(ms)), h.int("time_ms", str(ms - 1))]
            hit = next((v for v in tried if v in deny.times), None)
            if hit is not None:
                time_hits.setdefault(hit, no)
                break
    if stands_out(len(time_hits), lam, TIME_MIN):
        for v, no in time_hits.items():
            yield no, "timestamp", f"{v:016x}"

    # Timestamps shifted by whole days: a sample of the file against every shift.
    if len(stamps) >= 10:
        step = max(1, len(stamps) // SHIFT_SAMPLE)
        sample = [rs[0][0] for _, rs in stamps[::step]][:SHIFT_SAMPLE]
        lines_of = {rs[0][0]: no for no, rs in stamps}
        by_shift: dict[int, list[int]] = defaultdict(list)
        for t in sample:
            for k in range(-SHIFT_DAYS, SHIFT_DAYS + 1):
                if k and deny.time_hash(t + k * 86400) in deny.times:
                    by_shift[k].append(t)
        for k, ts in by_shift.items():
            if len(ts) < SHIFT_MIN:
                continue
            lam = sum(deny.hour_density(t + k * 86400) for t in sample)
            if stands_out(len(ts), lam, SHIFT_MIN, ALPHA / (2 * SHIFT_DAYS)):
                for t in ts:
                    yield lines_of[t], f"timestamp shifted {k:+d} days", f"{deny.time_hash(t + k * 86400):016x}"

    # (date, value) pairs. Only when enough match is the file read again for
    # chance: the same values against the days around them.
    hits: dict[int, int] = {}
    for no, date, kind, value in day_values(lines):
        if (v := h.int(kind, f"{date}|{value}")) in deny.pairs:
            hits.setdefault(v, no)
    if len(hits) >= PAIR_MIN:
        control = sum(h.int(kind, f"{shift_day(date, d)}|{value}") in deny.pairs
                      for _, date, kind, value in day_values(lines) for d in (-3, -2, -1, 1, 2, 3))
        if stands_out(len(hits), control / 6, PAIR_MIN):
            for v, no in hits.items():
                yield no, "day value (steps or amount)", f"{v:016x}"


# ── check ─────────────────────────────────────────────────────────────────

ALLOW_KIND = {"message text": "shingle", "place": "cell", "point": "point", "timestamp": "time",
              "day value (steps or amount)": "pair"}


def allow_kind(kind: str) -> str:
    return "time" if kind.startswith("timestamp") else ALLOW_KIND.get(kind, kind)


def load_denylist(strict: bool) -> Denylist | None:
    """The denylist, or None to pass. Refuses (exits) where the guard is required and unusable."""
    p = guard_paths()
    required = git_config("virtues.guardRequired") == "true"
    if not p["deny"].exists() or not p["shingles"].exists():
        if strict and required:
            sys.exit(f"\n✖ refused: virtues.guardRequired is set and there is no denylist at {p['deny']}.\n"
                     f"  Build it from the real record: tools/privacy-guard.py build")
        # A contributor's clone has no real database and no denylist; nothing to compare.
        if "VIRTUES_GUARD_DENYLIST" in os.environ or p["deny"].parent.exists():
            print(f"privacy-guard: no denylist at {p['deny']} — run `tools/privacy-guard.py build`",
                  file=sys.stderr)
        return None
    deny = Denylist(p)
    built = deny.meta.get("built_at") or int(p["deny"].stat().st_mtime)
    age = (time.time() - built) / 86400
    if age > MAX_AGE_DAYS:
        msg = (f"the denylist is {age:.0f} days old (limit {MAX_AGE_DAYS}); it cannot see anything the "
               f"record gained since. Rebuild: tools/privacy-guard.py build")
        if strict and required:
            sys.exit(f"\n✖ refused: {msg}")
        print(f"privacy-guard: {msg}", file=sys.stderr)
    return deny


def report(hits: list[dict], what: str) -> int:
    if not hits:
        return 0
    p = guard_paths()
    p["last"].write_text(json.dumps(hits))
    os.chmod(p["last"], 0o600)
    print(f"\n✖ {what}: it matches the owner's real record.", file=sys.stderr)
    shown: Counter[tuple[str, str]] = Counter()
    for hit in hits:
        key = (hit["file"], hit["kind"])
        shown[key] += 1
        if shown[key] <= 5:
            print(f"    {hit['file']}:{hit['line']}  {hit['kind']}", file=sys.stderr)
    for (f, kind), n in shown.items():
        if n > 5:
            print(f"    {f}  … {n - 5} more {kind}", file=sys.stderr)
    print("""
  Fixtures and examples quote only invented text; the repo carries no demo data
  (.claude/rules/fixtures.md). Replace the matched content with fictional values
  (+1512555xxxx, @example.com, names like David Okafor). Matches name the kind
  and the line, never the real value. If a hit is genuinely generic, the owner
  can accept it by hash:
      tools/privacy-guard.py allow --last""", file=sys.stderr)
    return 1


def scan_files(files, deny: Denylist, label: str = "") -> list[dict]:
    everywhere = git_config("virtues.factsScope") == "all"
    hits = []
    for path, lines in files:
        for no, kind, x in scan(lines, deny, path, everywhere):
            hits.append({"file": label + path, "line": no, "kind": kind, "hash": x})
    return hits


def diff_files(cp, base: str, target: str | None, only=None):
    for path, data, before, _ in cp.changed_files(base, target, only):
        lines = cp.added_lines(path, data, before)
        if lines:
            yield path, lines


def commit_files(cp, commits: list[str], deny: Denylist) -> list[dict]:
    hits = []
    for c in commits:
        info = subprocess.run(["git", "show", "-s", "--format=%P%x00%B", c],
                              capture_output=True, text=True, errors="replace").stdout
        parents, message = info.split("\0", 1)
        short = c[:9] + " "
        hits += scan_files([("(commit message)", list(enumerate(message.splitlines(), 1)))], deny, short)
        parents = parents.split()
        if len(parents) > 1:
            evil = set(filter(None, subprocess.run(
                ["git", "diff-tree", "--cc", "--name-only", "-z", "--no-commit-id", c],
                capture_output=True, text=True).stdout.split("\0")))
            if evil:
                hits += scan_files(diff_files(cp, parents[0], c, evil), deny, short)
        else:
            hits += scan_files(diff_files(cp, parents[0] if parents else cp.EMPTY_TREE, c), deny, short)
    return hits


def hook_files(payload: dict) -> list[tuple[str, list[tuple[int, str]]]]:
    """What a Write/Edit/MultiEdit/NotebookEdit (Claude) or apply_patch (Codex) would add."""
    tool, inp = payload.get("tool_name", ""), payload.get("tool_input") or {}
    root = payload.get("cwd") or os.getcwd()
    path = inp.get("file_path") or inp.get("notebook_path") or ""
    path = os.path.relpath(path, root) if os.path.isabs(path) else path
    if tool == "Write":
        texts = [inp.get("content", "")]
    elif tool == "Edit":
        texts = [inp.get("new_string", "")]
    elif tool == "MultiEdit":
        texts = [e.get("new_string", "") for e in inp.get("edits", [])]
    elif tool == "NotebookEdit":
        texts = [inp.get("new_source", "")]
    elif tool == "apply_patch":
        files, cur = {}, None
        for line in str(inp.get("command", "")).splitlines():
            m = re.match(r"\*\*\* (?:Add|Update) File: (.+)", line)
            if m:
                cur = files.setdefault(m.group(1).strip(), [])
            elif line.startswith("*** "):
                cur = None
            elif cur is not None and line.startswith("+"):
                cur.append((len(cur) + 1, line[1:]))
        return list(files.items())
    else:
        return []
    lines = [ln for t in texts for ln in str(t).splitlines()]
    return [(path, list(enumerate(lines, 1)))]


def cmd_check(args) -> int:
    if args.hook:
        # Write time, in any repo: block on a match, never on a missing or old denylist.
        try:
            payload = json.load(sys.stdin)
        except ValueError:
            return 0
        deny = load_denylist(strict=False)
        if deny is None:
            return 0
        hits = scan_files(hook_files(payload), deny)
        return 2 if report(hits, "write refused") else 0

    deny = load_denylist(strict=True)
    if deny is None:
        return 0
    cp = load_check_pii()
    if args.message:
        text = Path(args.message).read_text(errors="replace")
        lines = [(n, ln) for n, ln in enumerate(text.splitlines(), 1) if not ln.startswith("#")]
        return report(scan_files([("(commit message)", lines)], deny), "commit refused: the message")
    if args.range:
        return report(commit_files(cp, cp.commit_range(args.range), deny), "range refused")
    if args.push:
        hits = []
        for line in sys.stdin.read().splitlines():
            parts = line.split()
            if len(parts) == 4 and parts[1] != cp.ZERO:
                hits += commit_files(cp, cp.commit_range(f"{parts[3]}..{parts[1]}"), deny)
        return report(hits, "push refused: a commit in it")
    if args.stdin:
        lines = list(enumerate(sys.stdin.read().splitlines(), 1))
        return report(scan_files([(args.path or "(stdin)", lines)], deny), "refused")
    return report(scan_files(diff_files(cp, cp.head_or_empty(), None), deny),
                  "commit refused: staged content")


def tree_files(rev: str):
    """Every text file committed at `rev`, .gz decompressed, as numbered lines."""
    cp = load_check_pii()
    raw = subprocess.run(["git", "-c", "core.quotepath=off", "ls-tree", "-r", "-z", rev],
                         capture_output=True, text=True).stdout
    entries = [(rec.split("\t", 1)[1], rec.split()[2]) for rec in filter(None, raw.split("\0"))
               if rec.split()[1] == "blob" and not rec.startswith("120000")]
    for (path, _), data in zip(entries, cp.blobs([sha for _, sha in entries])):
        lines = cp.text_lines(path, data)
        if lines:
            yield path, list(enumerate(lines, 1))


def cmd_audit(args) -> int:
    deny = load_denylist(strict=False)
    if deny is None:
        return 1
    hits = scan_files(tree_files(args.rev), deny)
    by_file: Counter[tuple[str, str]] = Counter((x["file"], x["kind"]) for x in hits)
    for (f, kind), n in sorted(by_file.items()):
        print(f"{f}  {kind} ×{n}")
    print(f"privacy-guard: {len(hits)} hit(s) in {len({f for f, _ in by_file})} file(s) at {args.rev}",
          file=sys.stderr)
    return 1 if hits else 0


# ── facts, allow, status ──────────────────────────────────────────────────


def cmd_fact(args) -> int:
    p = guard_paths()
    p["facts"].parent.mkdir(parents=True, exist_ok=True)
    os.chmod(p["facts"].parent, 0o700)
    facts = Facts(p["facts"])
    facts.salt = facts.salt or secrets.token_bytes(32)
    words = tokens(args.text or args.remove or "")
    if not words:
        print(f"privacy-guard: {len(facts.hashes)} fact(s) held in {p['facts']}")
        return 0
    if len(words) > 6:
        sys.exit("privacy-guard: a fact is a short phrase (six words at most): a name, a place, a condition")
    k = facts.key(words)
    if args.remove:
        facts.hashes.discard(k)
    else:
        facts.hashes.add(k)
    # Lengths and first words are kept for every fact ever added; a stale one
    # only costs a few hashes, and removing it could drop a fact still held.
    facts.save(sorted(set(facts.lengths) | {len(words)}), facts.first | {facts.key(["\0first", words[0]])})
    print(f"privacy-guard: {'removed' if args.remove else 'added'} a {len(words)}-word fact; "
          f"{len(facts.hashes)} held")
    return 0


def cmd_allow(args) -> int:
    p = guard_paths()
    allow = json.loads(p["allow"].read_text()) if p["allow"].exists() else {}
    added = 0

    def accept(kind: str, x: str) -> int:
        k = allow_kind(kind)
        v = int(x, 16) if k in ("shingle", "point", "time", "pair") else x
        if v in allow.setdefault(k, []):
            return 0
        allow[k].append(v)
        return 1

    if args.last:
        for hit in json.loads(p["last"].read_text()):
            added += accept(hit["kind"], hit["hash"])
    if args.tree:
        deny = Denylist(p)
        for path, lines in tree_files(args.tree):
            for no, kind, x in scan(lines, deny, path, git_config("virtues.factsScope") == "all"):
                if accept(kind, x):
                    added += 1
                    print(f"    accepted {path}:{no}  {kind}")
    if args.phrase:
        h = Hasher(bytes.fromhex(json.loads(p["deny"].read_text())["salt"]))
        t = tokens(args.phrase)
        for i in range(len(t) - SHINGLE + 1):
            added += accept("message text", f"{h.shingle(t[i:i + SHINGLE]):016x}")
    write_json(p["allow"], allow)
    print(f"privacy-guard: {added} hash(es) added to {p['allow']}")
    return 0


def cmd_status(args) -> int:
    p = guard_paths()
    required = git_config("virtues.guardRequired") == "true"
    print(f"virtues.guardRequired: {'true' if required else 'false (a missing or old denylist passes)'}")
    if not p["deny"].exists():
        print(f"denylist: none at {p['deny']}")
        return 1 if required else 0
    deny = Denylist(p)
    built = deny.meta.get("built_at") or int(p["deny"].stat().st_mtime)
    age = (time.time() - built) / 86400
    through = deny.meta.get("data_through")
    print(f"denylist: built from {deny.meta.get('built_from')} {age:.1f} days ago"
          + (f"; the newest real sample in it is {(time.time() - through) / 86400:.0f} days old" if through else
             "; built before timestamps, points and pairs were hashed — rebuild"))
    print(f"  {len(deny.sets['phone'])} phones, {len(deny.sets['email'])} emails, {len(deny.sets['name'])} names, "
          f"{len(deny.sets['cell'])} place cells, {len(deny.points.a)} point cells, {len(deny.times.a)} timestamps, "
          f"{len(deny.pairs.a)} day pairs, {len(deny.shingles.a)} shingles, {len(deny.facts.hashes)} facts")
    stale = age > MAX_AGE_DAYS
    if stale:
        print(f"  older than {MAX_AGE_DAYS} days: " + ("check REFUSES until it is rebuilt" if required else "check warns"))
    return 1 if stale and required else 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("build")
    b.add_argument("--db", default="virtues_real")
    c = sub.add_parser("check")
    mode = c.add_mutually_exclusive_group()
    mode.add_argument("--message", metavar="FILE")
    mode.add_argument("--range", metavar="A..B")
    mode.add_argument("--push", action="store_true")
    mode.add_argument("--hook", action="store_true")
    mode.add_argument("--stdin", action="store_true")
    c.add_argument("--path", help="with --stdin: the path the text is for (scopes facts)")
    au = sub.add_parser("audit")
    au.add_argument("rev", nargs="?", default="HEAD")
    f = sub.add_parser("fact")
    f.add_argument("text", nargs="?")
    f.add_argument("--remove", metavar="TEXT")
    a = sub.add_parser("allow")
    a.add_argument("--last", action="store_true")
    a.add_argument("--phrase")
    a.add_argument("--tree", metavar="REV")
    sub.add_parser("status")
    args = ap.parse_args()
    return {"build": cmd_build, "check": cmd_check, "audit": cmd_audit, "fact": cmd_fact,
            "allow": cmd_allow, "status": cmd_status}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
