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
    r'build_batch_insert_query\(\s*"(?P<table>[a-z_]+)"\s*,\s*&\[(?P<cols>.*?)\]',
    re.S,
)

def columns_of(table, url):
    out = subprocess.run(
        ["psql", url, "-tAc",
         f"select column_name from information_schema.columns where table_name='{table}'"],
        capture_output=True, text=True)
    return {c.strip() for c in out.stdout.split() if c.strip()}

def main():
    url = os.environ.get("DATABASE_URL")
    if not url:
        print("DATABASE_URL not set", file=sys.stderr)
        return 2
    bad, checked = [], 0
    cache = {}
    for f in list(pathlib.Path(".").rglob("*.rs")):
        if "/target/" in str(f) or "/migrations/" in str(f):
            continue
        txt = f.read_text(errors="ignore")
        # Ignore test modules: their fixtures use invented table names, and a
        # fixture that does not match the schema is not a defect.
        cut = txt.find("#[cfg(test)]")
        if cut != -1:
            txt = txt[:cut]
        for m in CALL.finditer(txt):
            table = m.group("table")
            cols = re.findall(r'"([a-z_]+)"', m.group("cols"))
            if not cols:
                continue
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
    if bad:
        print("✖  dynamic INSERT columns that do not exist:")
        for b in bad:
            print(f"     {b}")
        return 1
    print(f"✓  {checked} dynamic INSERT column list(s) all match the schema")
    return 0

sys.exit(main())
