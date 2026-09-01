#!/usr/bin/env python3
"""Validate every `build_batch_insert_query` column list against the live schema.

These inserts are built from string arrays at runtime, so `sqlx` cannot check
them and the compiler cannot see them. That is not hypothetical: the 2026-08-17
column renames left `"timestamp"` and `"start_time"` in the column arrays of
seven ingest paths — heart rate, HRV, steps, active energy, distance, calendar
(Google and EventKit), Strava — and every one would have failed at the first
insert, on a box, with the applet reporting an error nobody reads.

Usage:  DATABASE_URL=postgres://... python3 tools/check-dynamic-inserts.py
Exit 1 if any named column does not exist on its table.
"""
import os, re, subprocess, sys, pathlib

CALL = re.compile(
    r'build_batch_(?:insert|upsert)_query\(\s*"(?P<table>[a-z_]+)"\s*,\s*&\[(?P<cols>.*?)\]',
    re.S,
)

def columns_of(table, url):
    out = subprocess.run(
        ["psql", url, "-tAc",
         f"select column_name from information_schema.columns where table_name='{table}'"],
        capture_output=True, text=True)
    return {c.strip() for c in out.stdout.split() if c.strip()}

def defaulted_of(table, url):
    """Columns the DDL itself supplies (DEFAULT or generated) — born written."""
    out = subprocess.run(
        ["psql", url, "-tAc",
         f"select column_name from information_schema.columns where table_name='{table}' "
         "and (column_default is not null or is_generated='ALWAYS')"],
        capture_output=True, text=True)
    return {c.strip() for c in out.stdout.split() if c.strip()}

# ── Phase 2: the model-facing catalog must not advertise phantom columns ──
#
# `sql_query.rs`'s `key_columns` are what the agent is TOLD it can query. The
# audit of 2026-08-28 found eight advertised columns that no writer had ever
# populated (`ref_count`, `url`, `event_type`, `credit_limit`, …): the model
# was being steered into queries that select real-but-forever-NULL columns —
# or, in the ref_count case, columns that do not exist at all. This phase
# checks every catalog column (a) exists and (b) is named by SOME writer:
# a `build_batch_insert_query` array, an `INSERT INTO` column list (Rust or
# seeds/*.sql — raw-SQL seeds are writers too; they hid five columns from the
# audit), or an `UPDATE … SET` assignment.

METADATA = re.compile(
    r'm\.insert\("(?P<table>[a-z_]+)",\s*TableMetadata\s*\{.*?'
    r'key_columns:\s*&\[(?P<cols>.*?)\]',
    re.S,
)
INSERT = re.compile(r'INSERT INTO\s+(?P<table>[a-z_]+)[\s\\]*\((?P<cols>[^)]*)\)', re.S | re.I)
UPDATE_HEAD = re.compile(r'UPDATE\s+(?P<table>[a-z_]+)\b', re.I)
ASSIGN = re.compile(r'([a-z_]+)\s*=')
# build_batch_insert_query with either an inline &[..] or a named array var.
BATCH = re.compile(
    r'build_batch_(?:insert|upsert)_query\(\s*"(?P<table>[a-z_]+)"\s*,\s*&(?:\[(?P<inline>.*?)\]|(?P<var>[a-z_]+))',
    re.S,
)

def update_assignments(txt):
    """Yield (table, [cols]) per UPDATE, with a bounded window so one
    unterminated statement cannot swallow the rest of the file."""
    for m in UPDATE_HEAD.finditer(txt):
        window = txt[m.end():m.end() + 2500]
        set_at = re.search(r'\bSET\b', window[:300])
        if not set_at:
            continue
        body = window[set_at.end():]
        stop = re.search(r'\bWHERE\b|\bRETURNING\b|"#|;', body)
        if stop:
            body = body[:stop.start()]
        yield m.group("table"), ASSIGN.findall(body)

