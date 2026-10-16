"""Extract the SHAPE of one real week from virtues_real into shape.json.

Lives outside the repo on purpose: shape.json holds real timing, real place
geometry (for clustering only) and real domains. It never goes to git and never
leaves this Mac. Text bodies are never read into the output — only lengths.

Usage: uv run --with 'psycopg[binary]' extract.py
"""
import json, math, re
from datetime import datetime, timezone, timedelta
import psycopg
from work import SRC

DB = "dbname=virtues_real"
W0 = datetime(2026, 9, 26, 5, 0, tzinfo=timezone.utc)   # Sat 00:00 CDT
W1 = W0 + timedelta(days=7)
OUT = f"{SRC}/shape.json"

def t(ts):
    return None if ts is None else round((ts - W0).total_seconds())

def hav(a, b, c, d):
    p = math.radians
    x = math.sin(p(c - a) / 2) ** 2 + math.cos(p(a)) * math.cos(p(c)) * math.sin(p(d - b) / 2) ** 2
    return 6371000 * 2 * math.asin(math.sqrt(x))

conn = psycopg.connect(DB)
q = lambda sql, *a: conn.execute(sql, a).fetchall()
shape = {"week_start_utc": W0.isoformat(), "tz": "America/Chicago"}

# ---------------------------------------------------------------- places
visits = q("""select latitude, longitude, started_at, ended_at from data_location_visit
              where ended_at > %s and started_at < %s order by started_at""", W0, W1)
clusters = []   # [lat, lon, [visit idx]]
vis_out = []
for i, (la, lo, s, e) in enumerate(visits):
    for c in clusters:
        if hav(la, lo, c[0], c[1]) < 150:
            c[2].append(i); break
    else:
        clusters.append([la, lo, [i]])

def dwell(c):
    return sum((visits[i][3] - visits[i][2]).total_seconds() for i in c[2])
clusters.sort(key=dwell, reverse=True)
home = clusters[0]
places = []
for n, c in enumerate(clusters):
    tok = f"P{n+1:02d}"
    night = day = 0.0
    for i in c[2]:
        s, e = visits[i][2], visits[i][3]
        h = s
        while h < e:
            step = min(e, h + timedelta(minutes=15))
            lh = (h - timedelta(hours=5)).hour
            secs = (step - h).total_seconds()
            if lh < 6: night += secs
            elif 9 <= lh < 18: day += secs
            h = step
    places.append({"tok": tok, "lat": c[0], "lon": c[1], "visits": len(c[2]),
                   "hours": round(dwell(c) / 3600, 2), "night_h": round(night / 3600, 2),
                   "day_h": round(day / 3600, 2),
                   "km_from_home": round(hav(c[0], c[1], home[0], home[1]) / 1000, 2)})
    for i in c[2]:
        vis_out.append({"place": tok, "t0": t(visits[i][2]), "t1": t(visits[i][3])})
vis_out.sort(key=lambda v: v["t0"])
shape["places"] = places
shape["visits"] = vis_out

# ---------------------------------------------------------------- points
pts = q("""select occurred_at, latitude, longitude, horizontal_accuracy, speed, altitude, course
           from data_location_point where occurred_at >= %s and occurred_at < %s order by occurred_at""", W0, W1)

def place_at(ts):
    for v, (la, lo, s, e) in zip(range(len(visits)), visits):
        if s <= ts <= e:
            return v
    return None

vis_tok = {}
for v in vis_out:
    vis_tok[(v["t0"], v["t1"])] = v["place"]
