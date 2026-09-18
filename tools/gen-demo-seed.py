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
    # The second ring: people a life actually contains and a demo usually
    # lacks — neighbours, trades, the doctor, the people you only see in one
    # context. A cast of six makes every list look like a test fixture.
    ("p3y_sofia", "Sofia Marchetti", "friend", "moderate", 470, DAYS, 4, "Bea's oldest friend. Cooks like it's a sport."),
    ("p3y_abe", "Abe Ferreira", "professional", "weak", 520, DAYS, 2, "The plumber. Twice, memorably."),
    ("p3y_wren", "Wren Calloway", "colleague", "moderate", 760, DAYS, 4, "Writes the docs. Argues about commas, correctly."),
    ("p3y_otis", "Otis Bramble", "friend", "weak", 330, DAYS, 3, "Neighbour with the loud truck and the good tomatoes."),
    ("p3y_maeve", "Maeve Sullivan", "family", "moderate", 0, DAYS, 3, "Cousin. Christmas and crises."),
    ("p3y_dr_park", "Dr. Ellen Park", "professional", "weak", 560, DAYS, 2, "GP. The one who asked about sleep first."),
    ("p3y_tobias", "Tobias Renn", "colleague", "moderate", 900, DAYS, 4, "Hardware. Patient about firmware."),
    ("p3y_pia", "Pia Halloran", "friend", "moderate", 640, DAYS, 3, "Book club, loosely defined."),
    ("p3y_hector", "Hector Salas", "professional", "weak", 600, DAYS, 2, "Bike shop. Knows the creak."),
    ("p3y_lin", "Lin Ashworth", "colleague", "weak", 1000, DAYS, 3, "First beta tester who filed a real bug."),
    ("p3y_greta", "Greta Voss", "friend", "weak", 250, 780, 3, "Climbing partner until the shoulder."),
    ("p3y_ray", "Ray Okonjo", "family", "weak", 0, DAYS, 2, "Uncle Hal's brother. Once a year."),
    ("p3y_juno", "Juno Ellery", "colleague", "moderate", 840, DAYS, 3, "Illustrator. Fast and unhurried at once."),
    ("p3y_stef", "Stef Nowak", "friend", "moderate", 180, DAYS, 3, "Austin. Met in a queue, stayed a friend."),
    ("p3y_bram", "Bram Teague", "professional", "weak", 700, DAYS, 2, "Accountant's associate. Emails only."),
    ("p3y_nyla", "Nyla Osei", "friend", "moderate", 540, DAYS, 4, "Walt's sister. Better company than Walt."),
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
    ("p3y_pl_taqueria", "Vera's Taqueria", "restaurant", 0.0061, 0.0034, "E 1st St, Austin, TX"),
    ("p3y_pl_bookshop", "Bell & Marrow Books", "shop", 0.0038, -0.0072, "S 1st St, Austin, TX"),
    ("p3y_pl_barton", "Barton Springs", "outdoors", -0.0091, -0.0136, "Austin, TX"),
    ("p3y_pl_hardware", "Cobb Hardware", "shop", 0.0079, 0.0051, "Manor Rd, Austin, TX"),
    ("p3y_pl_dentist", "Hyde Park Dental", "health", 0.0141, -0.0087, "Austin, TX"),
    ("p3y_pl_coffee2", "Radio Coffee", "cafe", -0.0044, -0.0121, "S 1st St, Austin, TX"),
    ("p3y_pl_venue", "The Parish", "venue", 0.0031, -0.0104, "E 6th St, Austin, TX"),
    ("p3y_pl_studio", "The workshop", "work", 0.0102, 0.0038, "Austin, TX"),
    ("p3y_pl_pool", "Deep Eddy", "outdoors", -0.0027, -0.0188, "Austin, TX"),
    ("p3y_pl_sofia", "Sofia's", "residence", 0.0118, 0.0094, "Austin, TX"),
    ("p3y_pl_parents", "Rockford", "travel", 11.99, 8.79, "Rockford, IL"),
    ("p3y_pl_chi_bar", "The Owl", "venue", 11.61, 10.11, "Logan Square, Chicago, IL"),
    ("p3y_pl_chi_climb", "Brooklyn Boulders", "gym", 11.63, 10.15, "Chicago, IL"),
    ("p3y_pl_denver", "Denver", "travel", 9.46, -7.28, "Denver, CO"),
    ("p3y_pl_conf", "Web Summit", "venue", 8.44, 88.58, "Lisboa, Portugal"),
    ("p3y_pl_lake", "Lake Travis", "outdoors", 0.1204, -0.2311, "Travis County, TX"),
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
    ("p3y_org_dental", "Hyde Park Dental", "clinic", "provider", None, None, None),
    ("p3y_org_club", "Bell & Marrow Reading Group", "club", "member", None, date(2024, 9, 5), None),
    ("p3y_org_client", "Kestrel Labs", "company", "client", "Design lead (contract)",
     date(2025, 3, 10), date(2025, 9, 30)),
    ("p3y_org_utility", "Austin Energy", "utility", "customer", None, None, None),
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
RUN_STARTED = D(2024, 9, 2)           # takes up running; Tue/Thu from here on.
# RUN_STARTED exists because the local-novelty case DID NOT WORK without it.
# `RAN_INSTEAD` is meant to be an ordinary KIND of event sitting at the far edge
# of its own cluster — but running happened on that one day and nowhere else, so
# across 1,095 days the corpus held exactly one run and one kayak trip. An
# isolated point in the embedding space scores high LOF, which is why BOTH
# showcase events pinned at the 3.00 local clamp and the global/local
# distinction could not be shown at all. A local outlier needs a cluster to be
# an outlier IN.
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
    # The rooms nothing had ever seeded. `id` is omitted on the three identity
    # tables (notebook items, notes, memories) — Postgres generates it.
    notebooks = tbl("app_notebooks", ["id", "name", "icon", "accent_color", "sort_order",
                                      "instructions", "auto_add_materials",
                                      "created_at", "updated_at"])
    nbitems = tbl("app_notebook_items", ["notebook_id", "url", "sort_order", "added_at",
                                         "role", "added_by"])
    notes = tbl("wiki_notes", ["subject_type", "subject_id", "kind", "body", "author",
                               "created_at", "source_refs", "resolved_at", "resolution",
                               "resolved_by"])
    memories = tbl("app_assistant_memories", ["lane", "body", "author", "created_at",
                                              "updated_at", "retired_at", "retired_reason"])
    marks = tbl("data_content_bookmark", ["id", "url", "title", "description",
                                          "source_platform", "bookmark_type", "author",
                                          "tags", "occurred_at", "source_stream_id",
                                          "source_table", "source_provider", "note",
                                          "enrichment_status"])
    docs = tbl("data_content_document", ["id", "title", "content", "content_summary",
                                         "document_type", "tags", "is_authored",
                                         "occurred_at", "source_stream_id", "source_table",
                                         "source_provider"])
    web = tbl("data_activity_web_browsing", ["id", "url", "domain", "page_title",
                                             "occurred_at", "source_stream_id",
                                             "source_table", "source_provider"])
    applets = tbl("app_applets", ["id", "name", "owner", "agent", "schedule", "enabled",
                                  "config", "triggers", "description", "created_at",
                                  "updated_at"])
    runs = tbl("app_applet_runs", ["id", "applet_id", "status", "started_at", "completed_at",
                                   "records_processed", "trigger", "result_summary", "error",
                                   "message", "created_at"])

    # ---- entities -------------------------------------------------------
    # `blurb`, not `notes` — the latter is the wiki_notes table three hundred
    # lines down, and binding it here shadowed it into a string.
    for pid, name, rel, bond, _first, _last, w, blurb in PEOPLE:
        handle = name.split()[0].lower()
        people.add(pid, name, [f"{handle}@example.com"],
                   [PHONE[pid]], rel, None, blurb, {"weight": w}, [], bond)

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

        # One dict per day, filled by each stream writer and read by the event
        # writer last. The summaries have to be composed from what the day
        # actually holds: the first version drew them from a five-item list, so
        # 3,257 events carried 13 distinct strings between them, the embedding
        # space had 13 points in it, and `build_lof_model` correctly refused to
        # score local novelty at all ("no spread in outlierness").
        ctx = {}
        _day_health(d, off, hr, hrv, steps, sleep, workout, migraine, instrumented, weekend, ctx)
        _day_location(d, off, lpoint, lvisit, instrumented, lisbon, chi, weekend, ctx)
        _day_comms(d, off, msg, email, lisbon, ctx)
        _day_money(d, off, txn, lisbon, chi, ctx)
        _day_weather(d, off, wx, lisbon, ctx)
        _day_calendar(d, off, cal, weekend, chi, ctx)
        _day_events(d, off, did, events, migraine, instrumented, lisbon, chi, weekend, ctx)
        _day_web(d, off, web, chi)
        n += 1

    facts = compute_facts(t)
    _chats(chats, cmsg, facts)
    _pages_and_articles(pages, articles, facts)
    _notebooks(notebooks, nbitems)
    _user_pages(pages)
    _notes_and_memories(notes, memories)
    _saves_and_docs(marks, docs)
    _applets(applets, runs)

    return t, n