# Columns the DDL itself supplies (DEFAULT / trigger / view projection) —
# legitimately advertised without appearing in any writer's column list.
# Every entry needs a reason.
WRITTEN_BY_DDL = {
    # wiki_day_prose is a VIEW; its columns are projections, not writes.
    ("wiki_day_prose", "day_id"), ("wiki_day_prose", "date"), ("wiki_day_prose", "prose"),
}

def catalog(root):
    src = (root / "virtues-core/src/tools/sql_query.rs").read_text()
    out = {}
    for m in METADATA.finditer(src):
        out[m.group("table")] = re.findall(r'"([a-z_]+)"', m.group("cols"))
    return out

def check_catalog(url, written, cache, bad):
    root = pathlib.Path(".")
    entries = catalog(root)
    if not entries:
        bad.append("catalog: parsed ZERO TableMetadata entries — the regex no longer matches sql_query.rs")
        return 0
    n = 0
    for table, cols in entries.items():
        if table not in cache:
            cache[table] = columns_of(table, url)
        real = cache[table]
        if not real:
            bad.append(f"catalog: table {table!r} does not exist")
            continue
        defaulted = defaulted_of(table, url)
        n += 1
        for c in cols:
            if c not in real:
                bad.append(f"catalog: {table}.{c} advertised but does not exist")
            elif (c not in written.get(table, set())
                  and c not in defaulted
                  and (table, c) not in WRITTEN_BY_DDL):
                bad.append(f"catalog: {table}.{c} advertised but NO writer names it")
    return n

def main():
    url = os.environ.get("DATABASE_URL")
    if not url:
        print("DATABASE_URL not set", file=sys.stderr)
        return 2
    bad, checked = [], 0
    cache = {}
    written = {}  # table -> set of columns some writer names
    def note(table, cols):
        written.setdefault(table, set()).update(cols)
    sources = [f for f in pathlib.Path(".").rglob("*.rs")
               if "/target/" not in str(f) and "/migrations/" not in str(f)]
    seeds = list(pathlib.Path("virtues-core/seeds").glob("*.sql"))
    for f in sources + seeds:
        txt = f.read_text(errors="ignore")
        # Ignore test modules: their fixtures use invented table names, and a
        # fixture that does not match the schema is not a defect.
        if f.suffix == ".rs":
            cut = txt.find("#[cfg(test)]")
            if cut != -1:
                txt = txt[:cut]
        for m in INSERT.finditer(txt):
            note(m.group("table"), re.findall(r"[a-z_]+", m.group("cols")))
        for table, assigns in update_assignments(txt):
            note(table, assigns)
        if f.suffix != ".rs":
            continue
        # Writer columns via build_batch_insert_query — inline arrays AND
        # named `let cols = [...]` arrays (the weather_sync shape).
        for m in BATCH.finditer(txt):
            if m.group("inline") is not None:
                note(m.group("table"), re.findall(r'"([a-z_]+)"', m.group("inline")))
            else:
                var = m.group("var")
                decl = re.search(r"let\s+" + var + r"[^=;]*=\s*&?\[([^;]*?)\]\s*;", txt, re.S)
                if decl:
                    note(m.group("table"), re.findall(r'"([a-z_]+)"', decl.group(1)))
        for m in CALL.finditer(txt):
            table = m.group("table")
            cols = re.findall(r'"([a-z_]+)"', m.group("cols"))
            if not cols:
                continue
            note(table, cols)
            if table not in cache:
                cache[table] = columns_of(table, url)
            real = cache[table]
            if not real:
                bad.append(f"{f}: table {table!r} does not exist")
                continue
            checked += 1
            for c in cols:
                if c not in real:
                    bad.append(f"{f}: {table}.{c} does not exist")
    cataloged = check_catalog(url, written, cache, bad)
    if bad:
        print("✖  schema drift:")
        for b in bad:
            print(f"     {b}")
        return 1
    print(f"✓  {checked} dynamic INSERT list(s) + {cataloged} catalog entries all match the schema and have writers")
    return 0

sys.exit(main())
