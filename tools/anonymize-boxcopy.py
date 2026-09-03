#!/usr/bin/env python3
"""Make a box snapshot safe to photograph.

`make dev-pull` copies a real server's Postgres into `virtues_boxcopy` so you can
browse your own record locally. That copy is somebody's actual life, and the
moment you screenshot it for the README, the repo rule applies: no real names,
addresses, employers, account numbers or coordinates, ever, in a public repo.

This script rewrites the copy in place so the UI still renders a coherent day —
same shape, same rhythm, same event count — with nobody's real details in it.

    python3 tools/anonymize-boxcopy.py            # scrub, keep a shot window
    python3 tools/anonymize-boxcopy.py --window 2026-09-02:2026-09-02

WHAT IT DOES

  1. Assigns every person, place and organization a fictional identity, keyed on
     row id so the mapping is stable across runs of the same snapshot. Real names
     are read at runtime and never written anywhere but this database.
  2. Rewrites the entity tables themselves: names, aliases, nicknames, handles,
     emails (@example.com), phones (+1512555xxxx), addresses, social handles,
     notes and generated articles.
  3. Shifts every coordinate by one constant delta, chosen so the centroid of
     your movement lands on a fixed decoy city. Relative geometry — the shape of
     a day's travel on the map — is preserved exactly; the absolute position is
     wrong by hundreds of miles.
  4. Substitutes those names through the prose that renders: day prose, event
     labels and summaries, topics, entity lists, articles, chat titles.
  5. Neutralizes bulk message/email/transcription bodies outside the shot
     window, because those carry private content that no name substitution can
     make safe. Inside the window they get the name substitution only, so a
     person page still shows plausible mentions.

WHAT IT DOES NOT DO

  It cannot read prose for meaning. A diary entry naming an employer, a medical
  detail or an unusual address will survive step 4 if that string is not an
  entity in the graph. READ EVERY FRAME BEFORE IT LEAVES THE MACHINE — this
  script makes the copy safe to look at, not automatically safe to publish.
"""

import argparse
import json
import os
import re
import subprocess
import sys

# The reserved fictional block this repo already uses. Deliberately bland and
# obviously invented; never a real person's name.
FIRST = [
    "Nick", "David", "Mara", "Tomas", "Priya", "Ines", "Oscar", "Lena", "Rafi",
    "Cora", "Emil", "Nadia", "Julien", "Bea", "Hugo", "Sana", "Marco", "Elin",
    "Theo", "Rosa", "Anton", "Vera", "Sami", "Ada", "Bruno", "Nell", "Kwame",
    "Iris", "Otto", "Sylvie", "Dmitri", "Faye", "Milo", "Zara", "Piet", "Noor",
]
LAST = [
    "Okafor", "Vance", "Lindqvist", "Moreau", "Sandoval", "Bergen", "Achebe",
    "Kovac", "Delgado", "Fenwick", "Aalto", "Rossi", "Nakamura", "Bauer",
    "Silva", "Novak", "Haddad", "Lindgren", "Barros", "Weiss",
]
PLACE_ADJ = ["Ash", "Fern", "Kestrel", "Marlow", "Alder", "Bramble", "Quill",
             "Harrow", "Selden", "Thorne", "Vesper", "Wren"]
PLACE_NOUN = ["Court", "Commons", "Yard", "Market", "Rooms", "Terrace", "Works",
              "Arcade", "Green", "Wharf", "Exchange", "Hall"]
ORG_A = ["Northwind", "Meridian", "Halcyon", "Ironwood", "Lantern", "Ostrich",
         "Pemberton", "Quiet River", "Saltgrass", "Tandem", "Umber", "Vantage"]
ORG_B = ["Labs", "Collective", "Works", "Partners", "Society", "Trading Co.",
         "Foundry", "Institute", "Group", "Union", "Press", "Studio"]

# The decoy centroid every coordinate is translated onto: downtown Chicago.
DECOY_LAT, DECOY_LON = 41.8781, -87.6298

# Free-text columns that render as prose. Name substitution runs over these.
KEYS = {}                                    # tables not keyed on `id`