ptok = {p["tok"]: p for p in places}
out_pts, seg, segs = [], None, []
for ts, la, lo, acc, sp, alt, crs in pts:
    vi = place_at(ts)
    if vi is not None:
        tok = vis_tok[(t(visits[vi][2]), t(visits[vi][3]))]
        P = ptok[tok]
        if hav(la, lo, P["lat"], P["lon"]) < 400:
            if seg: segs.append(seg); seg = None
            dy = hav(P["lat"], P["lon"], la, P["lon"]) * (1 if la >= P["lat"] else -1)
            dx = hav(P["lat"], P["lon"], P["lat"], lo) * (1 if lo >= P["lon"] else -1)
            out_pts.append({"t": t(ts), "kind": "stay", "place": tok, "dx": round(dx, 1), "dy": round(dy, 1),
                            "acc": acc, "speed": sp, "alt": alt})
            continue
    if seg is None:
        seg = {"id": len(segs), "path": []}
    seg["path"].append((t(ts), la, lo, acc, sp, alt))
if seg: segs.append(seg)

# Moves: replace geometry with cumulative-distance fraction along the real path.
seg_out = []
for s in segs:
    p = s["path"]
    cum = [0.0]
    for a, b in zip(p, p[1:]):
        cum.append(cum[-1] + hav(a[1], a[2], b[1], b[2]))
    L = cum[-1] or 1.0
    t0, t1 = p[0][0], p[-1][0]
    prev = next((v["place"] for v in reversed(vis_out) if v["t1"] <= t0 + 60), None)
    nxt = next((v["place"] for v in vis_out if v["t0"] >= t1 - 60), None)
    far = max(hav(x[1], x[2], home[0], home[1]) for x in p)
    seg_out.append({"id": s["id"], "t0": t0, "t1": t1, "from": prev, "to": nxt,
                    "real_km": round(L / 1000, 2), "max_km_from_home": round(far / 1000, 1),
                    "flight": far > 100000})
    for (ts, la, lo, acc, sp, alt), c in zip(p, cum):
        out_pts.append({"t": ts, "kind": "move", "seg": s["id"], "frac": round(c / L, 4),
                        "acc": acc, "speed": sp, "alt": alt})
out_pts.sort(key=lambda x: x["t"])
shape["points"] = out_pts
shape["moves"] = seg_out

# ---------------------------------------------------------------- messages
rows = q("""select occurred_at, thread_id, (metadata->>'is_from_me')::boolean, length(coalesce(body,'')),
                   is_group_message, has_attachments, metadata->>'reaction_type', reply_to_message_id is not null,
                   from_identifier, metadata->'attachment_media'
            from data_communication_message where occurred_at >= %s and occurred_at < %s order by occurred_at""", W0, W1)
tcount = {}
for r in rows: tcount[r[1]] = tcount.get(r[1], 0) + 1
trank = {k: f"T{i+1:02d}" for i, k in enumerate(sorted(tcount, key=lambda k: -tcount[k]))}
members = {}
for r in rows:
    if not r[2]: members.setdefault(r[1], set()).add(r[8])
shape["threads"] = [{"tok": trank[k], "n": n, "group": any(r[4] for r in rows if r[1] == k),
                     "others": len(members.get(k, ()))} for k, n in sorted(tcount.items(), key=lambda kv: -kv[1])]
shape["messages"] = [{"t": t(r[0]), "thread": trank[r[1]], "me": bool(r[2]), "len": r[3], "att": r[5],
                      "media": (json.dumps(r[9])[:40] if r[9] else None) and ("image" if "image" in json.dumps(r[9]) else "video" if "video" in json.dumps(r[9]) else "file"),
                      "reaction": bool(r[6] and r[6] not in ("0", "")), "reply": r[7],
                      "sender": (None if r[2] else f"{trank[r[1]]}.{sorted(members.get(r[1], {None}), key=str).index(r[8]) if r[8] in members.get(r[1], ()) else 0}")}
                     for r in rows]

# ---------------------------------------------------------------- email
AUTO = re.compile(r"no-?reply|notif|news|digest|updates?@|mailer|hello@|info@|team@|support@|marketing|alerts?@|bounce", re.I)
rows = q("""select occurred_at, direction, thread_id, from_email, has_attachments, labels, length(coalesce(subject,'')),
                   length(coalesce(body,'')), is_read, is_starred
            from data_communication_email where occurred_at >= %s and occurred_at < %s order by occurred_at""", W0, W1)
