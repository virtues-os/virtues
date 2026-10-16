"""Emit the fictional week as SQL for the demo box.

Usage: python3 emit.py [--today YYYY-MM-DD]   -> fiction/week.sql

Ring placement: week day d (0 = Saturday) lands on the most recent date <= today
with its own weekday, so weekdays are always true and today is always full.
Every id is `wk_...`; every row is fiction. Visits/points/streams use the same
source_table/provider/metadata shapes the real importers write.
"""
import json, math, os, random, sys, uuid
from datetime import date, datetime, timedelta, timezone
from zoneinfo import ZoneInfo
from collections import defaultdict

from work import SRC
R = random.Random(42)
TZ = ZoneInfo("America/Chicago")
DAY, H = 86400, 3600
s = json.load(open(f"{SRC}/shape.json"))
loc = json.load(open(f"{SRC}/fiction/location.json"))
cast = json.load(open(f"{SRC}/fiction/cast.json"))
plans = json.load(open(f"{SRC}/fiction/plans.json"))
C = f"{SRC}/fiction/content"
msgs = json.load(open(f"{C}/messages.json"))
emails = json.load(open(f"{C}/emails.json"))
trans = json.load(open(f"{C}/transcripts.json"))
pools = json.load(open(f"{C}/pools.json"))
skel = json.load(open(f"{SRC}/fiction/skeleton.json"))
PEOPLE = cast["people"]

today = date.fromisoformat(sys.argv[sys.argv.index("--today") + 1]) if "--today" in sys.argv else date.today()
SAT = 5  # Python weekday() of Saturday
def landing(d):
    wd = (SAT + d) % 7
    return today - timedelta(days=(today.weekday() - wd) % 7)
LAND = [landing(d) for d in range(7)]