# --------------------------------------------------------------------------
# Per-day stream writers
# --------------------------------------------------------------------------


def _day_health(d, off, hr, hrv, steps, sleep, workout, migraine, instrumented, weekend, ctx):
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
    hrv_val = round(base + RNG.uniform(-4, 4), 1)
    hrv.add(f"p3y_hrv_{k}", hrv_val, ts(d, 7, 12),
            f"p3y_s_hrv_{k}", "data_health_hrv", "demo")
    ctx["hrv"] = hrv_val
    ctx["sleep_min"] = dur

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
    ctx["steps"] = steps_total
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
    elif not migraine and d.weekday() in (1, 3) and off > RUN_STARTED:
        # The ordinary run: Tue/Thu, 4–6.5 km, easy effort. RAN_INSTEAD is a
        # Wednesday — a LIFTING day — at 9.1 km and 161 bpm, so it stays the
        # same kind of thing while sitting well outside this cluster's shape.
        w = ("running", RNG.randint(25, 42), RNG.randint(240, 420),
             round(RNG.uniform(4.0, 6.5), 1), RNG.randint(142, 156),
             RNG.randint(158, 172))
    elif weekend and RNG.random() < 0.45:
        w = ("walking", RNG.randint(28, 71), RNG.randint(90, 210), round(RNG.uniform(2.1, 5.8), 1),
             RNG.randint(92, 108), RNG.randint(110, 126))
    else:
        return
    start = ts(d, 17 if not weekend else 9, RNG.randint(0, 40))
    workout.add(f"p3y_wk_{k}", w[0], start, start + timedelta(minutes=w[1]), w[1], w[2], w[3],
                w[4], w[5], f"p3y_s_wk_{k}", "data_health_workout", "demo")
    ctx["workout"] = w


def _day_location(d, off, lpoint, lvisit, instrumented, lisbon, chi, weekend, ctx):
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

    ctx["stops"] = [nm for nm, *_ in stops]
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


def _day_comms(d, off, msg, email, lisbon, ctx):
    k = d.isoformat()
    spoke = []
    # Contact frequency decays for the Chicago half and rises for the Austin
    # half, so "who have I lost touch with" is answerable from counts alone.
    live = [p for p in PEOPLE if p[4] <= off <= p[5]]
    count = RNG.randint(1, 3) if lisbon else RNG.randint(2, 7)
    for i in range(count):
        pid, name, *_rest = RNG.choices(live, weights=[p[6] for p in live])[0]
        if name not in spoke:
            spoke.append(name)
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

    ctx["spoke"] = spoke
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


