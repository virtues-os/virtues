"""Week plan: an arc for the week, then one detailed plan per day.

Inputs are fiction only: the persona brief, the cast, and fiction/skeleton.json.
"""
import json, os, sys
from concurrent.futures import ThreadPoolExecutor
from llm import chat_json

from work import SRC
sk = json.load(open(f"{SRC}/fiction/skeleton.json"))
cast = json.load(open(f"{SRC}/fiction/cast.json"))

PERSONA = """Jules Arden, 30s, product designer in Austin, TX. Lives at 1847 Selden St (bought
2024 with help from realtor Cora Delgado). Moved from Chicago in late 2023 (Logan Square,
roommate Theo Brandt). Worked at Canopy as a product designer (2024 to Jan 2025), then quit
and went independent in March 2025: the chapter is called "Building the thing". The thing is a
small home server that keeps a person's own record of their life (messages, location, health,
money) and writes it up as day pages and a personal wiki, owned by the person rather than a
company. Jules designs it and builds the app; Tobias Renn does hardware and firmware; Emil Kovács
in Prague supplies enclosures and boards; Nadia Saleh runs the beta-tester thread; Wren
Calloway writes the docs; Lin Ashworth is the beta tester who files real bugs; Priya Haddad is a
collaborator who ships fast. Money is tight but not frightening (runway page: ~11 months).
Partner: Bea Lindqvist (met at the Mueller farmers market, March 2024); she keeps her own place
in Cherrywood ("Bea's"). Friends: the Thursday table hosted by Nell Kovac; Sofia Marchetti (Bea's
oldest friend, cooks); Kofi (Sunday basketball); Tam (neighbour two doors down); Otis (neighbour,
tomatoes). Family in Rockford, IL: Mom (Rosa, Sunday calls), younger sibling Lou, cousin Maeve,
uncle Hal (calls at odd hours), grandmother Birdie. Denver: Orla Byrne (old Chicago friend, now in
Denver). Had a migraine year in 2024–25, mostly sorted by sleep. Lifts at Eastside Barbell, runs.
Workspace: "The workshop", a small rented room a few blocks from home where the hardware lives."""

RULES = """Hard rules:
- Everything is fiction. Use only the people listed in the cast and the places listed in the
  skeleton. Invent no real public figures and no real private individuals.
- Respect the skeleton exactly: where Jules is at each time, how long, when they move, how many
  calendar slots, money slots and audio sessions exist. Fill those slots; do not add or drop any.
- Phone numbers must look like +1512555xxxx, emails @example.com.
- Write like a real life: ordinary, specific, a little messy. No melodrama, no life lessons,
  no product pitch. Small continuities across days (a part that hasn't arrived, a dinner being
  planned, a bug that keeps coming back) are good.
- Never name the weekday or date of another day inside a quote; say "yesterday", "on Monday" is fine."""

def people_list():
    return "\n".join(f'- {p["name"]} ({k}): {p["rel"]}' for k, p in cast["people"].items())

def threads_list():
    return "\n".join(f'- {name}: {", ".join(cast["people"][w]["name"] if w in cast["people"] else w for w in who)}'
                     for _, (name, who) in cast["threads"].items())

SYSTEM = f"""You are writing the fictional life of a demo persona for a personal-data product.
Designers will look at every screen of it, so it has to be complete, coherent across sources,
and believable down to the minute.

PERSONA
{PERSONA}

CAST (key: relationship)
{people_list()}

MESSAGE THREADS
{threads_list()}

{RULES}"""

def compact(d):
    return json.dumps({k: d[k] for k in ("weekday", "stays", "moves", "calendar", "audio", "messages", "money",
                                         "mac_hours", "email", "steps")}, ensure_ascii=False)

ARC_PROMPT = """Here is the skeleton of Jules's week, Saturday to Friday. Saturday begins in Denver
at Orla's: Jules flew up the previous Friday evening and flies home early Saturday morning. Friday
ends with Jules flying back up to Denver and sleeping at Orla's (the same trip, a week later in the
loop — treat the week as one that closes on itself: Friday's trip is the one Saturday comes home from).

Write the week's arc as JSON:
{"why_denver": "...", "threads_of_the_week": ["3-6 small storylines that run across days"],
 "days": [{"weekday": "...", "headline": "a plain 6-10 word line", "outline": "120-180 words: what
 happens, where, with whom, in order, consistent with the stays and moves"}]}

SKELETON
"""

def arc():
    body = "\n".join(compact(d) for d in sk["days"])
    return chat_json(SYSTEM, ARC_PROMPT + body, max_tokens=12000, tag="arc")

DAY_PROMPT = """The week's arc:
{arc}

Now plan {weekday} in full detail. Skeleton for the day:
{skel}

Return JSON:
{{"weekday": "{weekday}",
 "happenings": [{{"time": "HH:MM", "where": "...", "what": "one sentence", "with": ["cast keys"]}}],
 "calendar": [{{"slot": <slot>, "title": "...", "location": "... or null", "attendees": ["cast keys"], "notes": "short or null"}}],
 "money": [{{"slot": <slot>, "merchant": "...", "category": "groceries|coffee|restaurants|transport|travel|shopping|subscription|utilities|transfer|income|hardware|health|fitness|other", "memo": "what it was"}}],
 "audio": [{{"session": <session>, "scene": "who is present, what is said or playing, 1-2 sentences", "speakers": ["Jules", "cast names or 'stranger'"]}}],
 "threads": {{"<thread name>": "what this thread talks about today, 1-3 sentences"}},
 "personal_email": ["one line per personal email received today: sender key + gist"],
 "notes_to_self": "anything Jules jots down today, or null",
 "workout": {{"kind": "strength|run|walk|none", "start": "HH:MM", "minutes": 0, "detail": "..."}}}}

Every calendar slot, money slot and audio session in the skeleton must appear exactly once. Money
amounts are fixed by the skeleton; pick merchants that fit the amount, the place and the time
("in" means money arriving: a refund, a client payment, Bea paying back). Calendar attendees count
must match the skeleton's attendees number where possible."""

def day(d, A):
    prompt = DAY_PROMPT.format(arc=json.dumps(A, ensure_ascii=False), weekday=d["weekday"], skel=compact(d))
    return chat_json(SYSTEM, prompt, max_tokens=20000, tag=f"day{d['day']}")

if __name__ == "__main__":
    A = arc()
    json.dump(A, open(f"{SRC}/fiction/arc.json", "w"), indent=1, ensure_ascii=False)
    print(json.dumps(A, indent=1, ensure_ascii=False)[:4000])
    with ThreadPoolExecutor(7) as ex:
        plans = list(ex.map(lambda d: day(d, A), sk["days"]))
    json.dump(plans, open(f"{SRC}/fiction/plans.json", "w"), indent=1, ensure_ascii=False)
    for d, p in zip(sk["days"], plans):
        miss = {"cal": len(d["calendar"]) - len(p.get("calendar", [])), "money": len(d["money"]) - len(p.get("money", [])),
                "audio": len(d["audio"]) - len(p.get("audio", []))}
        print(d["weekday"], miss)
