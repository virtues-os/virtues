#!/usr/bin/env python3
"""Generate the multi-year demo seed.

Why this exists
---------------
`demo_narrative.sql` is 80 days of *conclusions* — 715 `wiki_events` and
nothing underneath them. The product's whole claim is that a day is rebuilt
from evidence and that a plan with no trace becomes an honest `unknown`, so a
seed with no raw streams demonstrates the claim without the proof: every
provenance surface (the dayline, sources, the deck's lanes, the movement map,
every chart) is empty on 79 of those 80 days. And at nine weeks, Chapters,
Years, Stories and Lifeline have nothing to draw at all.

This emits three years, bottom-up: raw streams first, the derived layer on top
of them, and the creation layer (chats, pages, articles) that nothing seeded
before. Output is deterministic — same seed, same SQL — so regenerating after a
schema change is a re-run rather than a rewrite.

Deliberately NOT wired into `demo_seed.rs`. That file `include_str!`s its seeds
into the binary; these total tens of MB and belong on the demo box as files,
run by `seeds/demo3y/run.sh`.

Usage:  python3 tools/gen-demo-seed.py [--out virtues-core/seeds/demo3y]
"""

from __future__ import annotations

import argparse
import json
import math
import random
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

# --------------------------------------------------------------------------
# Span. Absolute dates, as the seeds README requires — `run.sh` ends with a
# re-anchor that walks the whole life forward onto today.
#
# ANCHOR_END is the day before `demo_day.sql`'s instrumented Friday, so the two
# sets abut rather than fight: this file covers the three years leading up to
# it, and that day stays the richest single day in the database.
# --------------------------------------------------------------------------
ANCHOR_END = date(2026, 2, 12)
DAYS = 1095  # three years
START = ANCHOR_END - timedelta(days=DAYS - 1)
TZ = "America/Chicago"

# Austin. Home base and the places a life actually repeats through.
LAT0, LON0 = 30.2849, -97.7341

RNG = random.Random(20260916)

# --------------------------------------------------------------------------
# SQL emission
# --------------------------------------------------------------------------


def q(v) -> str:
    """One SQL literal. Every string is escaped; None becomes NULL."""
    if v is None:
        return "NULL"
    if isinstance(v, bool):
        return "TRUE" if v else "FALSE"
    if isinstance(v, (int, float)):
        return repr(v)
    if isinstance(v, (dict, list)):
        return "'" + json.dumps(v, separators=(",", ":")).replace("'", "''") + "'"
    if isinstance(v, datetime):
        return "'" + v.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ") + "'"
    if isinstance(v, date):
        return "'" + v.isoformat() + "'"
    return "'" + str(v).replace("'", "''") + "'"


class Table:
    """Buffers rows for one table and writes them as chunked multi-row INSERTs.

    Chunked because a single INSERT with 40k tuples is a statement Postgres
    will parse but nobody can read, diff, or bisect when one row is malformed.
    """

    CHUNK = 250

    def __init__(self, name: str, cols: list[str]):
        self.name = name
        self.cols = cols
        self.rows: list[tuple] = []

    def add(self, *vals):
        assert len(vals) == len(self.cols), f"{self.name}: {len(vals)} != {len(self.cols)}"
        self.rows.append(vals)

    def sql(self) -> str:
        if not self.rows:
            return ""
        head = f"INSERT INTO {self.name} ({', '.join(self.cols)}) VALUES\n"
        out = []
        for i in range(0, len(self.rows), self.CHUNK):
            chunk = self.rows[i : i + self.CHUNK]
            body = ",\n".join("  (" + ", ".join(q(v) for v in r) + ")" for r in chunk)
            out.append(head + body + "\nON CONFLICT DO NOTHING;\n")
        return "\n".join(out)


def ts(d: date, hh: int, mm: int = 0) -> datetime:
    """Local wall time in Austin → UTC. CST is -6, CDT -5; the DST boundary is
    approximated by month because a demo seed does not need tzdata, and an hour
    of drift in March and November is invisible in every surface that reads it.
    """
    offset = 5 if 3 < d.month < 11 else 6
    return datetime(d.year, d.month, d.day, hh, mm, tzinfo=timezone.utc) + timedelta(
        hours=offset
    )


# --------------------------------------------------------------------------
# The cast. Fictional throughout — `@example.com`, `+1512555xxxx`, per the
# repo's rule. The six people and eight places `demo_day.sql` already defines
# keep their ids so the instrumented day joins onto this history instead of
# standing beside a second, identically-named cast.
# --------------------------------------------------------------------------

PEOPLE = [
    # (id, name, relationship, bond, first_day_offset, last_day_offset, weight, notes)
    ("person_demo_maya", "Mara Vance", "colleague", "close", 0, DAYS, 9,
     "Design lead at Canopy, then the person who kept calling after both of us left. Lunch at Tatsu-ya most weeks."),
    ("person_demo_david", "David Okafor", "friend", "close", 0, DAYS, 7,
     "Known since Chicago. The one who drove the U-Haul down."),
    ("person_demo_rachel", "Cora Delgado", "professional", "weak", 240, DAYS, 3,
     "Realtor. Found the Selden St house."),
    ("person_demo_jess", "Nell Kovac", "friend", "close", 0, DAYS, 6,
     "Runs the Thursday table. Hosts more than anyone should have to."),
    ("person_demo_priya", "Priya Haddad", "colleague", "moderate", 430, DAYS, 5,
     "Contract client, then a collaborator. Ships faster than anyone."),
    ("person_demo_mom", "Rosa", "family", "close", 0, DAYS, 8, "Mom. Sunday calls."),
    # The Chicago half of the life, which fades after the move — this is the
    # "who have I lost touch with" case, and it has to be a real decay in the
    # data rather than a flag on a row.
    ("p3y_theo", "Theo Brandt", "friend", "close", 0, 300, 9,
     "Chicago. Roommate for two years, then a slow drift after the move."),
    ("p3y_junie", "Junie Ferrar", "friend", "moderate", 0, 355, 6, "Chicago. The Tuesday climbing crew."),
    ("p3y_walt", "Walt Osei", "colleague", "moderate", 0, 268, 5, "Agency creative director in Chicago."),
    ("p3y_marisol", "Marisol Reyes", "colleague", "weak", 0, 268, 4, "Agency. Account side."),
    # Two people who share a first name, on purpose: entity resolution should
    # have to show its confidence somewhere a person can see it.
    ("p3y_sam_ortiz", "Sam Ortiz", "friend", "moderate", 120, DAYS, 5, "Austin. Cycling, mostly."),
    ("p3y_sam_whitlock", "Sam Whitlock", "professional", "weak", 500, DAYS, 3,
     "Accountant. Once a year, plus whenever something goes wrong."),
    ("p3y_bea", "Bea Lindqvist", "partner", "close", 395, DAYS, 10,
     "Met at the Mueller farmers market. The best part of the last two years."),
    ("p3y_hal", "Hal Vitorino", "family", "moderate", 0, DAYS, 4, "Uncle. Calls at odd hours."),
    ("p3y_ines", "Ines Moreau", "friend", "moderate", 610, DAYS, 5, "Austin. Met through Bea."),
    ("p3y_desmond", "Desmond Achebe", "colleague", "moderate", 700, DAYS, 5, "Client turned friend."),
    ("p3y_lucia", "Lucia Fenn", "professional", "weak", 300, DAYS, 2, "Dentist's office. Reminders only."),
    ("p3y_kofi", "Kofi Mensah", "friend", "moderate", 150, DAYS, 4, "Basketball, Sundays, when knees allow."),
    ("p3y_rue", "Rue Sandoval", "colleague", "weak", 820, DAYS, 3, "Design contractor."),
    ("p3y_orla", "Orla Byrne", "friend", "weak", 60, 900, 3, "Chicago, then Denver. Birthdays and not much else."),
    ("p3y_gus", "Gus Pettit", "family", "moderate", 0, DAYS, 3, "Brother-in-law."),
    ("p3y_nadia", "Nadia Saleh", "colleague", "moderate", 880, DAYS, 4, "The Virtues beta thread."),
    ("p3y_emil", "Emil Kovács", "professional", "weak", 940, DAYS, 2, "Hardware supplier in Prague."),
    ("p3y_tam", "Tam Nguyen", "friend", "moderate", 200, DAYS, 4, "Neighbor two doors down."),
    ("p3y_bird", "Birdie Lowell", "family", "close", 0, DAYS, 5, "Grandmother. Letters, then calls."),
    ("p3y_curt", "Curt Halloway", "professional", "weak", 0, 120, 2, "Chicago landlord. Deposit disputes."),
]