def _day_money(d, off, txn, lisbon, chi, ctx):
    k = d.isoformat()
    bought = []
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
        bought.append((merchant, amount))
        ctx["bought"] = bought
        txn.add(f"p3y_tx_{k}_{i}", "p3y_acct_card", f"p3y_txid_{k}_{i}",
                amount, cur, merchant, cat, merchant.upper(),
                ["Shops", cat.title()], False, "debit",
                "online" if RNG.random() < 0.3 else "in store",
                ts(d, RNG.randint(8, 21), RNG.randint(0, 59)),
                f"p3y_s_tx_{k}_{i}", "data_financial_transaction", "demo")


def _day_weather(d, off, wx, lisbon, ctx):
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
    ctx["weather"] = (hi, lo, rain)


def _day_calendar(d, off, cal, weekend, chi, ctx):
    k = d.isoformat()
    ctx["meetings"] = []
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
        ctx["meetings"].append(title)
        cal.add(f"p3y_cal_{k}_{i}", title, None, "Work" if chi or h < 17 else "Personal",
                "confirmed", [], where, ts(d, h, 0), ts(d, h + 1, 0), False,
                title.startswith("Dinner"), f"p3y_s_cal_{k}_{i}",
                "data_calendar_event", "demo")


def _day_events(d, off, did, events, migraine, instrumented, lisbon, chi, weekend, ctx):
    """The derived layer, composed from what the day actually holds.

    ## The lede rule

    A day's events are not a roll-up of its slots. The day's headline event
    leads with the one thing that separates this day from the one before it —
    the reason it asks to be remembered — and numbers appear only where they
    are the evidence for that reason, never as a tally.

    This is mechanical before it is aesthetic. `dayline/novelty.rs` embeds
    `wiki_events.event_summary` verbatim, so the PROSE is the novelty signal.
    The first version of this generator gave every day the same three
    sentences with the digits swapped — "N minutes of X, D km, average H bpm"
    — and the skeleton carried more signal than the content: a walk and a
    kayak trip measured 0.705 cosine apart, against 0.374 for the same two
    activities in plain language. Local novelty is a LOF z-score, LOF needs
    variance in outlierness, and a templated corpus has none. So vary the
    sentence, not only the numbers in it.

    ## Step counts never appear

    A step count is the least distinguishing thing a day holds, and in the
    first version it closed 1,068 of 1,095 days — in the last clause, the
    position that should carry the point. The ONE exception is a migraine day,
    where a low count is evidence of a day spent in the dark rather than a
    score.

    ## A quiet day still emits four events

    `MIN_EVENTS_TO_NARRATE` in `api/day_summary.rs` is 4: below it the box does
    not narrate the day at all — no LLM call, no day article — and Chapters,
    Years and Lifeline are all built on narration. So an unremarkable day keeps
    its four events. It simply stops writing four ledes for them: one event
    carries the day, and the rest name a particular (a place, a person, a
    purchase, the weather) instead of reciting the same metrics.
    """
    k = d.isoformat()
    seq = 0

    def ev(h0, h1, label, where, onts, summary, kind="stay", conf="high",
           topics=None, entities=None):
        nonlocal seq
        events.add(f"p3y_ev_{k}_{seq}", did, ts(d, h0, 0), ts(d, h1, 0), label, where,
                   onts, topics or [], entities or [], summary, kind, conf)
        seq += 1

    # --- the particulars this day actually holds --------------------------
    w = ctx.get("workout")
    mins = ctx.get("sleep_min", 0)
    hrv_v = ctx.get("hrv")
    sleep_txt = f"{mins // 60}h{mins % 60:02d}"
    people = ctx.get("spoke", [])
    away = [p for p in ctx.get("stops", []) if p != "Home"]
    meetings = ctx.get("meetings", [])
    bought = ctx.get("bought", [])
    hi, lo, rain = ctx.get("weather", (21.0, 12.0, 0.0))

    def money(n=1):
        # The Lisbon rows are written in EUR (`_day_money`), so the symbol has
        # to follow the data. The lede literally says "everything in euros";
        # printing dollars underneath it is the seed contradicting itself, and
        # this demo's whole claim is that the prose stands on the record.
        sym = "\u20ac" if lisbon else "$"
        return ", ".join(f"{sym}{a / 100:.2f} at {m}" for m, a in bought[:n])

    def who(n=3):
        p = people[:n]
        if not p:
            return ""
        if len(p) == 1:
            return p[0]
        return ", ".join(p[:-1]) + " and " + p[-1]

    def sky():
        """Weather, but only when it did something. Otherwise silence — a
        temperature nobody noticed is the same padding as a step count."""
        if rain > 8:
            return RNG.choice(["Rain all day, the heavy kind.",
                               "It rained like it meant it.",
                               "Heavy rain from the morning on."])
        if rain > 1:
            return RNG.choice(["Wet underfoot.", "Rain on and off.", "Grey and wet."])
        if hi > 34:
            return RNG.choice([f"{hi:.0f}°C and nowhere to put it.",
                               f"{hi:.0f}°C, the kind that ends plans."])
        if lo < 2:
            return RNG.choice([f"Down to {lo:.0f}°C overnight.",
                               f"A hard frost, {lo:.0f}°C before dawn."])
        return ""

    def workout_line():
        """Plain language first, the figure second and only when it carries
        something. Kind-specific frames so two different activities do not
        come out as the same sentence with different digits."""
        kind, mn, _cal, km, avg, peak = w
        if kind == "strength_training":
            return RNG.choice([
                f"The usual barbell hour, {mn} minutes of it.",
                f"Lifting. Nothing in it that {mn} minutes does not explain.",
                f"Back under the bar for {mn} minutes.",
            ])
        if kind == "walking":
            return RNG.choice([
                f"A walk that turned into {km} km without meaning to.",
                f"Out on foot, {km} km of it, mostly to be outside.",
                f"Walked for {mn} minutes and thought about very little.",
            ])
        if kind == "running":
            return RNG.choice([
                f"Ran {km} km, and felt every one of them.",
                f"{km} km on the road, holding {avg}.",
                f"An easy {km} km before dinner.",
                f"Out for {mn} minutes; {km} km of it, nothing forced.",
                f"{km} km, the legs willing for once.",
                f"A steady {km} km at {avg}.",
            ])
        return f"{km} km of {kind.replace('_', ' ')}."

    # ------------------------------------------------------------------
    # The lede. Scripted singular days first, then whatever this day's own
    # data makes unusual, then nothing in particular — and a day with nothing
    # in particular says so, briefly, rather than manufacturing a headline.
    #
    # `spent` is what the lede has already used. A particular belongs to ONE
    # sentence: a lede that has already named the day's single purchase and a
    # supporting event that names it again is the padding this rewrite exists
    # to remove, and it reads worse than the template did.
    # ------------------------------------------------------------------
    spent = set()

    if migraine:
        late = next((f"${a / 100:.2f} at {m}" for m, a in bought if m == "Jo's"), None)
        ev(7, 12, "A migraine took the morning", "Home",
           ["data_health_heart_rate", "data_health_hrv", "data_health_sleep"],
           f"Migraine. {sleep_txt} behind it and HRV at {hrv_v}, the lowest in weeks "
           f"— then low light and no screen until the afternoon. "
           f"{ctx.get('steps', 0):,} steps for the whole day, which is what a day "
           f"spent in the dark looks like."
           + (f" A late coffee the night before, {late}." if late else ""),
           conf="high", topics=["health"])
        ev(12, 22, "It lifted by the afternoon", "Home",
           ["data_location_visit", "data_health_steps"],
           RNG.choice([f"Enough to eat by evening. Nothing attempted on a "
                       f"{d.strftime('%A')}, and nothing missed.",
                       f"Lifted enough to sit up, {sleep_txt} of sleep still "
                       f"owed. The calendar was empty anyway.",
                       f"By four it had gone. Nothing on, and nothing wanted."]),
           conf="medium")
        ev(22, 23, "Early night", "Home", ["data_health_sleep"],
           "In bed before ten.", kind="sleep", conf="medium")
        ev(0, 7, "Asleep", "Home", ["data_health_sleep", "data_health_hrv"],
           f"{sleep_txt}, and not much of it good.", kind="sleep", conf="high")
        return

    if lisbon:
        spend = money(2)
        ev(8, 20, "Lisbon", "Lisbon",
           ["data_location_visit", "data_financial_transaction", "data_environment_weather"],
           RNG.choice([
               "A day of walking a city on foot and getting lost in it on purpose.",
               "Out from morning until the light went, and none of it planned.",
               "Lisbon, at the pace of somewhere you do not have to be anywhere.",
           ]) + (f" Everything in euros — {spend}." if spend else " Everything in euros."),
           conf="medium", topics=["travel"])
        ev(20, 22, "The walk back", "Lisbon", ["data_location_visit"],
           RNG.choice(["The long way back along the water.",
                       "Back on foot, the river on one side the whole way.",
                       "A slow walk home in the last of the heat."]), conf="medium")
        ev(22, 23, "Messages home", "Lisbon", ["data_communication_message"],
           f"A few words to {who(2)}." if people else "Nothing said to anyone.",
           conf="medium")
        ev(0, 7, "Asleep", "Lisbon", ["data_health_sleep"],
           f"{sleep_txt}, on a mattress with opinions.", kind="sleep", conf="high")
        return

    if off == FIRST_KAYAK:
        ev(9, 12, "First time on the water", "Lady Bird Lake",
           ["data_health_workout", "data_location_visit", "data_location_point"],
           f"First time in a kayak. Three hours out and {w[3]} km of it, and the "
           f"arms knew about it long before the lake ended. Nothing else in three "
           f"years looks like this.",
           conf="high", topics=["outdoors", "first"])
    elif off == RAN_INSTEAD:
        ev(17, 18, "Ran instead of lifting", "Mueller Trails",
           ["data_health_workout", "data_health_heart_rate"],
           f"Ran, on an evening that is normally a barbell and forty minutes "
           f"indoors. {w[3]} km at {w[4]}, and no decision behind it worth the name.",
           conf="high", topics=["fitness"])
    elif off == GHOST_EVENT:
        ev(15, 16, None, None, ["data_calendar_event"],
           "A calendar block — “Coffee — J.” — with nothing behind it. "
           "No location, no messages, no spend in the hour either side. I cannot say "
           "whether it happened.", kind="unknown", conf="low")
    elif rain > 8:
        # Every frame has to name something only this day holds — rainfall, a
        # place, a meeting. A frame with no slot in it repeats verbatim across
        # hundreds of days, which is the degenerate embedding space again.
        if meetings:
            spent.add("meeting")
            held = f" {meetings[0]} went ahead anyway."
        elif bought:
            spent.add("money")
            held = f" {money(1)} was the whole of it."
        elif people:
            held = f" {who(1)} had the same weather."
        else:
            held = ""
        ev(9, 17, "Rain shut the day in", away[0] if away else "Home",
           ["data_environment_weather", "data_location_visit"],
           RNG.choice([
               f"Rain from before dawn \u2014 {rain:.0f} mm of it \u2014 and it never "
               f"let up.{held}",
               f"{rain:.0f} mm since the night before. Whatever the day was going "
               f"to be, it was this instead.{held}",
               f"It rained hard enough \u2014 {rain:.0f} mm \u2014 to make the day a "
               f"small one.{held}",
           ]), conf="high")
    elif hi > 34:
        ev(12, 18, "Too hot to be outside", "Home",
           ["data_environment_weather"],
           f"{hi:.0f}°C by noon, which ended any argument about going out.",
           conf="high")
    elif len(meetings) >= 3:
        ev(9, 17, "A day that was all calendar", "Office" if chi else "Home",
           ["data_calendar_event", "data_activity_app_session"],
           f"{len(meetings)} things back to back — {', '.join(meetings[:2])} "
           f"— and nothing between them long enough to be called work.",
           conf="high")
    elif weekend and away:
        ev(10, 15, away[0], away[0],
           ["data_location_visit", "data_financial_transaction"],
           RNG.choice([f"{away[0]}, without much of a reason.",
                       f"Out to {away[0]} and no further.",
                       f"{away[0]} in the middle of the day."])
           + (f" {money(1)}." if bought else ""), conf="medium")
        if bought:
            spent.add("money")
    elif w and w[0] == "walking":
        ev(9, 11, "A long walk", away[0] if away else "Mueller",
           ["data_health_workout"], workout_line(), conf="high", topics=["outdoors"])
    else:
        # Nothing asked to be remembered. Say that, and say it short — but a
        # quiet day is still a PARTICULAR quiet day, and the line has to carry
        # whichever particular it has, or 700 of these come out identical.
        dayname = d.strftime("%A")
        if meetings:
            spent.add("meeting")
            tail = f" {meetings[0]}, and nothing after it."
        elif bought:
            spent.add("money")
            tail = f" {money(1)}, which was the day's one decision."
        elif people:
            tail = f" {who(1)} the only voice in it."
        else:
            tail = ""
        ev(9, 17, "A quiet one", "Office" if chi else "Home",
           ["data_location_visit", "data_calendar_event"],
           RNG.choice([f"A {dayname} that left nothing behind.",
                       f"Work, and not much else worth the name on a {dayname}.",
                       f"Nothing in this {dayname} the next one would not repeat.",
                       f"An ordinary {dayname}, start to finish."])
           + tail + (f" {sky()}" if sky() else ""), conf="high")

    # ------------------------------------------------------------------
    # Supporting events. Each names a particular, not a metric. They exist so
    # the day clears MIN_EVENTS_TO_NARRATE with things that are true, not so
    # every slot gets a sentence.
    # ------------------------------------------------------------------

    ev(0, 7, "Asleep", "Home", ["data_health_sleep", "data_health_hrv"],
       RNG.choice([f"{sleep_txt}, straight through.",
                   f"Down early, {sleep_txt} of it.",
                   f"{sleep_txt}, and awake before the alarm."])
       if mins >= 392 else
       RNG.choice([f"A short night — {sleep_txt}, and HRV down at {hrv_v}.",
                   f"{sleep_txt}, which was not enough and showed by eleven."]),
       kind="sleep", conf="high")

    if off not in (FIRST_KAYAK, RAN_INSTEAD) and w and not (w[0] == "walking" and away):
        ev(17, 18, w[0].replace("_", " ").title(),
           away[-1] if away else "Home", ["data_health_workout"],
           workout_line(), conf="high", topics=["fitness"])

    if meetings and len(meetings) < 3 and "meeting" not in spent:
        ev(11, 12, meetings[0], "Office" if chi else "Home",
           ["data_calendar_event"],
           RNG.choice([f"{meetings[0]}, and it ran short.",
                       f"{meetings[0]}. Half an hour, and settled.",
                       f"One thing on the calendar: {meetings[0]}."]), conf="high")
    elif bought and "money" not in spent:
        ev(13, 14, "Out briefly", away[0] if away else "Mueller",
           ["data_financial_transaction", "data_location_visit"],
           RNG.choice([f"{money(1)}, and back.",
                       f"Out as far as {bought[0][0]} and no further.",
                       f"{money(1)}. The only reason to leave the house."]),
           conf="medium")
    elif away:
        ev(13, 14, away[0], away[0], ["data_location_visit"],
           RNG.choice([f"An hour at {away[0]}.",
                       f"{away[0]}, and not long there.",
                       f"Out to {away[0]} and straight back."]), conf="medium")
    elif people:
        ev(13, 14, "Midday", "Office" if chi else "Home",
           ["data_communication_message"],
           RNG.choice([f"{who(1)} halfway through the afternoon.",
                       f"A message from {who(1)} and nothing that needed answering."]),
           conf="medium")
    else:
        # Something has to stand here: below MIN_EVENTS_TO_NARRATE the box does
        # not narrate the day at all, and the lede having eaten this day's only
        # particular is not a reason to lose the day from Chapters and Years.
        ev(13, 14, "Midday", "Office" if chi else "Home",
           ["data_location_visit", "data_activity_app_session"],
           RNG.choice([f"The middle of a {d.strftime('%A')} and nothing in it.",
                       f"Straight through the middle of the day without stopping.",
                       f"A {d.strftime('%A')} afternoon that asked for nothing."]),
           conf="low")

    if people:
        ev(18, 23, "Evening", "Home",
           ["data_location_visit", "data_communication_message"],
           RNG.choice([f"Messages back and forth with {who(2)}.",
                       f"{who(1)} texting about nothing in particular.",
                       f"In for the night, {who(2)} on and off."]), conf="medium")
    else:
        ev(18, 23, "Evening", "Home", ["data_location_visit"],
           RNG.choice([f"Home by six on a {d.strftime('%A')}, and nobody needed "
                       f"anything.",
                       f"An evening with no one in it, {d.strftime('%B')} being "
                       f"the kind of month for that.",
                       f"In, and quiet with it. {sky()}".strip()]), conf="medium")


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
             f"{f['run_n']} {'run' if f['run_n'] == 1 else 'runs'}, so there is no prior "
             f"event of that kind to compare it against.\n\nEverything else about the day "
             f"was ordinary.\n\nI'm not going to tell you whether it was a good day. "
             f"That one's yours."),
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
        cid = f"chat_p3y_{i:02d}"
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
        pid = f"page_p3y_{i:02d}"
        pages.add(pid, title, content, None, [], "article", now, now)
        articles.add(f"p3y_art_{i:02d}", subject_type, subject_id, pid, now, "auto", [], [])

    for off in range(DAYS - 120, DAYS):
        d = START + timedelta(days=off)
        if in_outage(off):
            continue
        did = f"day_{d.isoformat()}"
        pid = f"page_p3y_day_{d.isoformat()}"
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
                                 updated_at = updated_at + (shift_days||' days')::interval WHERE id LIKE 'chat_p3y_%%';
  UPDATE app_chat_messages   SET created_at = created_at + (shift_days||' days')::interval WHERE id LIKE 'chat_p3y_%%';
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
for f in 01_entities.sql 02_streams.sql 03_derived.sql 04_creation.sql 05_content.sql 99_reanchor.sql; do
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