PROSE_COLUMNS = [
    # Day prose lives in app_pages.content, reachable through wiki_articles;
    # `wiki_day_prose` is a view over that join and is not updatable.
    ("app_pages", "content"),
    ("app_pages", "title"),
    ("wiki_events", "auto_label"),
    ("wiki_events", "auto_location"),
    ("wiki_events", "user_label"),
    ("wiki_events", "user_location"),
    ("wiki_events", "user_notes"),
    ("wiki_events", "event_summary"),
    ("wiki_days", "epigraph"),
    ("wiki_people", "notes"),
]
# JSON columns holding name-ish arrays.
PROSE_JSON = [("wiki_events", "topics"), ("wiki_events", "entities")]

# Bulk tables: private content beyond names. Bodies outside the shot window are
# replaced wholesale rather than substituted.
BULK = [
    ("data_communication_message", "body", "occurred_at"),
    ("data_communication_email", "body", "occurred_at"),
    ("data_communication_email", "subject", "occurred_at"),
    ("data_communication_transcription", "text", "started_at"),
    ("data_communication_transcription", "summary", "started_at"),
    ("data_communication_transcription", "title", "started_at"),
    ("data_activity_web_browsing", "url", "occurred_at"),
]

PLACEHOLDER = "(content removed for the demo copy)"


def psql(db, sql, *, tuples=True):
    pg = subprocess.run(["brew", "--prefix", "postgresql@18"],
                        capture_output=True, text=True).stdout.strip()
    cmd = [os.path.join(pg, "bin", "psql"), "-d", db, "-v", "ON_ERROR_STOP=1"]
    # Explicit field AND record separators: prose columns contain newlines, and
    # psql's default line-per-row output silently splits them into fake rows.
    cmd += ["-tA", "-F", "\x1f", "-R", "\x1e", "-c", sql] if tuples else ["-c", sql]
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode:
        # NEVER echo the statement: an UPDATE carries the row's content, and the
        # whole point of this script is that the content is somebody's life.
        # The first line, truncated at the first literal, is enough to locate it.
        head = sql.strip().split("\n")[0].split("=")[0][:120]
        raise SystemExit(f"psql failed:\n{r.stderr.strip()}\n--- while running: {head}…")
    if not tuples:
        return r.stdout
    return [rec.split("\x1f") for rec in r.stdout.split("\x1e") if rec.strip()]


def q(s):
    """Quote a Python string as a SQL literal."""
    return "'" + s.replace("'", "''") + "'"


def jarr(*values):
    """A jsonb array literal — these columns are jsonb, not text[]."""
    return q(json.dumps(list(values))) + "::jsonb"