PLACES = [
    # (id, name, category, lat_off, lon_off, address)
    ("place_demo_home", "Home", "residence", 0.0, 0.0, "1847 Selden St, Austin, TX"),
    ("place_demo_office", "Office", "work", 0.0121, -0.0187, "E 6th St, Austin, TX"),
    ("place_demo_ramen", "Ramen Tatsu-ya", "restaurant", 0.0049, -0.0093, "S Lamar Blvd, Austin, TX"),
    ("place_demo_jos", "Jo's", "cafe", 0.0033, -0.0051, "S Congress Ave, Austin, TX"),
    ("place_demo_ladybird", "Lady Bird Lake", "outdoors", -0.0062, -0.0044, "Austin, TX"),
    ("place_demo_mueller_trails", "Mueller Trails", "outdoors", 0.0208, 0.0121, "Mueller, Austin, TX"),
    ("p3y_pl_chicago_apt", "Logan Square apartment", "residence", 11.60, 10.13, "Chicago, IL"),
    ("p3y_pl_agency", "The agency", "work", 11.62, 10.10, "West Loop, Chicago, IL"),
    ("p3y_pl_market", "Mueller Farmers Market", "market", 0.0211, 0.0128, "Mueller, Austin, TX"),
    ("p3y_pl_gym", "Eastside Barbell", "gym", 0.0087, 0.0043, "E Cesar Chavez St, Austin, TX"),
    ("p3y_pl_grocery", "H-E-B", "grocery", 0.0071, -0.0032, "S Congress Ave, Austin, TX"),
    ("p3y_pl_library", "Central Library", "civic", 0.0028, -0.0119, "W Cesar Chavez St, Austin, TX"),
    ("p3y_pl_clinic", "Family clinic", "health", 0.0155, -0.0061, "Austin, TX"),
    ("p3y_pl_airport", "AUS", "transit", -0.0803, 0.0442, "Austin-Bergstrom, TX"),
    ("p3y_pl_bea", "Bea's", "residence", 0.0094, 0.0072, "Cherrywood, Austin, TX"),
    ("p3y_pl_nells", "Nell's", "residence", 0.0132, -0.0098, "Hyde Park, Austin, TX"),
    ("p3y_pl_lisbon", "Lisbon", "travel", 8.43, 88.6, "Lisboa, Portugal"),
    ("p3y_pl_dogpark", "Norwood dog park", "outdoors", 0.0043, 0.0087, "Austin, TX"),
]

ORGS = [
    ("org_demo_employer", "Canopy", "company", "employer", "Product designer",
     date(2023, 12, 4), date(2025, 1, 14)),
    ("org_demo_realty", "Delgado Realty", "company", "vendor", None, None, None),
    ("p3y_org_agency", "Field & Marrow", "company", "employer", "Senior designer",
     date(2021, 6, 1), date(2023, 11, 3)),
    ("p3y_org_clinic", "Hyde Park Family Medicine", "clinic", "provider", None, None, None),
    ("p3y_org_gym", "Eastside Barbell", "company", "member", None, date(2024, 2, 1), None),
    ("p3y_org_bank", "Third Coast Credit Union", "bank", "customer", None, None, None),
]

# Chapters must tile the span without overlapping — `wiki_chapters_no_overlap`
# enforces it. The `unknown` kind is here on purpose: the years someone
# declines to name are still part of the shape, and Lifeline has to draw one.
CHAPTERS = [
    ("p3y_ch_chicago", "chapter", "The Chicago years", date(2021, 6, 1), date(2023, 11, 5),
     "month", "day", "A job that was good enough for long enough."),
    ("p3y_ch_move", "chapter", "The move", date(2023, 11, 6), date(2024, 3, 31),
     "day", "month", "Five months of boxes, a new city, and no routine to speak of."),
    ("p3y_ch_canopy", "chapter", "Canopy", date(2024, 4, 1), date(2025, 1, 14),
     "month", "day", "Steady work, a house, and the slow end of the Chicago phone book."),
    ("p3y_ch_unknown", "unknown", None, date(2025, 1, 15), date(2025, 3, 2),
     "day", "day", None),
    ("p3y_ch_building", "chapter", "Building the thing", date(2025, 3, 3), None,
     "day", None, "Independent. Longer days, fewer meetings, better ones."),
]

STORIES = [
    ("p3y_st_move", "Leaving Chicago", date(2023, 9, 1), date(2024, 1, 31), "month", "month",
     "The decision took a summer and the move took a weekend."),
    ("p3y_st_house", "The Selden St house", date(2024, 6, 3), date(2024, 8, 19), "day", "day",
     "Six weeks of showings with Cora, one bad inspection, and a closing that nearly didn't."),
    ("p3y_st_bea", "Meeting Bea", date(2024, 3, 16), None, "day", None,
     "A Saturday market, a conversation about tomatoes that ran ninety minutes."),
    ("p3y_st_migraines", "The migraine year", date(2024, 9, 1), date(2025, 6, 30), "month", "month",
     "Eleven months of working out what set them off. Mostly sleep, partly light, not stress."),
    ("p3y_st_lisbon", "Lisbon", date(2025, 5, 9), date(2025, 5, 19), "day", "day",
     "Eleven days, one conference, and the first real time off in two years."),
    ("p3y_st_leaving", "Going independent", date(2025, 1, 15), date(2025, 4, 30), "day", "month",
     "Quitting without a plan, then finding one."),
]

YEARS = [
    (2023, "Chicago, and the decision to leave"),
    (2024, "Austin: a house, a job, and Bea"),
    (2025, "Independent"),
    (2026, "So far"),
]

# --------------------------------------------------------------------------
# The planted oddities. Each one exists so a specific surface has to prove
# itself; a seed that is uniformly pleasant hides every state that matters.
# Offsets are days from START.
# --------------------------------------------------------------------------
D = lambda y, m, dd: (date(y, m, dd) - START).days  # noqa: E731

OUTAGE = (D(2025, 2, 11), 9)          # phone died — nine days with no streams at all
LISBON = (D(2025, 5, 9), 11)          # timezone + currency + a thinner message log
MIGRAINE_DAYS = [D(2024, 9, 14), D(2024, 11, 2), D(2025, 1, 23), D(2025, 3, 30),
                 D(2025, 6, 11), D(2025, 9, 7), D(2025, 12, 2)]
FIRST_KAYAK = D(2025, 7, 12)          # global novelty — a kind of event with no neighbours
RAN_INSTEAD = D(2025, 10, 8)          # local novelty — ordinary kind, far edge of its cluster
GHOST_EVENT = D(2025, 8, 21)          # a calendar block with no trace behind it
MOVE_DAY = D(2023, 11, 6)
GYM_JOINED = D(2024, 2, 1)
BEA_ARRIVES = D(2024, 3, 16)   # the household doubles; groceries follow
LATE_COFFEE = 475            # cents; cited verbatim by the migraine chat

# Days given full instrumentation: minute-level location, a heart-rate series,
# app sessions. Every month gets at least one so no month opens empty.
INSTRUMENTED = sorted({
    *[D(y, m, 12) for y in (2023, 2024, 2025) for m in range(1, 13)
      if START <= date(y, m, 12) <= ANCHOR_END],
    *MIGRAINE_DAYS, FIRST_KAYAK, RAN_INSTEAD, MOVE_DAY,
    D(2026, 1, 12), D(2026, 2, 9),
})


def in_outage(off: int) -> bool:
    return OUTAGE[0] <= off < OUTAGE[0] + OUTAGE[1]


def in_lisbon(off: int) -> bool:
    return LISBON[0] <= off < LISBON[0] + LISBON[1]


def in_chicago(off: int) -> bool:
    return off < MOVE_DAY


# --------------------------------------------------------------------------
# Generation
# --------------------------------------------------------------------------