def _day_web(d, off, web, chi):
    k = d.isoformat()
    for i in range(RNG.randint(1, 4)):
        dom, title = RNG.choice([
            ("news.ycombinator.com", "Hacker News"),
            ("en.wikipedia.org", RNG.choice(["Kayak", "Subsidiarity", "Lisbon", "Migraine",
                                             "Local outlier factor", "Jacques Jannon"])),
            ("github.com", RNG.choice(["sqlx", "llama.cpp", "svelte", "pgvector"])),
            ("apartments.com" if chi else "austinmonthly.com", "Listings"),
            ("kagi.com", "Search"),
            ("bell-and-marrow.example.com", "Bell & Marrow Books"),
        ])
        web.add(f"p3y_web_{k}_{i}", f"https://{dom}/", dom, title,
                ts(d, RNG.randint(8, 23), RNG.randint(0, 59)),
                f"p3y_s_web_{k}_{i}", "data_activity_web_browsing", "demo")


NOTEBOOKS = [
    # (id, name, icon, accent, instructions, auto_add, [(url, role)])
    ("p3y_nb_house", "The Selden St house", "\U0001F3E1", "#7A5C3E",
     "Everything about buying and keeping this house. Inspection notes are the "
     "important part; the rest is sentiment.", True,
     [("/place/place_demo_home", "pin"), ("/person/person_demo_rachel", "library"),
      ("/page/p3y_up_inspection", "manuscript"), ("/page/p3y_up_closing", "library")]),
    ("p3y_nb_migraine", "Migraines", "\U0001FA7A", "#8C3B3B",
     "Track what precedes one. Do not speculate about causes — collect, then look.",
     True,
     [("/chat/chat_p3y_00", "pin"), ("/page/p3y_up_triggers", "manuscript"),
      ("/person/p3y_dr_park", "library")]),
    ("p3y_nb_lisbon", "Lisbon", "\u2708\uFE0F", "#3E6B7A", None, False,
     [("/place/p3y_pl_lisbon", "pin"), ("/page/p3y_up_packing", "library"),
      ("/place/p3y_pl_conf", "library")]),
    ("p3y_nb_independent", "Going independent", "\U0001F5DD\uFE0F", "#4A5D3A",
     "The decision, the runway maths, and who I told in what order.", True,
     [("/org/org_demo_employer", "library"), ("/org/p3y_org_client", "library"),
      ("/page/p3y_up_runway", "manuscript"), ("/chat/chat_p3y_03", "pin")]),
    ("p3y_nb_chicago", "Chicago, before", "\U0001F5C3\uFE0F", "#5A5A6B", None, False,
     [("/person/p3y_theo", "pin"), ("/person/p3y_junie", "library"),
      ("/place/p3y_pl_chicago_apt", "library"), ("/org/p3y_org_agency", "library")]),
    ("p3y_nb_reading", "Reading", "\U0001F4DA", "#6B5A3E",
     "Books and long pieces only. Links to tools go somewhere else.", True,
     [("/page/p3y_up_reading", "manuscript"), ("/person/p3y_pia", "library")]),
]