def at(t):
    """week-seconds -> aware UTC datetime on the ring."""
    d = max(0, min(6, int(t // DAY)))
    extra = t - d * DAY
    base = datetime.combine(LAND[d], datetime.min.time(), TZ)
    return (base + timedelta(seconds=extra)).astimezone(timezone.utc)

def U(*parts):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, "wk/" + "/".join(map(str, parts)))).upper()

def q(v):
    if v is None: return "NULL"
    if isinstance(v, bool): return "TRUE" if v else "FALSE"
    if isinstance(v, (int, float)): return repr(round(v, 7)) if isinstance(v, float) else str(v)
    if isinstance(v, datetime): return f"'{v.isoformat()}'"
    if isinstance(v, date): return f"'{v.isoformat()}'"
    if isinstance(v, (dict, list)): return "'" + json.dumps(v, ensure_ascii=False).replace("'", "''") + "'::jsonb"
    return "'" + str(v).replace("'", "''") + "'"

tables = defaultdict(lambda: {"cols": None, "rows": []})
def add(table, **row):
    T = tables[table]
    if T["cols"] is None: T["cols"] = list(row)
    assert list(row) == T["cols"], (table, list(row), T["cols"])
    T["rows"].append(row)

# ------------------------------------------------------------------ places + location
for key, (pid, name, la, lo) in loc["places"].items():
    if pid.startswith("wk_"):
        add("wiki_places", id=pid, name=name, category={"den": "transit", "den_rail": "transit", "union": "venue",
            "orla": "residence"}[key], address={"den": "Denver, CO", "den_rail": "Denver International Airport, CO",
            "union": "Wynkoop St, Denver, CO", "orla": "Highland, Denver, CO"}[key], latitude=la, longitude=lo, radius_m=120.0)

def activity(speed):
    if speed is None or speed < 0.4: return "stationary"
    if speed < 2.2: return "walking"
    if speed < 4.5: return "running"
    if speed < 80: return "automotive"
    return "unknown"
for i, p in enumerate(loc["points"]):
    add("data_location_point", id=f"wk_pt_{i:05d}", latitude=round(p["lat"], 7), longitude=round(p["lon"], 7),
        altitude=p["alt"], horizontal_accuracy=p["acc"], vertical_accuracy=round(R.uniform(2, 8), 2),
        occurred_at=at(p["t"]), source_stream_id=U("loc", i).lower(), source_table="stream_ios_location",
        source_provider="ios", metadata={"activity_type": activity(p["speed"]), "activity_confidence": "high"},
        speed=p["speed"], course=-1.0)
for i, v in enumerate(loc["visits"]):
    t0, t1 = v["t0"], min(v["t1"], 7 * DAY - 1)
    if t1 - t0 < 300: continue
    add("data_location_visit", id=f"wk_vis_{i:03d}", place_name=None, latitude=v["lat"], longitude=v["lon"],
        started_at=at(t0), ended_at=at(t1), duration_minutes=int((t1 - t0) // 60),
        source_stream_id=f"wk_vis_{i:03d}", source_table="location_point", source_provider="ios",
        metadata={"timeline": "v1", "point_count": sum(1 for p in loc["points"] if t0 <= p["t"] <= t1), "radius_meters": 60})

# ------------------------------------------------------------------ messages
ME_PHONE = cast["me"]["phone"]
def thread_id(tok):
    name, who = cast["threads"][tok]
    if who == ["svc"]: return None
    if len(who) == 1: return f"iMessage;-;{PEOPLE[who[0]]['phone']}"
    return f"iMessage;+;chat{abs(hash(tok)) % 10**9:09d}"
REACT = {"Loved": "2000", "Liked": "2001", "Disliked": "2002", "Laughed": "2003", "Emphasized": "2004", "Questioned": "2005"}
svc = cast["sms_services"]
for i, m in enumerate(s["messages"]):
    body = msgs.get(str(i), "")
    name, who = cast["threads"][m["thread"]]
    group = len(who) > 1
    if who == ["svc"]:
        sname, sid = svc[int((m.get("sender") or "x.0").split(".")[-1]) % len(svc)]
        frm, fname, tid = sid, sname, f"SMS;-;{sid}"
    elif m["me"]:
        frm, fname, tid = "me", None, thread_id(m["thread"])
    else:
        k = int((m.get("sender") or "x.0").split(".")[-1]) % len(who)
        frm, fname, tid = PEOPLE[who[k]]["phone"], PEOPLE[who[k]]["name"], thread_id(m["thread"])
    rt = next((v for k, v in REACT.items() if body.startswith(k)), None) if m["reaction"] else None
    to = [ME_PHONE] if not m["me"] else ([PEOPLE[w]["phone"] for w in who] if who != ["svc"] else [])
    add("data_communication_message", id=f"wk_msg_{i:04d}", message_id=U("msg", i), thread_id=tid, channel="imessage",
        body=body, from_identifier=frm, from_name=fname, to_identifiers=to, is_read=True, is_group_message=group,
        reply_to_message_id=None, has_attachments=bool(m["att"]), occurred_at=at(m["t"]),
        source_stream_id=U("msgs", i), source_table="mac_imessage", source_provider="mac",
        metadata={"service": "SMS" if who == ["svc"] else "iMessage", "is_from_me": m["me"],
                  "group_title": name if group else None, "reaction_type": rt,
                  "attachment_count": 1 if m["att"] else 0,
                  "attachments": ([{"mime_type": "image/heic" if m["media"] == "image" else "video/quicktime" if m["media"] == "video" else "application/pdf"}] if m["att"] else [])},
        # The owner's rows and short codes carry an empty handle, as the Mac
        # importer writes them; "people who messaged you" counts handles.
        from_handle="" if (m["me"] or who == ["svc"]) else frm)

# ------------------------------------------------------------------ email
people = cast["email_people"]
for i, e in enumerate(s["emails"]):
    c = emails.get(str(i))
    if not c: continue
    if "service" in c:
        frm, fname = c["service_email"], c["service"]
        to, toname, direction = ["jules@example.com"], ["Jules Arden"], "received"
    else:
        pk = c.get("person") if c.get("person") in PEOPLE else R.choice(people)
        if e["dir"] == "sent":
            frm, fname, to, toname, direction = "jules@example.com", "Jules Arden", [PEOPLE[pk]["email"]], [PEOPLE[pk]["name"]], "sent"
        else:
            frm, fname, to, toname, direction = PEOPLE[pk]["email"], PEOPLE[pk]["name"], ["jules@example.com"], ["Jules Arden"], "received"
    labels = e["labels"] or ["INBOX"]
    tid = "wk_" + (c.get("thread_hint") or f"t{i}")[:40]
    add("data_communication_email", id=f"wk_em_{i:03d}", message_id=f"<{U('em', i).lower()}@mail.example.com>", thread_id=tid,
        subject=c.get("subject"), body=c.get("body"), body_preview=(c.get("body") or "")[:180], from_email=frm, from_name=fname,
        to_emails=to, to_names=toname, cc_emails=[], bcc_emails=[], direction=direction, is_read=e["read"],
        is_starred=e["star"], has_attachments=e["att"], labels=labels, occurred_at=at(e["t"]),
        source_stream_id=U("em", i).replace("-", "")[:16].lower(), source_table="google_gmail", source_provider="google",
        metadata={"gmail_id": U("em", i).replace("-", "")[:16].lower(), "label_ids": labels, "thread_id": tid})

# ------------------------------------------------------------------ audio
rec_by_t = {}
for i, r in enumerate(s["recordings"]):
    sid = U("rec", i)
    rec_by_t.setdefault(r["t0"], []).append(sid)
    add("data_audio_recording", id=f"wk_rec_{i:04d}", source_stream_id=sid, started_at=at(r["t0"]),
        ended_at=at(r["t1"] or r["t0"] + (r["dur"] or 60)), duration_seconds=r["dur"], audio_url=None,
        audio_format="m4a", is_silent=bool(r["silent"]), average_db_level=r["db"], source_table="stream_ios_microphone",
        source_provider="ios", metadata={})
used = set()
for i, x in enumerate(s["transcripts"]):
    cands = [sid for sid in rec_by_t.get(x["t0"], []) if sid not in used]
    sid = cands[0] if cands else U("tr", i)
    used.add(sid)
    c = trans.get(str(i), {}) if x["cls"] != "silent" else {}
    text = c.get("text", "") if x["cls"] != "silent" else ""
    add("data_communication_transcription", id=f"wk_tr_{i:04d}", audio_url=None, text=text, language="en",
        duration_seconds=x["dur"], started_at=at(x["t0"]), ended_at=at(x["t1"] or x["t0"] + (x["dur"] or 60)),
        speaker_count=x["speakers"] or (1 if text else 0), title=c.get("title"), summary=None,
        confidence=x["conf"] or 0.9, tags=c.get("tags", []), entities={}, source_stream_id=sid,
        source_table="stream_ios_microphone", source_provider="ios", metadata={"scene": x["cls"]})
for i, a in enumerate(s["audio_sessions"]):
    inside = [trans.get(str(j), {}).get("text", "") for j, x in enumerate(s["transcripts"])
              if a["t0"] - 30 <= x["t0"] <= a["t1"] + 30 and x["cls"] != "silent"]
    add("data_audio_session", id=f"wk_as_{i:03d}", started_at=at(a["t0"]), ended_at=at(a["t1"]),
        speaker_mode=a["mode"] if a["mode"] is not None else 0, avg_db=a["db"], chunk_count=a["chunks"] or len(inside),
        content="\n".join(t for t in inside if t) or None, source_table="data_audio_session", source_provider="ios")

# ------------------------------------------------------------------ mac apps + web
apps_pool, web_pool = pools["apps"], pools["web"]
dev = "dev_" + U("mac").replace("-", "")[:16].lower()
title_state = {}
def app_title(app, t):
    pool = apps_pool.get(app) or [app]
    k = (app, t // 1800)
    if k not in title_state:
        title_state[k] = pool[R.randrange(len(pool))]
    return title_state[k]
for i, a in enumerate(s["app_sessions"]):
    if a["app"] in cast["drop_apps"]: continue
    app = cast["app_map"].get(a["app"], a["app"])
    if app == "Claude" and 9 * H <= a["t0"] % DAY <= 18 * H and (a["t0"] // 1200) % 2 == 0:
        app = "Figma"
    if app == "Dia":
        dom = R.choice(list(web_pool)[:12]); pg = R.choice(web_pool[dom]); title = pg["title"]
    else:
        title = app_title(app, a["t0"])
    t1 = a["t1"] or a["t0"] + 5
    bundle = cast["bundles"].get(app, f"com.example.{app.lower().replace(' ', '')}")
    add("data_activity_app_session", id=f"wk_app_{i:04d}", app_name=app, app_bundle_id=bundle,
        started_at=at(a["t0"]), ended_at=at(t1), window_title=title, source_stream_id=f"{dev}:{bundle}:{int(at(a['t0']).timestamp())}",
        source_table="mac_apps", source_provider="mac", metadata={"titles": [title]}, device_id=dev,
        attention=a["attention"] or "active", is_open=bool(a["open"]), closed_by=a["closed_by"])
priv = json.load(open(f"{SRC}/cast_private.json"))
dmap = priv["domain_map"]
last = {}
for i, w in enumerate(s["web"]):
    dom = dmap.get(w["domain"]) or "google.com"
    pool = web_pool.get(dom) or web_pool.get("google.com") or [{"url": f"https://{dom}/", "title": dom}]
    # dwell on a page: repeat the previous page for the same domain half the time
    pg = last[dom] if dom in last and R.random() < 0.45 else pool[R.randrange(len(pool))]
    last[dom] = pg
    add("data_activity_web_browsing", id=f"wk_web_{i:04d}", url=pg["url"], domain=dom, page_title=pg["title"],
        occurred_at=at(w["t"]), source_stream_id=U("web", i).lower(), source_table="mac_browser", source_provider="mac",
        metadata={"browser": "dia"})
del dmap, priv

# ------------------------------------------------------------------ calendar
cal = skel["calendar"]
cal_plan = {}
for d, p in enumerate(plans):
    for c in p.get("calendar", []):
        cal_plan[c["slot"]] = c
for i, c in enumerate(cal):
    pc = cal_plan.get(i, {})
    att = [PEOPLE[k]["email"] for k in pc.get("attendees", []) if k in PEOPLE]
    add("data_calendar_event", id=f"wk_cal_{i:02d}", title=pc.get("title") or "Busy", description=pc.get("notes"),
        calendar_name="Work" if att else "Jules", status="confirmed", response_status="accepted",
        organizer_identifier="jules@example.com", attendee_identifiers=(["jules@example.com"] + att) if att else [],
        location_name=pc.get("location"), started_at=at(c["t0"]), ended_at=at(c["t1"]), is_all_day=c["all_day"],
        is_sacred=False, source_stream_id=U("cal", i).lower(), source_table="stream_ios_eventkit", source_provider="ios",
        external_id=U("calx", i), metadata={"calendar_id": "work" if att else "personal"}, calendar_access_role="owner")

# ------------------------------------------------------------------ money
mplan = {}
for p in plans:
    for m in p.get("money", []):
        mplan[m["slot"]] = m
for i, x in enumerate(s["transactions"]):
    m = mplan.get(i, {"merchant": "Card purchase", "category": "other", "memo": None})
    cat = m.get("category", "other")
    acct = "p3y_acct_check" if cat in ("transfer", "income", "utilities") or "mortgage" in (m.get("memo") or "").lower() \
        or abs(x["amount"]) > 100000 else "p3y_acct_card"
    amount = abs(int(round(x["amount"]))) * (-1 if x["amount"] < 0 else 1)
    add("data_financial_transaction", id=f"wk_tx_{i:03d}", account_id=acct, transaction_id=U("tx", i), amount=amount,
        currency="USD", merchant_name=m["merchant"], merchant_category=cat, description=(m.get("memo") or m["merchant"]),
        category=[cat.title()], is_pending=bool(x["pending"]), transaction_type="credit" if amount < 0 else "debit",
        payment_channel=x["channel"] or ("online" if cat in ("subscription", "transfer", "income") else "in store"),
        occurred_at=at(x["t"]), source_stream_id=U("txs", i).lower(), source_table="stream_ios_financekit",
        source_provider="apple_finance", metadata={})

# ------------------------------------------------------------------ health
for i, x in enumerate(s["steps"]):
    add("data_health_steps", id=f"wk_st_{i:03d}", step_count=int(x["v"]), occurred_at=at(x["t"]),
        source_stream_id=U("st", i).lower(), source_table="stream_ios_healthkit", source_provider="ios", metadata={})
for i, x in enumerate(s["distance"]):
    add("data_health_distance", id=f"wk_di_{i:03d}", meters=x["v"], occurred_at=at(x["t"]),
        source_stream_id=U("di", i).lower(), source_table="stream_ios_healthkit", source_provider="ios", metadata={})

# Activity windows from the fiction: workouts, moves, sleep.
WORKOUTS = []
for m in loc["moves"]:
    if m["mode"] == "run":
        WORKOUTS.append(("running", m["t0"], m["t1"]))
    if m["mode"] == "walk" and m["t1"] - m["t0"] > 45 * 60:
        WORKOUTS.append(("walking", m["t0"], m["t1"]))
for v in loc["visits"]:
    if v["place"] == "gym":
        WORKOUTS.append(("traditional_strength_training", v["t0"] + 5 * 60, v["t1"] - 4 * 60))
WORKOUTS.sort(key=lambda w: w[1])

# Sleep: the longest quiet stretch at home/Bea's/Orla's each night (no message by me, no app use).
busy = sorted([m["t"] for m in s["messages"] if m["me"]] + [a["t0"] for a in s["app_sessions"]] +
              [p["t"] for p in loc["points"] if (p["speed"] or 0) > 1.5])
SLEEPS = []
for d in range(7):
    lo, hi = d * DAY - 4 * H, d * DAY + 11 * H
    ev = [t for t in busy if lo <= t <= hi]
    if d == 0:  # Fri night at Orla's, short: up at 04:40 for the flight
        SLEEPS.append((d * DAY - 70 * 60, d * DAY + 4 * H + 40 * 60)); continue
    gaps = [(b - a, a, b) for a, b in zip(ev, ev[1:])]
    g = max(gaps) if gaps else (0, d * DAY, d * DAY + 7 * H)
    SLEEPS.append((g[1] + 10 * 60, g[2] - 5 * 60))
# Friday night (into the next Saturday) is at Orla's: write it on day 6, before midnight only.
SLEEPS.append((6 * DAY + 23 * H + 50 * 60, 7 * DAY - 1))

def state(t):
    for k, a, b in WORKOUTS:
        if a <= t <= b: return k
    for a, b in SLEEPS:
        if a <= t <= b: return "sleep"
    for m in loc["moves"]:
        if m["t0"] <= t <= m["t1"]: return m["mode"]
    return "rest"
BPM = {"running": (148, 9), "walking": (98, 7), "traditional_strength_training": (118, 14), "sleep": (54, 3),
       "rest": (68, 5), "foot": (92, 6), "car": (74, 4), "flight": (78, 4), "run": (150, 8), "walk": (96, 6)}
t, i = -2 * H, 0
hr_rows = []
while t < 7 * DAY:
    st = state(t)
    if t >= 0 and not (st == "rest" and R.random() < 0.15):
        mu, sd = BPM.get(st, (70, 5))
        hour = (t % DAY) // H
        if st == "rest" and 18 <= hour <= 22: mu += 6
        bpm = int(R.gauss(mu, sd))
        hr_rows.append((t, bpm, st))
        add("data_health_heart_rate", id=f"wk_hr_{i:05d}", bpm=bpm, occurred_at=at(t), source_stream_id=U("hr", i).lower(),
            source_table="stream_ios_healthkit", source_provider="ios",
            metadata={"measurement_context": "active" if st in ("running", "walking", "traditional_strength_training") else "background"})
        i += 1
    t += 15 if st in ("running", "traditional_strength_training", "walking") else (600 if st == "sleep" else R.randint(240, 420))
for j, (a, b) in enumerate(SLEEPS):
    if b - a < 2 * H: continue
    mins = int((b - a) // 60)
    stages, left = [], mins
    while left > 0:
        for stg, lo_, hi_ in (("asleep_core", 25, 55), ("asleep_deep", 10, 35), ("asleep_core", 10, 30), ("asleep_rem", 10, 30)):
            n = min(left, R.randint(lo_, hi_)); stages.append({"stage": stg, "duration_minutes": n}); left -= n
            if left <= 0: break
        if left > 30 and R.random() < 0.35:
            n = R.randint(2, 9); stages.append({"stage": "awake", "duration_minutes": n}); left -= n
    asleep = sum(x["duration_minutes"] for x in stages if x["stage"] != "awake")
    add("data_health_sleep", id=f"wk_sl_{j}", started_at=at(max(0, a)) if a >= 0 else at(0) - timedelta(seconds=-a),
        ended_at=at(b), duration_minutes=asleep, sleep_quality_score=round(min(0.97, asleep / 480), 2),
        sleep_stages=stages, source_stream_id=U("sl", j).lower(), source_table="stream_ios_healthkit",
        source_provider="ios", metadata={})
    for k in range(3):
        add("data_health_hrv", id=f"wk_hrv_{j}_{k}", hrv_ms=round(R.gauss(52 if j != 0 else 38, 8), 1),
            occurred_at=at(a + (k + 1) * (b - a) // 4), source_stream_id=U("hrv", j, k).lower(),
            source_table="stream_ios_healthkit", source_provider="ios", metadata={})
for j, (k, a, b) in enumerate(WORKOUTS):
    hs = [bpm for t_, bpm, st in hr_rows if a <= t_ <= b] or [120]
    dist = None
    if k in ("running", "walking"):
        pts = [p for p in loc["points"] if a <= p["t"] <= b]
        dist = round(sum(math.hypot((q_["lat"] - p_["lat"]) * 111.32, (q_["lon"] - p_["lon"]) * 96.2)
                         for p_, q_ in zip(pts, pts[1:])), 2)
    add("data_health_workout", id=f"wk_wo_{j}", workout_type=k, started_at=at(a), ended_at=at(b),
        duration_minutes=int((b - a) // 60), calories_burned=int((b - a) / 60 * {"running": 11, "walking": 4.5}.get(k, 7)),
        distance_km=dist, avg_heart_rate=int(sum(hs) / len(hs)), max_heart_rate=max(hs),
        source_stream_id=U("wo", j).lower(), source_table="stream_ios_healthkit", source_provider="ios", metadata={})
for h_ in range(7 * 24):
    t = h_ * H
    st = state(t + 1800)
    kcal = {"running": 520, "traditional_strength_training": 330, "walking": 210, "sleep": 2, "rest": 18,
            "foot": 120, "car": 8, "flight": 6}.get(st, 15) * R.uniform(0.7, 1.2)
    add("data_health_active_energy", id=f"wk_ae_{h_:03d}", kcal=round(kcal, 1), occurred_at=at(t + 3599),
        source_stream_id=U("ae", h_).lower(), source_table="stream_ios_healthkit", source_provider="ios", metadata={})

# ------------------------------------------------------------------ write
order = ["wiki_places", "data_location_point", "data_location_visit", "data_communication_message",
         "data_communication_email", "data_audio_recording", "data_communication_transcription", "data_audio_session",
         "data_activity_app_session", "data_activity_web_browsing", "data_calendar_event", "data_financial_transaction",
         "data_health_steps", "data_health_distance", "data_health_heart_rate", "data_health_hrv", "data_health_sleep",
         "data_health_workout", "data_health_active_energy"]
out = [f"-- Demo week (fiction). Generated {datetime.now(timezone.utc):%Y-%m-%d}. Ring placed for {today}.",
       "BEGIN;"]
for tname in order:
    T = tables[tname]
    if not T["rows"]: continue
    cols = T["cols"]
    for k in range(0, len(T["rows"]), 400):
        chunk = T["rows"][k:k + 400]
        vals = ",\n".join("(" + ", ".join(q(r[c]) for c in cols) + ")" for r in chunk)
        out.append(f"INSERT INTO {tname} ({', '.join(cols)}) VALUES\n{vals}\nON CONFLICT (id) DO NOTHING;")
out.append("COMMIT;")
open(f"{SRC}/fiction/week.sql", "w").write("\n".join(out) + "\n")
print("landing", [d.isoformat() for d in LAND])
print({k: len(v["rows"]) for k, v in tables.items()})