def build():
    t = {}

    def tbl(name, cols):
        t[name] = Table(name, cols)
        return t[name]

    people = tbl("wiki_people", ["id", "name", "emails", "phones", "relationship_category",
                                 "nickname", "content", "metadata", "aliases", "bond"])
    places = tbl("wiki_places", ["id", "name", "category", "address", "latitude", "longitude",
                                 "radius_m", "metadata"])
    orgs = tbl("wiki_orgs", ["id", "name", "organization_type", "relationship_type",
                             "role_title", "started_at", "ended_at", "metadata"])
    chapters = tbl("wiki_chapters", ["id", "kind", "title", "started_at", "ended_at",
                                     "started_precision", "ended_precision", "summary"])
    stories = tbl("wiki_stories", ["id", "title", "started_at", "ended_at",
                                   "started_precision", "ended_precision", "summary"])
    years = tbl("wiki_years", ["id", "year", "title", "summary"])
    # Three columns is the whole table now — no morning_baseline, no
    # readiness, no data_quality. The outage therefore says nothing here; it
    # says it by having no streams and an `unknown` event, which is the honest
    # place for it anyway.
    days = tbl("wiki_days", ["id", "date", "start_timezone"])
    # NO DERIVED COLUMNS. `novelty_z`, `local_novelty_z`, `avg_hr` and
    # `embedding` are all computed by the box — annotation writes avg_hr, and
    # novelty stands on the event embedding. Seeding them looks harmless and is
    # not: `compute_novelty_for_day` only selects events where a novelty
    # channel is NULL, so a pre-set score makes the scorer skip that event
    # forever, and the row keeps a number invented here rather than one derived
    # from the record. Leave them NULL and let `virtues reindex` fill them —
    # which is also the only way the seeded novelty is ever true.
    events = tbl("wiki_events", ["id", "day_id", "started_at", "ended_at", "auto_label",
                                 "auto_location", "source_ontologies", "topics", "entities",
                                 "event_summary", "kind", "confidence"])

    hr = tbl("data_health_heart_rate", ["id", "bpm", "occurred_at", "source_stream_id",
                                        "source_table", "source_provider"])
    hrv = tbl("data_health_hrv", ["id", "hrv_ms", "occurred_at", "source_stream_id",
                                  "source_table", "source_provider"])
    steps = tbl("data_health_steps", ["id", "step_count", "occurred_at", "source_stream_id",
                                      "source_table", "source_provider"])
    sleep = tbl("data_health_sleep", ["id", "started_at", "ended_at", "duration_minutes",
                                      "sleep_quality_score", "sleep_stages", "source_stream_id",
                                      "source_table", "source_provider"])
    workout = tbl("data_health_workout", ["id", "workout_type", "started_at", "ended_at",
                                          "duration_minutes", "calories_burned", "distance_km",
                                          "avg_heart_rate", "max_heart_rate", "source_stream_id",
                                          "source_table", "source_provider"])
    lpoint = tbl("data_location_point", ["id", "latitude", "longitude", "horizontal_accuracy",
                                         "occurred_at", "speed", "source_stream_id",
                                         "source_table", "source_provider"])
    lvisit = tbl("data_location_visit", ["id", "place_name", "latitude", "longitude",
                                         "started_at", "ended_at", "duration_minutes",
                                         "source_stream_id", "source_table", "source_provider"])
    msg = tbl("data_communication_message", ["id", "message_id", "thread_id", "channel", "body",
                                             "from_identifier", "from_name", "to_identifiers",
                                             "is_read", "is_group_message", "has_attachments",
                                             "occurred_at", "source_stream_id", "source_table",
                                             "source_provider"])
    email = tbl("data_communication_email", ["id", "message_id", "thread_id", "subject", "body",
                                             "body_preview", "from_email", "from_name",
                                             "to_emails", "to_names", "cc_emails", "bcc_emails",
                                             "direction", "is_read", "is_starred",
                                             "has_attachments", "labels", "occurred_at",
                                             "source_stream_id", "source_table",
                                             "source_provider"])
    acct = tbl("data_financial_account", ["id", "account_name", "account_type",
                                          "institution_name", "mask", "currency",
                                          "current_balance", "available_balance", "credit_limit",
                                          "is_active", "source_stream_id", "source_table",
                                          "source_provider"])
    txn = tbl("data_financial_transaction", ["id", "account_id", "transaction_id", "amount",
                                             "currency", "merchant_name", "merchant_category",
                                             "description", "category", "is_pending",
                                             "transaction_type", "payment_channel",
                                             "occurred_at", "source_stream_id", "source_table",
                                             "source_provider"])
    cal = tbl("data_calendar_event", ["id", "title", "description", "calendar_name", "status",
                                      "attendee_identifiers", "location_name", "started_at",
                                      "ended_at", "is_all_day", "is_sacred", "source_stream_id",
                                      "source_table", "source_provider"])
    wx = tbl("data_environment_weather", ["id", "latitude", "longitude", "occurred_at",
                                          "issued_at", "is_forecast", "temperature_c",
                                          "apparent_c", "humidity_pct", "precipitation_mm",
                                          "wind_kph", "temp_max_c", "temp_min_c", "weather_code",
                                          "source_stream_id", "source_table", "source_provider"])

    chats = tbl("app_chats", ["id", "title", "message_count", "icon", "created_at", "updated_at"])
    cmsg = tbl("app_chat_messages", ["id", "chat_id", "role", "content", "model", "provider",
                                     "sequence_num", "created_at"])
    pages = tbl("app_pages", ["id", "title", "content", "icon", "tags", "kind",
                              "created_at", "updated_at"])
    articles = tbl("wiki_articles", ["id", "subject_type", "subject_id", "page_id",
                                     "last_written_at", "maintenance", "theirs", "removed"])

    # ---- entities -------------------------------------------------------
    for pid, name, rel, bond, f, l, w, notes in PEOPLE:
        handle = name.split()[0].lower()
        people.add(pid, name, [f"{handle}@example.com"],
                   [f"+1512555{RNG.randint(1000, 9999):04d}"], rel, None, notes,
                   {"weight": w}, [], bond)

    for pid, name, cat, dlat, dlon, addr in PLACES:
        places.add(pid, name, cat, addr, round(LAT0 + dlat, 6), round(LON0 + dlon, 6),
                   120.0 if cat in ("residence", "work") else 80.0, {})

    for oid, name, otype, rtype, role, s, e in ORGS:
        orgs.add(oid, name, otype, rtype, role, s, e, {})

    for cid, kind, title, s, e, sp, ep, summary in CHAPTERS:
        chapters.add(cid, kind, title, s, e, sp, ep, summary)

    for sid, title, s, e, sp, ep, summary in STORIES:
        stories.add(sid, title, s, e, sp, ep, summary)

    for y, title in YEARS:
        years.add(f"year_{y}", y, title, None)

    # ---- accounts -------------------------------------------------------
    acct.add("p3y_acct_check", "Everyday Checking", "depository",
             "Third Coast Credit Union", "4417", "USD", 812_44, 812_44, None, True,
             "p3y_s_acct_1", "data_financial_account", "demo")
    acct.add("p3y_acct_card", "Visa Signature", "credit", "Third Coast Credit Union",
             "9032", "USD", -1_244_18, None, 800_000, True,
             "p3y_s_acct_2", "data_financial_account", "demo")
    acct.add("p3y_acct_save", "Savings", "depository", "Third Coast Credit Union",
             "5580", "USD", 14_320_09, 14_320_09, None, True,
             "p3y_s_acct_3", "data_financial_account", "demo")

    # ---- per-day --------------------------------------------------------
    n = 0
    for off in range(DAYS):
        d = START + timedelta(days=off)
        did = f"day_{d.isoformat()}"
        outage, lisbon, chi = in_outage(off), in_lisbon(off), in_chicago(off)
        instrumented = off in INSTRUMENTED and not outage
        migraine = off in MIGRAINE_DAYS
        weekend = d.weekday() >= 5

        days.add(did, d, "Europe/Lisbon" if lisbon else TZ)

        if outage:
            # Nine days with nothing under them. One `unknown` event per day so
            # the gap is visible in the record rather than merely absent from
            # it — a hole a reader can see is a feature; a hole they cannot is
            # a bug report.
            events.add(f"p3y_ev_{d.isoformat()}_0", did, ts(d, 0, 0), ts(d, 23, 59),
                       None, None, [], [], [], "No data reached the server on this day.",
                       "unknown", "low")
            n += 1
            continue

        _day_health(d, off, hr, hrv, steps, sleep, workout, migraine, instrumented, weekend)
        _day_location(d, off, lpoint, lvisit, instrumented, lisbon, chi, weekend)
        _day_comms(d, off, msg, email, lisbon)
        _day_money(d, off, txn, lisbon, chi)
        _day_weather(d, off, wx, lisbon)
        _day_calendar(d, off, cal, weekend, chi)
        _day_events(d, off, did, events, migraine, instrumented, lisbon, chi, weekend)
        n += 1

    facts = compute_facts(t)
    _chats(chats, cmsg, facts)
    _pages_and_articles(pages, articles, facts)

    return t, n