def _notebooks(notebooks, nbitems):
    """Six notebooks with their children — the room has never had any data.

    A notebook item is a URL into the record (`/person/`, `/place/`, `/org/`,
    `/page/`, `/chat/`), so the children are real refs rather than copies, and
    the roles exercise all three: `pin` is the spine, `manuscript` is the thing
    being written, `library` is everything gathered around it.
    """
    now = ts(ANCHOR_END, 9, 0)
    for i, (nid, name, icon, accent, instr, auto, items) in enumerate(NOTEBOOKS):
        made = ts(START + timedelta(days=400 + i * 90), 10, 0)
        notebooks.add(nid, name, icon, accent, i, instr, auto, made, now)
        for j, (url, role) in enumerate(items):
            # Some of it gathered by hand, some pulled in by the magnet — a
            # notebook where every row says "user" hides half the feature.
            by = "user" if role != "library" or j % 3 else "magnet"
            nbitems.add(nid, url, j, made + timedelta(days=j), role, by)


USER_PAGES = [
    ("p3y_up_inspection", "Inspection — 1847 Selden", "\U0001F50D",
     "Roof: 4-6 years left, seller won't budge. Foundation: two hairlines, both "
     "old, [@Cora Delgado](/person/person_demo_rachel) says they're settlement not "
     "movement and the report agrees.\n\nHVAC is the real number — 2009, and the "
     "inspector's face did a thing when he said it.\n\n**Walk away number: 5%.** "
     "Wrote it down so I'd hold to it. Held to it."),
    ("p3y_up_triggers", "What precedes a migraine", "\U0001F4C9",
     "Keeping this by hand because the server can't see everything.\n\n- Short "
     "sleep — every time, no exceptions yet\n- Late coffee — most times\n- Bright "
     "afternoon, especially on the water\n- NOT stress, as far as I can tell. The "
     "worst weeks of the Canopy thing had none.\n\n[@Dr. Ellen Park]"
     "(/person/p3y_dr_park) asked about sleep before anything else, which I "
     "thought was a stock question and turned out not to be."),
    ("p3y_up_runway", "Runway", "\U0001F4B0",
     "Savings covers 11 months at the current burn, 7 if the health insurance goes "
     "the way [@Sam Whitlock](/person/p3y_sam_whitlock) thinks.\n\nKestrel "
     "contract would cover 4 of those on its own.\n\nThe honest version: I am not "
     "doing this because the maths works. The maths merely doesn't forbid it."),
    ("p3y_up_packing", "Lisbon list", "\U0001F9F3",
     "Adapter. The good shoes, not the comfortable ones — eleven days is long "
     "enough to regret either.\n\nTalk is 20 minutes with 5 for questions. "
     "Rehearse on the plane, not in the room."),
    ("p3y_up_closing", "Closing day", "\U0001F511",
     "Wire went out 9:40, cleared 2:15, keys 4:30. [@Cora Delgado]"
     "(/person/person_demo_rachel) brought bread and salt, which she said is a "
     "thing her mother did.\n\nSlept on the floor of the front room because the "
     "bed didn't come until Tuesday. Best night's sleep in a year."),
    ("p3y_up_reading", "Reading, 2025", "\U0001F4D6",
     "**Finished:** Middlemarch (again, properly this time) · The Peregrine · "
     "A Pattern Language, in pieces\n\n**Abandoned:** two books about "
     "productivity, without regret\n\n**Next:** whatever [@Pia Halloran]"
     "(/person/p3y_pia) brings on Thursday"),
    ("p3y_up_names", "Names for the thing", "\u2234",
     "Ruled out: anything with *self*, anything with *mind*, anything a search "
     "engine already owns.\n\nWhat I keep coming back to is that the argument is "
     "about custody, not intelligence. Name the virtue, not the machine."),
    ("p3y_up_garden", "The bed by the fence", "\U0001F33F",
     "Tomatoes from [@Otis Bramble](/person/p3y_otis) — the ones that taste like "
     "something. Basil died twice; third time in the shade and it lived.\n\n"
     "Note for next spring: the fence side gets four hours, not six. Plan for "
     "four."),
    ("p3y_up_letter", "Letter to Theo, unsent", "\u2709\uFE0F",
     "Drafted this three times over two years and never sent any of them.\n\n"
     "[@Theo Brandt](/person/p3y_theo) — there was no falling out, which is "
     "somehow the harder thing to write about. We just stopped, and I moved, and "
     "the stopping got easier than the starting would have been.\n\nKeeping it "
     "here rather than sending it, which I recognise is the coward's archive."),
    ("p3y_up_workshop", "Workshop setup", "\U0001F527",
     "Bench height 92cm — measured off the one at [@The agency]"
     "(/org/p3y_org_agency) that never hurt my back.\n\nPower on the wall side, "
     "not the floor. Learned that twice."),
]