scount = {}
for r in rows: scount[r[3]] = scount.get(r[3], 0) + 1
srank = {k: f"S{i+1:02d}" for i, k in enumerate(sorted(scount, key=lambda k: -scount[k]))}
shape["senders"] = [{"tok": srank[k], "n": n, "auto": bool(k and AUTO.search(k))} for k, n in sorted(scount.items(), key=lambda kv: -kv[1])]
shape["emails"] = [{"t": t(r[0]), "dir": r[1], "sender": srank[r[3]], "att": r[4],
                    "labels": [l for l in (r[5] or []) if str(l).startswith(("CATEGORY_", "INBOX", "IMPORTANT", "SENT", "UNREAD"))],
                    "subj_len": r[6], "len": r[7], "read": r[8], "star": r[9]} for r in rows]

# ---------------------------------------------------------------- audio + transcripts
MEDIA = {"background_media", "media", "media_clips", "background_music", "music", "podcast", "television",
         "video", "tv shows", "movie", "movies", "youtube", "soundtrack", "commentary", "sports commentary"}
QUIET = {"quiet", "mostly_quiet", "mostly quiet", "ambient", "ambient noise", "ambient_noise", "noise",
         "background noise", "typing", "household sounds", "footsteps", "road noise", "engine noise"}
SELF = {"self-talk"}
PHONE = {"phone call", "call", "facetime"}
def klass(tags, speakers):
    tg = {str(x).lower() for x in (tags or [])}
    if tg & PHONE: return "call"
    if tg & MEDIA and not ({"conversation", "dialogue", "discussion"} & tg): return "media"
    if tg & SELF: return "self"
    if tg & QUIET and not ({"conversation", "dialogue", "discussion"} & tg): return "quiet"
    return "conversation" if (speakers or 1) > 1 or "conversation" in tg else "speech"
rows = q("""select started_at, ended_at, duration_seconds, length(coalesce(text,'')), speaker_count, language,
                   confidence, case when jsonb_typeof(to_jsonb(tags))='array' then to_jsonb(tags) else '[]'::jsonb end
            from data_communication_transcription where started_at >= %s and started_at < %s order by started_at""", W0, W1)
shape["transcripts"] = [{"t0": t(r[0]), "t1": t(r[1]), "dur": r[2], "len": r[3], "speakers": r[4], "lang": r[5],
                         "conf": r[6], "cls": klass(r[7], r[4]) if r[3] else "silent"} for r in rows]
rows = q("""select started_at, ended_at, duration_seconds, is_silent, average_db_level, audio_format
            from data_audio_recording where started_at >= %s and started_at < %s order by started_at""", W0, W1)
shape["recordings"] = [{"t0": t(r[0]), "t1": t(r[1]), "dur": r[2], "silent": r[3], "db": r[4], "fmt": r[5]} for r in rows]
rows = q("""select started_at, ended_at, speaker_mode, avg_db, chunk_count, length(coalesce(content,''))
            from data_audio_session where started_at >= %s and started_at < %s order by started_at""", W0, W1)
shape["audio_sessions"] = [{"t0": t(r[0]), "t1": t(r[1]), "mode": r[2], "db": r[3], "chunks": r[4], "len": r[5]} for r in rows]

# ---------------------------------------------------------------- mac activity
rows = q("""select started_at, ended_at, app_name, app_bundle_id, attention, is_open, closed_by, length(coalesce(window_title,''))
            from data_activity_app_session where started_at >= %s and started_at < %s order by started_at""", W0, W1)
shape["app_sessions"] = [{"t0": t(r[0]), "t1": t(r[1]), "app": r[2], "bundle": r[3], "attention": r[4],
                          "open": r[5], "closed_by": r[6], "title_len": r[7]} for r in rows]