# --------------------------------------------------------------------------
# Per-day stream writers
# --------------------------------------------------------------------------


def _day_health(d, off, hr, hrv, steps, sleep, workout, migraine, instrumented, weekend):
    k = d.isoformat()
    # Sleep. The migraine days are preceded by a short, poor night — the
    # correlation has to be IN the data or "why do I have a migraine today?"
    # has no honest answer.
    short = migraine or (off + 1) in MIGRAINE_DAYS
    dur = RNG.randint(283, 337) if short else RNG.randint(392, 501)
    bed = ts(d - timedelta(days=1), 23, RNG.randint(0, 55))
    sleep.add(f"p3y_sl_{k}", bed, bed + timedelta(minutes=dur), dur,
              round(RNG.uniform(0.41, 0.58) if short else RNG.uniform(0.66, 0.92), 2),
              {"deep": int(dur * 0.17), "rem": int(dur * 0.22), "core": int(dur * 0.55),
               "awake": int(dur * 0.06)},
              f"p3y_s_sl_{k}", "data_health_sleep", "demo")

    # HRV: one morning reading, depressed on and around a migraine day.
    base = 41 if short else RNG.uniform(56, 78)
    hrv.add(f"p3y_hrv_{k}", round(base + RNG.uniform(-4, 4), 1), ts(d, 7, 12),
            f"p3y_s_hrv_{k}", "data_health_hrv", "demo")

    # Resting heart rate through the day, and a dense series when instrumented.
    hours = range(6, 23) if instrumented else (7, 12, 18, 22)
    for h in hours:
        bpm = RNG.randint(58, 74)
        if migraine:
            bpm += 9
        hr.add(f"p3y_hr_{k}_{h}", bpm, ts(d, h, RNG.randint(0, 59)),
               f"p3y_s_hr_{k}_{h}", "data_health_heart_rate", "demo")

    steps_total = RNG.randint(1400, 3600) if migraine else (
        RNG.randint(6200, 14800) if weekend else RNG.randint(4100, 10200))
    if instrumented:
        for h in range(7, 22):
            steps.add(f"p3y_st_{k}_{h}", max(0, int(steps_total / 15 + RNG.randint(-260, 420))),
                      ts(d, h, 30), f"p3y_s_st_{k}_{h}", "data_health_steps", "demo")
    else:
        steps.add(f"p3y_st_{k}", steps_total, ts(d, 21, 0),
                  f"p3y_s_st_{k}", "data_health_steps", "demo")

    # Workouts. Lifting most weekdays from the gym join onward; the two novelty
    # cases are workouts too, which is the point — they are ordinary in kind.
    if off == FIRST_KAYAK:
        w = ("kayaking", 95, 168, 7.4, 128, 154)
    elif off == RAN_INSTEAD:
        w = ("running", 52, 604, 9.1, 161, 178)
    elif not migraine and d.weekday() in (0, 2, 4) and off > 360:
        w = ("strength_training", RNG.randint(41, 66), RNG.randint(180, 340), None,
             RNG.randint(104, 122), RNG.randint(131, 152))
    elif weekend and RNG.random() < 0.45:
        w = ("walking", RNG.randint(28, 71), RNG.randint(90, 210), round(RNG.uniform(2.1, 5.8), 1),
             RNG.randint(92, 108), RNG.randint(110, 126))
    else:
        return
    start = ts(d, 17 if not weekend else 9, RNG.randint(0, 40))
    workout.add(f"p3y_wk_{k}", w[0], start, start + timedelta(minutes=w[1]), w[1], w[2], w[3],
                w[4], w[5], f"p3y_s_wk_{k}", "data_health_workout", "demo")