def _user_pages(pages):
    """Pages a person wrote, as opposed to articles the box wrote.

    Entity refs are inline markdown — `[@Label](/person/id)` — because that is
    how the app stores them; there is no link table. So these also give the
    backlink panels something real to resolve.
    """
    for i, (pid, title, icon, content) in enumerate(USER_PAGES):
        made = ts(START + timedelta(days=430 + i * 55), 21, 30)
        pages.add(pid, title, content, icon, [], "page", made,
                  made + timedelta(days=RNG.randint(0, 40)))


def _notes_and_memories(notes, memories):
    """Marginalia on the record, and what the assistant has been told to keep.

    Deliberately not all resolved and not all from the same author: an open
    correction beside an accepted one is the only way the room shows its own
    states.
    """
    now = ts(ANCHOR_END, 9, 0)
    rows = [
        ("person", "p3y_sam_ortiz", "correction",
         "This is not the same Sam as the accountant. Two different people, and "
         "the record has merged them at least once.", "human", None, None, None),
        ("person", "p3y_sam_whitlock", "provenance",
         "Only ever appears in email, never in messages — so the interaction "
         "count is low but the relationship is not.", "ai", None, None, None),
        ("day", None, "observation",
         "Nothing reached the server for nine days here. The gap is the device, "
         "not the life.", "ai", now, "accepted", "human"),
        ("place", "place_demo_home", "memo",
         "Bought in August. Everything before that date at this address is the "
         "showings, not living here.", "human", None, None, None),
        ("person", "p3y_theo", "appraisal",
         "The drop in contact lines up with the move rather than with anything "
         "between you, as far as the record shows.", "ai", now, "absorbed", "ai"),
        ("chapter", "p3y_ch_unknown", "memo",
         "I have not named this stretch and I would rather it stayed unnamed "
         "than got a tidy label.", "human", None, None, None),
        ("organization", "org_demo_employer", "correction",
         "End date is the last day worked, not the last day paid.", "human",
         now, "accepted", "human"),
        ("year", "year_2025", "style_note",
         "Write this one plainly. It does not need a shape put on it yet.",
         "human", None, None, None),
        ("story", "p3y_st_migraines", "observation",
         "Seven episodes in the record. Short sleep precedes every one; late "
         "caffeine precedes four.", "ai", None, None, None),
        ("person", "p3y_bea", "style_note",
         "Do not summarise her. Quote what was actually said or leave it out.",
         "human", None, None, None),
    ]
    # `wiki_notes_machine_must_cite`: an AI-authored note must carry at least
    # one source ref. A person may assert; the machine may only point. Good
    # rule, and the reason these carry real record URLs rather than an empty
    # array — a note the box wrote with nothing behind it is exactly the thing
    # the constraint exists to forbid.
    cites = {
        "p3y_sam_whitlock": ["/record/data_communication_email/p3y_em_2024-06-14"],
        "p3y_theo": ["/person/p3y_theo",
                     "/record/data_communication_message/p3y_ms_2024-01-07_0"],
        "p3y_st_migraines": ["/record/data_health_sleep/p3y_sl_2025-06-11",
                             "/record/data_health_hrv/p3y_hrv_2025-06-11"],
    }
    for st, sid, kind, body, author, res_at, res, res_by in rows:
        if st == "day" and sid is None:
            sid = f"day_{(START + timedelta(days=OUTAGE[0])).isoformat()}"
        refs = cites.get(sid, [])
        if author == "ai" and not refs:
            refs = [f"/record/wiki_days/day_"
                    f"{(START + timedelta(days=OUTAGE[0])).isoformat()}"]
        notes.add(st, sid, kind, body, author, now, refs, res_at, res, res_by)

    mem = [
        ("facts", "Lives at 1847 Selden St, Austin. Bought August 2024.", "ai", None, None),
        ("facts", "Bea is the partner. Do not call her a friend.", "human", None, None),
        ("facts", "Left Canopy in January 2025; independent since.", "ai", None, None),
        ("facts", "Two Sams. Ortiz is the friend, Whitlock is the accountant.",
         "human", None, None),
        ("manner", "Short answers. I will ask if I want more.", "human", None, None),
        ("manner", "Never open with a compliment.", "human", None, None),
        ("manner", "If the record does not say, say that it does not say.", "human", None, None),
        ("manner", "American spelling.", "human", None, None),
        ("practices", "Sunday evening is for the week ahead. Do not schedule then.",
         "human", None, None),
        ("practices", "Lift Monday, Wednesday, Friday, evenings.", "ai", None, None),
        ("practices", "No screens after the migraine starts — do not offer to read "
         "anything aloud either.", "human", None, None),
        ("practices", "Used to run every morning before the move.", "ai",
         ts(ANCHOR_END - timedelta(days=200), 9, 0), "no longer true"),
    ]
    for lane, body, author, retired, reason in mem:
        memories.add(lane, body, author, now, now, retired, reason)