rows = q("""select occurred_at, domain, length(coalesce(page_title,'')), url
            from data_activity_web_browsing where occurred_at >= %s and occurred_at < %s order by occurred_at""", W0, W1)
dcount = {}
for r in rows: dcount[r[1]] = dcount.get(r[1], 0) + 1
shape["domains"] = [{"domain": k, "n": n} for k, n in sorted(dcount.items(), key=lambda kv: -kv[1])]
shape["web"] = [{"t": t(r[0]), "domain": r[1], "title_len": r[2],
                 "depth": len([s for s in (r[3] or "").split("?")[0].split("/")[3:] if s])} for r in rows]

# ---------------------------------------------------------------- money
rows = q("""select occurred_at, amount, currency, merchant_name, merchant_category, category::text, payment_channel,
                   transaction_type, is_pending, account_id, source_provider
            from data_financial_transaction where occurred_at >= %s and occurred_at < %s order by occurred_at""", W0, W1)
mcount = {}
for r in rows: mcount[r[3]] = mcount.get(r[3], 0) + 1
mrank = {k: f"M{i+1:02d}" for i, k in enumerate(sorted(mcount, key=lambda k: (-mcount[k], str(k))))}
arank = {k: f"A{i+1}" for i, k in enumerate(sorted({str(r[9]) for r in rows}))}
shape["transactions"] = [{"t": t(r[0]), "amount": float(r[1]), "cur": r[2], "merchant": mrank[r[3]],
                          "mcc": r[4], "category": r[5], "channel": r[6], "type": r[7], "pending": r[8],
                          "account": arank[str(r[9])], "provider": r[10]} for r in rows]

# ---------------------------------------------------------------- calendar (real calendars only)
rows = q("""select started_at, ended_at, is_all_day, status, response_status,
                   case when jsonb_typeof(to_jsonb(attendee_identifiers))='array' then jsonb_array_length(to_jsonb(attendee_identifiers)) else 0 end,
                   location_name is not null and location_name <> ''
            from data_calendar_event where started_at >= %s and started_at < %s
              and (is_all_day or ended_at - started_at >= interval '15 minutes') order by started_at""", W0, W1)
shape["calendar"] = [{"t0": t(r[0]), "t1": t(r[1]), "all_day": r[2], "status": r[3], "response": r[4],
                      "attendees": r[5], "has_location": r[6]} for r in rows]

# ---------------------------------------------------------------- health + misc
shape["steps"] = [{"t": t(a), "v": b} for a, b in q("select occurred_at, step_count from data_health_steps where occurred_at >= %s and occurred_at < %s order by 1", W0, W1)]
shape["distance"] = [{"t": t(a), "v": float(b)} for a, b in q("select occurred_at, meters from data_health_distance where occurred_at >= %s and occurred_at < %s order by 1", W0, W1)]
shape["weather"] = [{"t": t(r[0]), "temp": r[1], "app": r[2], "hum": r[3], "precip": r[4], "wind": r[5], "is_day": r[6],
                     "tmax": r[7], "tmin": r[8], "code": r[9], "forecast": r[10]}
                    for r in q("""select occurred_at, temperature_c, apparent_c, humidity_pct, precipitation_mm, wind_kph, is_day,
                                         temp_max_c, temp_min_c, weather_code, is_forecast
                                  from data_environment_weather where occurred_at >= %s and occurred_at < %s and not is_forecast order by 1""", W0, W1)]

json.dump(shape, open(OUT, "w"), default=str)
summary = {k: (len(v) if isinstance(v, list) else v) for k, v in shape.items()}
print(json.dumps(summary))
print("moves:", [(m["from"], m["to"], m["real_km"], m["flight"]) for m in seg_out if m["real_km"] > 0.3][:40])
print("places:", [(p["tok"], p["hours"], p["visits"], p["night_h"], p["day_h"], p["km_from_home"]) for p in places])