def _day_location(d, off, lpoint, lvisit, instrumented, lisbon, chi, weekend):
    k = d.isoformat()
    if lisbon:
        stops = [("Lisbon", 8.43, 88.6, 8, 20)]
    elif chi:
        stops = ([("Logan Square apartment", 11.60, 10.13, 0, 8),
                  ("The agency", 11.62, 10.10, 9, 18),
                  ("Logan Square apartment", 11.60, 10.13, 19, 23)]
                 if not weekend else
                 [("Logan Square apartment", 11.60, 10.13, 0, 11),
                  ("Logan Square apartment", 11.60, 10.13, 15, 23)])
    elif weekend:
        stops = [("Home", 0.0, 0.0, 0, 10),
                 (RNG.choice(["Mueller Farmers Market", "Lady Bird Lake", "H-E-B", "Bea's"]),
                  RNG.uniform(-0.01, 0.022), RNG.uniform(-0.012, 0.014), 10, 15),
                 ("Home", 0.0, 0.0, 16, 23)]
    else:
        stops = [("Home", 0.0, 0.0, 0, 8),
                 ("Office" if off < D(2025, 1, 15) else "Home", 0.0121 if off < D(2025, 1, 15) else 0.0,
                  -0.0187 if off < D(2025, 1, 15) else 0.0, 9, 17),
                 ("Home", 0.0, 0.0, 18, 23)]

    for i, (nm, dlat, dlon, h0, h1) in enumerate(stops):
        s, e = ts(d, h0, RNG.randint(0, 30)), ts(d, h1, RNG.randint(0, 50))
        lvisit.add(f"p3y_lv_{k}_{i}", nm, round(LAT0 + dlat, 6), round(LON0 + dlon, 6),
                   s, e, int((e - s).total_seconds() // 60),
                   f"p3y_s_lv_{k}_{i}", "data_location_visit", "demo")

    if not instrumented:
        return
    # A real trace on instrumented days — enough for the movement map to draw a
    # path rather than a scatter of stops. Capped at 22 so `demo_day.sql`'s
    # 26-point Friday stays the single richest day in the database.
    for i in range(22):
        frac = i / 21
        a, b = stops[0], stops[min(1, len(stops) - 1)]
        jitter = RNG.uniform(-0.0012, 0.0012)
        lpoint.add(f"p3y_lp_{k}_{i:02d}",
                   round(LAT0 + a[1] + (b[1] - a[1]) * frac + jitter, 6),
                   round(LON0 + a[2] + (b[2] - a[2]) * frac + jitter, 6),
                   round(RNG.uniform(4.0, 18.0), 1),
                   ts(d, 8, 0) + timedelta(minutes=i * 3),
                   round(RNG.uniform(0.0, 11.5), 2),
                   f"p3y_s_lp_{k}_{i:02d}", "data_location_point", "demo")


# Stable per-person numbers. `hash()` is salted per process, so deriving these
# from it made the generator non-deterministic — two runs produced different
# SQL while the docstring promised they would not.
PHONE = {p[0]: f"+1512555{2000 + i * 7:04d}" for i, p in enumerate(PEOPLE)}


def _day_comms(d, off, msg, email, lisbon):
    k = d.isoformat()
    # Contact frequency decays for the Chicago half and rises for the Austin
    # half, so "who have I lost touch with" is answerable from counts alone.
    live = [p for p in PEOPLE if p[4] <= off <= p[5]]
    count = RNG.randint(1, 3) if lisbon else RNG.randint(2, 7)
    for i in range(count):
        pid, name, *_rest = RNG.choices(live, weights=[p[6] for p in live])[0]
        handle = name.split()[0].lower()
        inbound = RNG.random() < 0.55
        body = RNG.choice([
            "running about ten minutes late", "did you see the thing I sent",
            "thursday still good?", "call me when you get a sec",
            "ok that's actually very funny", "landed", "made too much soup, come over",
            "no rush on this at all", "found it, never mind", "how'd it go?",
        ])
        msg.add(f"p3y_ms_{k}_{i}", f"p3y_msgid_{k}_{i}", f"p3y_thr_{pid}", "imessage", body,
                PHONE[pid] if inbound else "+15125550100",
                name if inbound else None,
                ["+15125550100"] if inbound else [f"{handle}@example.com"],
                True, False, False, ts(d, RNG.randint(8, 22), RNG.randint(0, 59)),
                f"p3y_s_ms_{k}_{i}", "data_communication_message", "demo")

    if RNG.random() < 0.6:
        subj = RNG.choice([
            "Re: invoice", "Your order has shipped", "Notes from today",
            "Re: Thursday", "Statement available", "Re: the draft", "Appointment reminder",
        ])
        email.add(f"p3y_em_{k}", f"p3y_emid_{k}", f"p3y_ethr_{k}", subj,
                  "Attached.\n\nBest,\n", "Attached.", "notifications@example.com",
                  "Notifications", ["you@example.com"], ["You"], [], [],
                  "received", RNG.random() < 0.8, False, RNG.random() < 0.2,
                  ["inbox"], ts(d, RNG.randint(7, 20), RNG.randint(0, 59)),
                  f"p3y_s_em_{k}", "data_communication_email", "demo")


def _day_money(d, off, txn, lisbon, chi):
    k = d.isoformat()
    cur = "EUR" if lisbon else "USD"
    # The subscription nobody uses. Charged on the 6th of every month for the
    # whole span — this is what makes "what am I still paying for that I never
    # use?" a question the record can actually answer.
    # Gym membership: monthly, like the subscription. It used to sit in the
    # random daily pool, which billed it 358 times in three years.
    if d.day == 14 and off > GYM_JOINED:
        txn.add(f"p3y_tx_gym_{k}", "p3y_acct_card", f"p3y_txid_gym_{k}", 6_500, "USD",
                "Eastside Barbell", "fitness", "EASTSIDE BARBELL MONTHLY",
                ["Service", "Fitness"], False, "debit", "online",
                ts(d, 6, 30), f"p3y_s_tx_gym_{k}", "data_financial_transaction", "demo")

    # A late coffee the evening before each migraine, so the correlation the
    # demo chat cites is one the record actually holds rather than one the
    # prose asserts.
    if (off + 1) in MIGRAINE_DAYS:
        txn.add(f"p3y_tx_latecoffee_{k}", "p3y_acct_card", f"p3y_txid_lc_{k}", LATE_COFFEE, "USD",
                "Jo's", "coffee", "JOS COFFEE", ["Shops", "Coffee"], False, "debit",
                "in store", ts(d, 18, 40), f"p3y_s_tx_lc_{k}",
                "data_financial_transaction", "demo")

    if d.day == 6:
        txn.add(f"p3y_tx_sub_{k}", "p3y_acct_card", f"p3y_txid_sub_{k}", 2_400, "USD",
                "Lumen Cloud Storage", "subscription", "LUMEN CLOUD MONTHLY",
                ["Service", "Subscription"], False, "debit", "online",
                ts(d, 4, 12), f"p3y_s_tx_sub_{k}", "data_financial_transaction", "demo")
    for i in range(RNG.randint(1, 4)):
        merchant, cat, lo, hi = RNG.choice([
            ("H-E-B", "groceries", 1_800, 14_200), ("Jo's", "coffee", 400, 1_250),
            ("Ramen Tatsu-ya", "restaurants", 1_600, 4_400),
            ("Third Coast CU", "transfer", 5_000, 60_000),
            ("Central Market", "groceries", 2_200, 9_800),
            ("Shell", "fuel", 2_800, 6_100),
        ]) if not lisbon else RNG.choice([
            ("Pastelaria Aloma", "restaurants", 320, 1_450),
            ("Metropolitano de Lisboa", "transit", 165, 640),
            ("Livraria Bertrand", "shopping", 890, 4_200),
        ])
        amount = RNG.randint(lo, hi)
        if cat == "groceries":
            # One household, then two. The basket grows with the story rather
            # than drifting at random, so "am I spending more?" has a real
            # answer and a real cause underneath it.
            amount = int(amount * (1.0 if chi else 1.45 if off < BEA_ARRIVES else 2.15))
        txn.add(f"p3y_tx_{k}_{i}", "p3y_acct_card", f"p3y_txid_{k}_{i}",
                amount, cur, merchant, cat, merchant.upper(),
                ["Shops", cat.title()], False, "debit",
                "online" if RNG.random() < 0.3 else "in store",
                ts(d, RNG.randint(8, 21), RNG.randint(0, 59)),
                f"p3y_s_tx_{k}_{i}", "data_financial_transaction", "demo")


def _day_weather(d, off, wx, lisbon):
    k = d.isoformat()
    lat, lon = (38.72, -9.14) if lisbon else (LAT0, LON0)
    seasonal = 20 + 12 * math.sin((d.timetuple().tm_yday - 100) / 365 * 2 * math.pi)
    hi = round(seasonal + RNG.uniform(1, 7), 1)
    lo = round(seasonal - RNG.uniform(4, 11), 1)
    rain = round(RNG.choice([0, 0, 0, 0, 0.4, 2.1, 8.6, 19.2]), 1)
    wx.add(f"p3y_wx_{k}", lat, lon, ts(d, 12), ts(d, 5), False,
           round((hi + lo) / 2, 1), round((hi + lo) / 2 + RNG.uniform(-2, 3), 1),
           round(RNG.uniform(38, 88), 1), rain, round(RNG.uniform(3, 27), 1),
           hi, lo, 61 if rain > 1 else 2,
           f"p3y_s_wx_{k}", "data_environment_weather", "demo")


def _day_calendar(d, off, cal, weekend, chi):
    k = d.isoformat()
    # The ghost: a calendar block with nothing behind it. The day page must
    # render this as an honest `unknown`, never as a thing that happened.
    if off == GHOST_EVENT:
        cal.add(f"p3y_cal_ghost_{k}", "Coffee — J.", None, "Personal", "confirmed",
                [], None, ts(d, 15, 0), ts(d, 16, 0), False, False,
                f"p3y_s_cal_ghost_{k}", "data_calendar_event", "demo")
        return
    if weekend and RNG.random() < 0.7:
        return
    for i in range(RNG.randint(0, 3)):
        title, where = RNG.choice([
            ("Design review", "Office"), ("1:1 with Mara", "Office"),
            ("Standup", None), ("Dentist", "Hyde Park Family Medicine"),
            ("Dinner — Nell's", "Nell's"), ("Call with Priya", None),
            ("Market run", "Mueller Farmers Market"), ("Client call", None),
        ])
        h = RNG.randint(9, 18)
        cal.add(f"p3y_cal_{k}_{i}", title, None, "Work" if chi or h < 17 else "Personal",
                "confirmed", [], where, ts(d, h, 0), ts(d, h + 1, 0), False,
                title.startswith("Dinner"), f"p3y_s_cal_{k}_{i}",
                "data_calendar_event", "demo")


def _day_events(d, off, did, events, migraine, instrumented, lisbon, chi, weekend):
    """The derived layer. Thin on purpose relative to the streams beneath it —
    on a real box these come out of the segmentation pass, and the point of
    seeding raw data first is that the pass can be re-run to replace them."""
    k = d.isoformat()
    seq = 0

    def ev(h0, h1, label, where, onts, summary, kind="stay", conf="high",
           topics=None, entities=None):
        nonlocal seq
        events.add(f"p3y_ev_{k}_{seq}", did, ts(d, h0, 0), ts(d, h1, 0), label, where,
                   onts, topics or [], entities or [], summary, kind, conf)
        seq += 1

    ev(0, 7, "Asleep", "Home", ["data_health_sleep"],
       "A short night — under five hours." if migraine else "A full night.",
       kind="sleep", conf="high")

    if migraine:
        ev(7, 12, "A slow morning", "Home",
           ["data_health_heart_rate", "data_health_hrv", "data_health_sleep"],
           "Migraine. Low light, no screen, and almost no movement until the afternoon.",
           conf="high", topics=["health"], entities=[])
        ev(12, 22, "Recovering", "Home", ["data_location_visit", "data_health_steps"],
           "Fourteen hundred steps for the whole day.", conf="medium")
        return

    if off == GHOST_EVENT:
        ev(15, 16, None, None, ["data_calendar_event"],
           "A calendar block with nothing behind it — no location, no messages, no spend.",
           kind="unknown", conf="low")

    if lisbon:
        ev(8, 20, "Lisbon", "Lisbon", ["data_location_visit", "data_financial_transaction"],
           "Out most of the day. Everything priced in euros.", conf="medium",
           topics=["travel"], entities=[])
        return

    if off == FIRST_KAYAK:
        ev(9, 12, "Kayaking on the lake", "Lady Bird Lake",
           ["data_health_workout", "data_location_visit", "data_location_point"],
           "First time on the water. Nothing else in the record looks like it.",
           conf="high", topics=["outdoors", "first"])
    elif off == RAN_INSTEAD:
        ev(17, 18, "Ran instead of lifting", "Mueller Trails",
           ["data_health_workout", "data_health_heart_rate"],
           "An ordinary workout, at the far edge of its own kind — nine kilometres "
           "on a day that is usually a lifting day.",
           conf="high", topics=["fitness"])
    elif chi:
        ev(9, 18, "At the agency", "The agency",
           ["data_location_visit", "data_calendar_event"], "A studio day.", conf="high")
    elif weekend:
        ev(10, 15, "Out", "Austin", ["data_location_visit"],
           "Market, then errands.", conf="medium")
    else:
        ev(9, 17, "Working", "Office" if off < D(2025, 1, 15) else "Home",
           ["data_location_visit", "data_calendar_event", "data_activity_app_session"],
           "A working day.", conf="high")

    ev(18, 23, "Evening", "Home", ["data_location_visit", "data_communication_message"],
       "Home by six.", conf="medium")


# --------------------------------------------------------------------------
# The creation layer — nothing seeded this before, so Chats, Pages and every
# article surface have been empty in every demo we have ever given.
# --------------------------------------------------------------------------

def _col(t, name):
    i = t.cols.index(name)
    return [r[i] for r in t.rows]


def compute_facts(t):
    """Every number the demo chats and articles quote, measured off the rows
    that were just generated.

    This exists because the first version did not. The chats were written by
    hand beside the data and five of six checkable claims disagreed with it —
    a sleep median off by twenty minutes, a gym billed 358 times instead of 36,
    a coffee that was never bought. For a product whose entire claim is that
    answers come from the record, a demo whose assistant contradicts its own
    database is worse than a demo with no chat history at all.

    ONLY SHIFT-INVARIANT FACTS. `demo3y_reanchor` walks the whole life forward
    onto today, and it moves rows, not prose. Counts, totals, medians and the
    distance between two events all survive that. "795 days ago" and any
    absolute date do not — they were wrong the morning after they were written.
    """
    import statistics as st

    f = {}

    sleep_mins = _col(t["data_health_sleep"], "duration_minutes")
    med = round(st.median(sleep_mins))
    f["sleep_median"] = f"{med // 60}h{med % 60:02d}m"

    shortest = round(min(sleep_mins))
    f["short_night"] = f"{shortest // 60}h{shortest % 60:02d}m"

    hrv = _col(t["data_health_hrv"], "hrv_ms")
    f["hrv_median"] = round(st.median(hrv))
    f["hrv_low"] = round(min(hrv), 1)

    f["migraine_n"] = len(MIGRAINE_DAYS)
    f["late_coffee"] = f"${LATE_COFFEE / 100:.2f}"

    txn = t["data_financial_transaction"]
    merch, amt, when = _col(txn, "merchant_name"), _col(txn, "amount"), _col(txn, "occurred_at")

    def total(name):
        rows = [a for m, a in zip(merch, amt) if m == name]
        return len(rows), sum(rows)

    n, tot = total("Lumen Cloud Storage")
    f["lumen_n"], f["lumen_total"] = n, f"${tot / 100:,.0f}"
    f["lumen_monthly"] = f"${tot / n / 100:.2f}"
    f["gym_n"], gym_tot = total("Eastside Barbell")
    f["gym_monthly"] = f"${gym_tot / f['gym_n'] / 100:.0f}" if f["gym_n"] else "$0"

    # Groceries: first twelve months against the last ninety days, monthly.
    first_cut = START + timedelta(days=365)
    last_cut = ANCHOR_END - timedelta(days=90)
    g_first = [a for m, a, w in zip(merch, amt, when)
               if m in ("H-E-B", "Central Market") and w.date() < first_cut]
    g_last = [a for m, a, w in zip(merch, amt, when)
              if m in ("H-E-B", "Central Market") and w.date() >= last_cut]
    f["groc_early"] = f"${sum(g_first) / 12 / 100:.0f}"
    f["groc_late"] = f"${sum(g_last) / 3 / 100:.0f}"
    early_m, late_m = sum(g_first) / 12, sum(g_last) / 3
    delta = (late_m - early_m) / early_m * 100
    f["groc_dir"] = "up" if delta >= 0 else "down"
    f["groc_verdict"] = "Yes." if delta >= 0 else "No — less, in fact."
    f["groc_pct"] = f"{abs(delta):.0f}%"

    # Theo: how much he is in the record, and how long after the move it ends.
    names = _col(t["data_communication_message"], "from_name")
    times = _col(t["data_communication_message"], "occurred_at")
    theo = [w for n_, w in zip(names, times) if n_ == "Theo Brandt"]
    f["theo_n"] = len(theo)
    if theo:
        weeks = (max(theo).date() - (START + timedelta(days=MOVE_DAY))).days / 7
        f["theo_weeks_after_move"] = f"{weeks:.0f} weeks"
    else:
        f["theo_weeks_after_move"] = "shortly"

    wk = _col(t["data_health_workout"], "workout_type")
    f["kayak_n"] = wk.count("kayaking")
    f["run_n"] = wk.count("running")
    f["lift_n"] = wk.count("strength_training")

    f["outage_days"] = OUTAGE[1]
    f["days"] = DAYS
    f["years"] = round(DAYS / 365)
    return f


def _chats(chats, cmsg, f):
    """Six chats. Every figure below is interpolated from `compute_facts`, so
    the assistant's answers and the database agree by construction."""
    convos = [
        ("Why do I have a migraine today?", "\U0001FA7A", D(2025, 6, 11), [
            ("user", "why do I have a migraine today?"),
            ("assistant",
             f"Three things line up against your own baseline.\n\n"
             f"You slept {f['short_night']} — your median across the record is "
             f"{f['sleep_median']}, and every one of the {f['migraine_n']} migraine days "
             f"here is preceded by a night well under it.\n\nYour morning HRV came in at "
             f"{f['hrv_low']} ms against a median of {f['hrv_median']} ms.\n\nAnd there "
             f"was a {f['late_coffee']} coffee at Jo's at 6:40pm yesterday, which is late "
             f"for you.\n\nWhat I can't tell you is which of the three did it, or whether "
             f"they share a cause."),
            ("user", "what about light?"),
            ("assistant",
             "I don't have light data. Nothing in your streams measures it, and I'd be "
             "guessing if I inferred it from time outdoors. If it matters to you it's "
             "worth writing down by hand on the days it happens; after a season of that "
             "there'd be something to check against."),
        ]),
        ("What am I still paying for that I never use?", "\U0001F4B3", D(2025, 9, 2), [
            ("user", "what am I still paying for that I never use?"),
            ("assistant",
             f"One stands out clearly.\n\n**Lumen Cloud Storage**, {f['lumen_monthly']} on "
             f"the 6th of every month. {f['lumen_n']} consecutive charges across the record "
             f"— {f['lumen_total']} in total. I can find no app session, no browsing, and no "
             f"email from them other than the receipts.\n\n**Eastside Barbell** at "
             f"{f['gym_monthly']}/mo is the opposite case: {f['lift_n']} logged sessions "
             f"against {f['gym_n']} charges, so that one is being used.\n\nI can't cancel "
             f"it for you, and I can't see whether something depends on it."),
        ]),
        ("Who have I lost touch with?", "\U0001F4C7", D(2025, 11, 18), [
            ("user", "who have I lost touch with that I used to talk to every day?"),
            ("assistant",
             f"**Theo Brandt.** {f['theo_n']} messages in the record, nearly all of them "
             f"before the move — he's the most frequent name in the early part of it, and "
             f"he's constantly in the location data because you lived together. The last "
             f"message is about {f['theo_weeks_after_move']} after the move.\n\n"
             f"**Junie Ferrar** follows the same shape but softer, tapering over about a "
             f"year rather than stopping.\n\nBoth were Chicago. The drop lines up with the "
             f"move rather than with anything between you, as far as the record shows — "
             f"though the record would not know if it were otherwise."),
        ]),
        ("Reading the last year", "\U0001F4D6", D(2026, 1, 8), [
            ("user", "summarize what changed for me"),
            ("assistant",
             f"You left Canopy in January without another job lined up, and the weeks "
             f"after that are the thinnest stretch in the record — few calendar events, "
             f"little movement, and the one chapter you've never given a name to.\n\n"
             f"From March the shape changes: longer days at home, and a different set of "
             f"people in them — Priya, Desmond, Nadia, none of whom appear earlier.\n\n"
             f"Lisbon is the only travel in {f['years']} years.\n\nThere is also a "
             f"{f['outage_days']}-day hole where nothing reached the server at all. I've "
             f"left it as a hole rather than guessing what filled it."),
        ]),
        ("Kayaks", "\U0001F6F6", FIRST_KAYAK, [
            ("user", "was today unusual?"),
            ("assistant",
             f"Yes. The kayaking has no neighbours at all in your history — it is the only "
             f"one in {f['days']} days, against {f['lift_n']} lifting sessions and "
             f"{f['run_n']} runs, so there is no prior event of that kind to compare it "
             f"against.\n\nEverything else about the day was ordinary.\n\nI'm not going "
             f"to tell you whether it was a good day. That one's yours."),
        ]),
        ("Groceries, over time", "\U0001F9FE", D(2025, 10, 21), [
            ("user", "am I spending more on groceries than I used to?"),
            ("assistant",
             f"{f['groc_verdict']} Monthly grocery spend averaged {f['groc_early']} across "
             f"the first year of the record and {f['groc_late']} over the last ninety days "
             f"— {f['groc_dir']} {f['groc_pct']}.\n\nTwo things sit underneath that: the move to Austin, "
             f"after which the basket gets bigger, and Bea.\n\nI can't separate price "
             f"changes from basket changes — the record has totals, not line items."),
        ]),
    ]
    for i, (title, icon, off, turns) in enumerate(convos):
        cid = f"p3y_chat_{i:02d}"
        d = START + timedelta(days=off)
        base = ts(d, 20, 14)
        chats.add(cid, title, len(turns), icon, base, base + timedelta(minutes=6))
        for j, (role, content) in enumerate(turns):
            cmsg.add(f"{cid}_m{j:02d}", cid, role, content,
                     None if role == "user" else "demo-model",
                     None if role == "user" else "demo",
                     j, base + timedelta(minutes=j * 2))


def _pages_and_articles(pages, articles, f):
    now = ts(ANCHOR_END, 9, 0)
    arts = [
        ("narrative_identity", "nar_identity_001", "In your own words",
         "I grew up in Rockford and left as soon as there was somewhere to go. Chicago for "
         "eleven years, most of it at one agency, and I was good at that work without ever "
         "being sure it was mine.\n\nThe move to Austin was Bea's idea before it was mine, "
         "which I have never quite admitted to her.\n\nI think in drafts. I get to the end "
         "of a thing and then rewrite it from the start, and I've stopped apologizing for "
         "how slow that is.\n\nI am not looking for advice about my life. I want to be "
         "able to look at it clearly — what I actually did, not what I meant to do. That is "
         "the whole reason this thing is in my house.\n\nMy father kept a daybook for "
         "forty years. I read them after he died and there was almost nothing in them: "
         "weather, what he paid for gas, who came by. It was the most complete picture of "
         "him I will ever have."),
        ("chapter", "p3y_ch_building", "Building the thing",
         "It started as three weeks off and became the work. There was no announcement and "
         "no plan — the calendar simply emptied and filled back up, differently, two months "
         "later.\n\nThe people are different too. Almost nobody in this chapter appears in "
         "the one before it."),
        ("year", "year_2025", "2025",
         f"The year began with no job and ended with a working thing.\n\nThe first weeks "
         f"are the thinnest stretch in the whole record, and the one that has never been "
         f"given a name. Lisbon was the only travel. The migraines — {f['migraine_n']} of "
         f"them across the record — thinned out in the second half, and sleep improved over "
         f"the same months, which is suggestive and not more than that."),
        ("person", "p3y_theo", "Theo Brandt",
         f"Roommate in Logan Square and, for the early part of this record, the person you "
         f"spoke to most — {f['theo_n']} messages, and constantly in the location data "
         f"because you lived in the same rooms.\n\nThe messages stop about "
         f"{f['theo_weeks_after_move']} after the move to Austin. There is no argument in "
         f"the record and no final conversation; the frequency decays and then ends. The "
         f"record can show that it happened. It cannot say why, and neither of you appears "
         f"to have written it down."),
        ("place", "place_demo_home", "Home",
         "1847 Selden St. Bought after six weeks of showings with Cora Delgado and one "
         "inspection that nearly ended it.\n\nIt is the most-visited place in the record "
         "by a wide margin, and since leaving Canopy it is also where the work happens — "
         "the Office visits stop almost exactly when the job does."),
    ]
    for i, (subject_type, subject_id, title, content) in enumerate(arts):
        pid = f"p3y_page_{i:02d}"
        pages.add(pid, title, content, None, [], "article", now, now)
        articles.add(f"p3y_art_{i:02d}", subject_type, subject_id, pid, now, "auto", [], [])

    for off in range(DAYS - 120, DAYS):
        d = START + timedelta(days=off)
        if in_outage(off):
            continue
        did = f"day_{d.isoformat()}"
        pid = f"p3y_page_day_{d.isoformat()}"
        if off in MIGRAINE_DAYS:
            prose = ("A short night and then a migraine that took the morning. Low light, "
                     "no screen, barely any movement. By evening it had lifted enough to eat.")
        elif off == FIRST_KAYAK:
            prose = ("Out on the lake by nine and on the water until noon — the first time, "
                     "and nothing else in the record looks like it. Home by six, sunburnt "
                     "and pleased.")
        elif in_lisbon(off):
            prose = ("Lisbon. Out most of the day, everything priced in euros, and a long "
                     "walk back along the water in the evening.")
        else:
            prose = ("A working day at the house, quiet through the afternoon. " +
                     RNG.choice([
                         "Dinner at Nell's.", "Groceries on the way home.",
                         "A long call with Mara that went somewhere unexpected.",
                         "Nothing much after six.", "Out to the trails before dark.",
                     ]))
        pages.add(pid, d.strftime("%A, %-d %B %Y"), prose, None, [], "article", now, now)
        articles.add(f"p3y_art_day_{d.isoformat()}", "day", did, pid, now, "auto", [], [])


REANCHOR = """\
-- Re-anchor: walk the generated life forward so its last day lands on today.
--
-- EXACT DAYS, matching `demo_reanchor.sql`, and this is a trade rather than a
-- preference. A whole-week shift would preserve weekday shape — markets on
-- Saturdays, no calendar at weekends, which this set now has and the old
-- twelve-week one did not — but it lands the last day up to six days behind
-- today, and Home asks for the literal current date with no fallback to the
-- newest day holding data. A demo whose front page is blank for six days out
-- of seven is worse than one whose farmers market drifts to a Wednesday.
--
-- The fix that buys both is to generate the final partial week against the
-- real current date at seed time instead of from a fixed anchor. Worth doing
-- if the weekday rotation is ever noticed in Lifeline; it is not free, which
-- is why it is written down here rather than done.
--
-- Anchors on a known date rather than on "the day with the most location
-- points", because `demo_day.sql` deliberately keeps the densest day and would
-- otherwise capture the anchor.
--
-- RUN THIS INSTEAD OF `demo_reanchor.sql`, not alongside it. That file is
-- generic over every timestamp column in `data_*` and `wiki_*` and would shift
-- this set a second time. It also skips `app_*` on purpose — product state it
-- must not drag — which is why the chats and pages below are moved here by
-- name rather than by a sweep.
--
-- Idempotent: after a run the remaining offset is under seven days and floors
-- to zero, so a second run moves nothing.

DO $$
DECLARE
  cur_anchor date;
  shift_days integer;
BEGIN
  -- READ THE ANCHOR OUT OF THE DATA, never from a constant. A fixed anchor
  -- makes this file shift again on every run, walking the life further into
  -- the future each time — which is not a theoretical failure, it is what the
  -- first draft of this file did (+216 days on the second run). Reading it
  -- back means a second run computes zero and returns, and that is also what
  -- makes re-running the seed the way to re-age a long-lived demo box.
  --
  -- Anchored on this set's own events (`p3y_`) rather than on max(wiki_days),
  -- because `demo_day.sql` shares the `day_<date>` id namespace and its own
  -- instrumented Friday sits two days later.
  SELECT max(ended_at)::date INTO cur_anchor FROM wiki_events WHERE id LIKE 'p3y_%%';
  IF cur_anchor IS NULL THEN RETURN; END IF;
  shift_days := current_date - cur_anchor;
  IF shift_days = 0 THEN RETURN; END IF;

  -- `wiki_days.date` is UNIQUE, and Postgres writes a unique index row by row
  -- inside an UPDATE rather than deferring the check to the end of the
  -- statement. A uniform `date + N` therefore collides mid-statement whenever
  -- N is smaller than the span — always true here, three years against a shift
  -- of months. Park the column far outside any plausible range and land it
  -- from there, so neither pass writes a value the column already holds.
  -- (`demo_reanchor.sql` solves the same problem the same way; this file
  -- cannot simply call it, see the header.)
  UPDATE wiki_days SET date = date + 100000;
  UPDATE wiki_days SET date = date - 100000 + shift_days;
  UPDATE wiki_events          SET started_at = started_at + (shift_days || ' days')::interval,
                                  ended_at   = ended_at   + (shift_days || ' days')::interval
                              WHERE id LIKE 'p3y_%%';
  UPDATE data_health_heart_rate SET occurred_at = occurred_at + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE data_health_hrv        SET occurred_at = occurred_at + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE data_health_steps      SET occurred_at = occurred_at + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE data_health_sleep      SET started_at = started_at + (shift_days||' days')::interval,
                                    ended_at   = ended_at   + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE data_health_workout    SET started_at = started_at + (shift_days||' days')::interval,
                                    ended_at   = ended_at   + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE data_location_point    SET occurred_at = occurred_at + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE data_location_visit    SET started_at = started_at + (shift_days||' days')::interval,
                                    ended_at   = ended_at   + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE data_communication_message SET occurred_at = occurred_at + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE data_communication_email   SET occurred_at = occurred_at + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE data_financial_transaction SET occurred_at = occurred_at + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE data_calendar_event    SET started_at = started_at + (shift_days||' days')::interval,
                                    ended_at   = ended_at   + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE data_environment_weather SET occurred_at = occurred_at + (shift_days||' days')::interval,
                                      issued_at   = issued_at   + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE app_chats           SET created_at = created_at + (shift_days||' days')::interval,
                                 updated_at = updated_at + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  UPDATE app_chat_messages   SET created_at = created_at + (shift_days||' days')::interval WHERE id LIKE 'p3y_%%';
  -- `wiki_chapters` carries an EXCLUSION constraint against overlapping
  -- spans, checked per row and not deferrable. An in-place shift overlaps
  -- transiently, and the park-and-land trick used above for `wiki_days` does
  -- not rescue it: the current chapter has `ended_at IS NULL`, so parking it
  -- makes an unbounded range that covers every row still waiting its turn.
  -- Emptying the table first is the only order with no intermediate conflict.
  --
  -- NOTE for whoever touches `demo_reanchor.sql`: it shifts every date column
  -- under `wiki_*` generically and will hit this same wall the moment chapters
  -- are seeded into the set it moves.
  CREATE TEMP TABLE _p3y_ch ON COMMIT DROP AS
    SELECT * FROM wiki_chapters WHERE id LIKE 'p3y_%%';
  DELETE FROM wiki_chapters WHERE id LIKE 'p3y_%%';
  INSERT INTO wiki_chapters (id, kind, title, started_at, ended_at,
                             started_precision, ended_precision, summary,
                             created_at, updated_at)
    SELECT id, kind, title, started_at + shift_days, ended_at + shift_days,
           started_precision, ended_precision, summary, created_at, updated_at
    FROM _p3y_ch;
  UPDATE wiki_stories        SET started_at = started_at + shift_days,
                                 ended_at   = ended_at   + shift_days WHERE id LIKE 'p3y_%%';
  UPDATE wiki_orgs           SET started_at = started_at + shift_days,
                                 ended_at   = ended_at   + shift_days WHERE id LIKE 'p3y_%%' OR id LIKE 'org_demo_%%';
END $$;
"""

RUNSH = """\
#!/bin/sh
# Run the three-year demo seed. Order matters: entities before the rows that
# name them, and the re-anchor strictly last.
#
# These files are NOT compiled into the binary (demo_seed.rs uses include_str!,
# and these are tens of MB). On a demo box, run them with psql.
#
#   DB=virtues sh run.sh
set -eu
DB="${DB:-virtues}"
cd "$(dirname "$0")"
for f in 01_entities.sql 02_streams.sql 03_derived.sql 04_creation.sql 99_reanchor.sql; do
  echo "→ $f"
  psql -v ON_ERROR_STOP=1 -d "$DB" -f "$f" >/dev/null
done
echo "✓ seeded"
"""

HEADER = """\
-- GENERATED by tools/gen-demo-seed.py — do not edit by hand.
-- Regenerate:  python3 tools/gen-demo-seed.py
--
-- Fictional throughout. No real person's name, number, address or account
-- appears here, and none may be added: this repo is public and box context
-- reaches model providers at runtime.
"""

GROUPS = {
    "01_entities": ["wiki_people", "wiki_places", "wiki_orgs", "wiki_chapters",
                    "wiki_stories", "wiki_years", "data_financial_account"],
    "02_streams": ["data_health_heart_rate", "data_health_hrv", "data_health_steps",
                   "data_health_sleep", "data_health_workout", "data_location_point",
                   "data_location_visit", "data_communication_message",
                   "data_communication_email", "data_financial_transaction",
                   "data_calendar_event", "data_environment_weather"],
    "03_derived": ["wiki_days", "wiki_events"],
    "04_creation": ["app_chats", "app_chat_messages", "app_pages", "wiki_articles"],
}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--out", default="virtues-core/seeds/demo3y")
    ap.add_argument("--check-db", metavar="DBNAME",
                    help="validate every column against a live database first")
    args = ap.parse_args()
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    tables, ndays = build()

    # Validate before writing. A seed is raw SQL, so a renamed column is not a
    # compile error anywhere — it is a runtime failure on a box, months later,
    # which is how `wiki_people` in `demo_day.sql` came to be checked by hand.
    if args.check_db:
        import subprocess
        bad = False
        for nm, t in tables.items():
            live = subprocess.run(
                ["psql", "-d", args.check_db, "-tAc",
                 f"select column_name from information_schema.columns "
                 f"where table_name='{nm}'"],
                capture_output=True, text=True).stdout.split()
            if not live:
                print(f"!! {nm}: no such table")
                bad = True
                continue
            missing = [c for c in t.cols if c not in live]
            if missing:
                print(f"!! {nm}: missing {missing}")
                print(f"     available: {', '.join(sorted(live))}")
                bad = True
        if bad:
            raise SystemExit("schema check failed — nothing written")
        print(f"schema check OK against {args.check_db}")

    total = 0
    for group, names in GROUPS.items():
        body = [HEADER]
        for nm in names:
            if nm in tables and tables[nm].rows:
                body.append(f"-- {nm}: {len(tables[nm].rows)} rows")
                body.append(tables[nm].sql())
                total += len(tables[nm].rows)
        (out / f"{group}.sql").write_text("\n".join(body))

    (out / "99_reanchor.sql").write_text(
        HEADER + "\n" + REANCHOR.replace("%ANCHOR%", ANCHOR_END.isoformat()))
    run = out / "run.sh"
    run.write_text(RUNSH)
    run.chmod(0o755)

    print(f"{ndays} days, {START} → {ANCHOR_END}")
    for nm in sorted(tables, key=lambda k: -len(tables[k].rows)):
        if tables[nm].rows:
            print(f"  {len(tables[nm].rows):7d}  {nm}")
    print(f"  {total:7d}  TOTAL")


if __name__ == "__main__":
    main()