def _saves_and_docs(marks, docs):
    """Saves and documents, across every enrichment state the rooms render.

    Uniformly-enriched saves hide the states a real box spends most of its time
    in, which is the same reason `demo_bookmarks.sql` is deliberately uneven.
    """
    saves = [
        ("How to read a home inspection report", "housebuying.example.com", "article", "done"),
        ("Local Outlier Factor, explained", "stats.example.com", "article", "done"),
        ("The Peregrine — J.A. Baker", "bell-and-marrow.example.com", "book", "done"),
        ("Bench heights and back pain", "woodworking.example.com", "article", "pending"),
        ("Lisbon in eleven days", "travel.example.com", "article", "done"),
        ("llama.cpp server flags", "github.example.com", "repo", "done"),
        ("Migraine triggers: what the evidence says", "health.example.com",
         "article", "failed"),
        ("A Pattern Language, notes", "arch.example.com", "article", "skipped"),
        ("Tomato varieties for shade", "garden.example.com", "article", "done"),
        ("Austin Energy rate schedule", "austinenergy.example.com", "document", "pending"),
        ("Kayak rentals, Lady Bird Lake", "atx.example.com", "listing", "done"),
        ("On keeping a daybook", "essays.example.com", "essay", "done"),
    ]
    for i, (title, dom, btype, status) in enumerate(saves):
        when = ts(START + timedelta(days=200 + i * 68), 22, 10)
        marks.add(f"p3y_bm_{i:02d}", f"https://{dom}/{i}", title, None, dom, btype,
                  None, [], when, f"p3y_s_bm_{i:02d}", "data_content_bookmark",
                  "demo", None, status)

    papers = [
        ("Closing statement — 1847 Selden St", "contract", False),
        ("Kestrel Labs — statement of work", "contract", False),
        ("Canopy separation letter", "letter", False),
        ("Lisbon — conference talk notes", "notes", True),
        ("Inspection report, 1847 Selden St", "report", False),
        ("Homeowner's insurance policy", "policy", False),
        ("Tax return, prior year", "tax", False),
        ("Bench plans", "notes", True),
    ]
    for i, (title, dtype, authored) in enumerate(papers):
        when = ts(START + timedelta(days=380 + i * 84), 14, 0)
        docs.add(f"p3y_doc_{i:02d}", title, None,
                 "Filed from the Drive folder.", dtype, [], authored, when,
                 f"p3y_s_doc_{i:02d}", "data_content_document", "demo")