def pseudonym(kind, i):
    if kind == "person":
        return f"{FIRST[i % len(FIRST)]} {LAST[(i // len(FIRST)) % len(LAST)]}"
    if kind == "place":
        return f"{PLACE_ADJ[i % len(PLACE_ADJ)]} {PLACE_NOUN[(i // len(PLACE_ADJ)) % len(PLACE_NOUN)]}"
    return f"{ORG_A[i % len(ORG_A)]} {ORG_B[(i // len(ORG_A)) % len(ORG_B)]}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--database", default="virtues_boxcopy")
    ap.add_argument("--window", default="2026-08-25:2026-09-03",
                    help="START:END dates whose bulk text keeps name-substituted "
                         "content instead of being blanked")
    ap.add_argument("--force", action="store_true",
                    help="allow a database whose name does not contain 'boxcopy'")
    a = ap.parse_args()

    # The guard that matters: this is a destructive rewrite, and pointing it at
    # the wrong database would edit a real record rather than a copy of one.
    if "boxcopy" not in a.database and not a.force:
        sys.exit(f"refusing to rewrite '{a.database}' — this script is for a "
                 f"throwaway snapshot (name must contain 'boxcopy'), and it "
                 f"destroys data in place. Use --force only if you are certain.")
    db = a.database
    start, end = a.window.split(":")

    # ── 1. Mappings ──────────────────────────────────────────────────────
    # Written into the copy on the first run and reused afterwards. The entity
    # rewrite destroys the real names it was derived from, so a re-run that
    # re-derived them would map fiction onto fiction and leave every real name
    # sitting in the prose with nothing to match it against.
    psql(db, "create table if not exists anon_map (kind text, id text primary key, "
             "real_name text, fake_name text)", tuples=False)
    have = psql(db, "select count(*) from anon_map")[0][0]
    maps = {}
    if int(have):
        print(f"  reusing the stored mapping ({have} entities)")
        for kind in ("person", "place", "org"):
            rows = psql(db, f"select id, real_name, fake_name from anon_map "
                            f"where kind={q(kind)} order by id")
            maps[kind] = {r[0]: (r[1], r[2]) for r in rows}
    else:
        ins = []
        for kind, table in [("person", "wiki_people"), ("place", "wiki_places"),
                            ("org", "wiki_orgs")]:
            rows = psql(db, f"select id, coalesce(name,'') from {table} order by id")
            maps[kind] = {rid: (real, pseudonym(kind, i))
                          for i, (rid, real) in enumerate(rows)}
            for rid, (real, fake) in maps[kind].items():
                ins.append(f"insert into anon_map values ({q(kind)}, {q(rid)}, "
                           f"{q(real)}, {q(fake)})")
            print(f"  {table}: {len(rows)} mapped")
        for i in range(0, len(ins), 200):
            psql(db, ";\n".join(ins[i:i + 200]), tuples=False)

    # Longest-first so "Nick Fenwick" is replaced before a bare "Nick", plus
    # first names on their own, which is how prose actually refers to people.
    # Entity graphs contain rows named after ordinary words — "The", "Mat",
    # "Lee", "Ted" — and a substring match turns "sleep" into "sAntonp" and
    # "noted" into "noSylvie Rossi". Word boundaries plus a stoplist plus a
    # four-character floor are what keep the prose readable.
    STOP = set("""the and but for you your they them this that with from into
        over under about after before here there when what which who whom how
        all any some none one two three not now new old are was were been being
        has have had did does done can may might will just like more most other
        than then their our its his her him she out off own too very
        mat mate lee ted tim ann kim jan may june july august don rob bill mark
        art may will sky rose grace hope faith joy""".split())

    subs = {}
    for kind, m in maps.items():
        for real, fake in m.values():
            if len(real) < 4 or real.lower() in STOP:
                continue
            subs[real] = fake
            if kind == "person" and " " in real:
                rf, ff = real.split()[0], fake.split()[0]
                if len(rf) >= 4 and rf.lower() not in STOP:
                    subs.setdefault(rf, ff)
    ordered = sorted(subs.items(), key=lambda kv: -len(kv[0]))
    # (?<!\w) / (?!\w) rather than \b: names can end in punctuation, and \b
    # after a non-word character would not anchor the way it reads.
    pattern = re.compile(
        "(?<!\\w)(?:" + "|".join(re.escape(k) for k, _ in ordered) + ")(?!\\w)",
        re.IGNORECASE)
    lookup = {k.lower(): v for k, v in ordered}

    def scrub(text):
        return pattern.sub(lambda m: lookup[m.group(0).lower()], text)

    # ── 2. Entity tables ─────────────────────────────────────────────────
    stmts = []
    for rid, (real, fake) in maps["person"].items():
        first, last = fake.split()
        email = jarr(f"{first.lower()}.{last.lower()}@example.com")
        phone = jarr(f"+1512555{(abs(hash(rid)) % 9000) + 1000}")
        alias = jarr(first)
        stmts.append(
            f"update wiki_people set name={q(fake)}, nickname={q(first)}, "
            f"emails={email}, phones={phone}, aliases={alias}, "
            f"handles='[]'::jsonb, metadata='{{}}'::jsonb, content=null, "
            f"instagram=null, facebook=null, linkedin=null, x=null, "
            f"birthday=null, picture=null where id={q(rid)}")
    for rid, (real, fake) in maps["place"].items():
        stmts.append(
            f"update wiki_places set name={q(fake)}, "
            f"address={q(fake + ', Chicago, IL')}, google_place_id=null, "
            f"metadata='{{}}'::jsonb, content=null, aliases={jarr(fake)} "
            f"where id={q(rid)}")
    for rid, (real, fake) in maps["org"].items():
        stmts.append(
            f"update wiki_orgs set name={q(fake)}, metadata='{{}}'::jsonb, "
            f"content=null, aliases={jarr(fake)} where id={q(rid)}")
    print(f"  rewriting {len(stmts)} entity rows")
    for i in range(0, len(stmts), 200):
        psql(db, ";\n".join(stmts[i:i + 200]), tuples=False)

    # ── 3. Coordinates: one constant delta onto a decoy centroid ─────────
    c = psql(db, "select round(avg(latitude)::numeric,6), round(avg(longitude)::numeric,6) "
                 "from data_location_point where latitude is not null")
    if c and c[0][0]:
        dlat = DECOY_LAT - float(c[0][0])
        dlon = DECOY_LON - float(c[0][1])
        for table in ["data_location_point", "data_location_visit", "wiki_places"]:
            cols = psql(db, "select column_name from information_schema.columns "
                            f"where table_name='{table}' and column_name in ('latitude','longitude')")
            if len(cols) == 2:
                psql(db, f"update {table} set latitude=latitude+({dlat}), "
                         f"longitude=longitude+({dlon}) where latitude is not null",
                     tuples=False)
        print(f"  coordinates shifted onto the decoy centroid")

    # ── 4. Prose substitution ────────────────────────────────────────────
    for table, col in PROSE_COLUMNS:
        key = KEYS.get(table, "id")
        rows = psql(db, f"select {key}, {col} from {table} where {col} is not null and {col} <> ''")
        ups = []
        for rid, text in rows:
            new = scrub(text)
            if new != text:
                ups.append(f"update {table} set {col}={q(new)} where {key}={q(rid)}")
        for i in range(0, len(ups), 100):
            psql(db, ";\n".join(ups[i:i + 100]), tuples=False)
        if ups:
            print(f"  {table}.{col}: {len(ups)} rows rewritten")
    for table, col in PROSE_JSON:
        rows = psql(db, f"select id, {col}::text from {table} where {col} is not null")
        ups = []
        for rid, text in rows:
            new = scrub(text)
            if new != text:
                ups.append(f"update {table} set {col}={q(new)}::jsonb where id={q(rid)}")
        for i in range(0, len(ups), 100):
            psql(db, ";\n".join(ups[i:i + 100]), tuples=False)
        if ups:
            print(f"  {table}.{col}: {len(ups)} rows rewritten")

    # User-written pages: arbitrary private writing, not a graph of entities,
    # so substitution cannot make them safe. Blank them.
    psql(db, f"update app_pages set content={q(PLACEHOLDER)}, title='Untitled page' "
             f"where kind is distinct from 'article'", tuples=False)
    print("  app_pages: user-written pages blanked")

    # ── 6. Bulk bodies ───────────────────────────────────────────────────
    for table, col, tcol in BULK:
        exists = psql(db, "select 1 from information_schema.columns where "
                          f"table_name='{table}' and column_name='{col}'")
        if not exists:
            continue
        psql(db, f"update {table} set {col}={q(PLACEHOLDER)} where {col} is not null "
                 f"and ({tcol} < '{start}'::date or {tcol} >= '{end}'::date + 1)",
             tuples=False)
        rows = psql(db, f"select id, {col} from {table} where {col} is not null "
                        f"and {tcol} >= '{start}'::date and {tcol} < '{end}'::date + 1")
        ups = []
        for rid, text in rows:
            new = scrub(text)
            if new != text:
                ups.append(f"update {table} set {col}={q(new)} where id={q(rid)}")
        for i in range(0, len(ups), 100):
            psql(db, ";\n".join(ups[i:i + 100]), tuples=False)
        print(f"  {table}.{col}: blanked outside the window, {len(ups)} rewritten inside")

    print("\nDone. Now READ EVERY FRAME before it leaves this machine — prose can "
          "name an employer or a street that was never an entity in the graph.")


if __name__ == "__main__":
    main()