def _applets(applets, runs):
    """Applets and their run history, including the runs that did not succeed.

    ("Applets" is the shipping name — it was renamed to "Routines" and back on
    2026-09-16; see agents/build/voice.md.)
    """
    now = ts(ANCHOR_END, 9, 0)
    defs = [
        ("p3y_ap_day", "Write yesterday", "0 4 * * *",
         "Segments the day and writes it up, every morning at four."),
        ("p3y_ap_wiki", "Keep the articles current", "0 5 * * *",
         "Revisits any subject whose evidence changed."),
        ("p3y_ap_finance", "Pull transactions", "0 */6 * * *",
         "Six-hourly sync from the bank."),
        ("p3y_ap_weekly", "Sunday letter", "0 18 * * 0",
         "A short account of the week, in the evening."),
        ("p3y_ap_garden", "Frost watch", "0 20 * * *",
         "Tells me the night before, between November and March, and says nothing "
         "otherwise."),
    ]
    for aid, name, sched, desc in defs:
        applets.add(aid, name, "user", None, sched, True, {}, [], desc,
                    ts(START + timedelta(days=500), 9, 0), now)

    statuses = (["success"] * 16) + ["error", "skipped", "budget_exceeded", "cancelled"]
    n = 0
    for aid, *_ in defs:
        for back in range(40):
            st = RNG.choice(statuses)
            start = ts(ANCHOR_END - timedelta(days=back), 4, RNG.randint(0, 40))
            runs.add(f"p3y_run_{n:04d}", aid, st, start,
                     start + timedelta(minutes=RNG.randint(1, 26)),
                     RNG.randint(0, 900) if st == "success" else 0, "cron",
                     {"success": "Done.",
                      "error": None,
                      "skipped": "Nothing to do — no new evidence.",
                      "budget_exceeded": "Stopped at the ceiling.",
                      "cancelled": None}[st],
                     "model call failed after 3 attempts" if st == "error" else None,
                     None, start)
            n += 1


GROUPS = {
    "01_entities": ["wiki_people", "wiki_places", "wiki_orgs", "wiki_chapters",
                    "wiki_stories", "wiki_years", "data_financial_account"],
    "02_streams": ["data_health_heart_rate", "data_health_hrv", "data_health_steps",
                   "data_health_sleep", "data_health_workout", "data_location_point",
                   "data_location_visit", "data_communication_message",
                   "data_communication_email", "data_financial_transaction",
                   "data_calendar_event", "data_environment_weather"],
    "03_derived": ["wiki_days", "wiki_events"],
    "04_creation": ["app_chats", "app_chat_messages", "app_pages", "wiki_articles",
                    "app_notebooks", "app_notebook_items", "wiki_notes",
                    "app_assistant_memories", "app_applets", "app_applet_runs"],
    "05_content": ["data_content_bookmark", "data_content_document",
                   "data_activity_web_browsing"],
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
