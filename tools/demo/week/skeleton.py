"""Per-day skeleton in FICTIONAL terms: what the plan prompt is allowed to see.

Times, counts, durations and fictional places only. No real text, name or
domain reaches this file.
"""
import json, os
from collections import Counter, defaultdict

from work import SRC
s = json.load(open(f"{SRC}/shape.json"))
loc = json.load(open(f"{SRC}/fiction/location.json"))
cast = json.load(open(f"{SRC}/fiction/cast.json"))
DAY, H = 86400, 3600
DN = ["Saturday", "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday"]
def hm(t): t %= DAY; return f"{t//H:02d}:{t%H//60:02d}"

visits = loc["visits"]
def where(t):
    for v in visits:
        if v["t0"] <= t <= v["t1"]:
            return v["name"]
    for m in loc["moves"]:
        if m["t0"] <= t <= m["t1"]:
            return f"moving ({m['mode']})"
    best = min(visits, key=lambda v: min(abs(v["t0"] - t), abs(v["t1"] - t)))
    return best["name"] + "?"

# Calendar: drop the duplicate copies (same event mirrored into two calendars).
cal, seen = [], set()
for c in s["calendar"]:
    k = (c["t0"], c["t1"])
    if k in seen: continue
    seen.add(k); cal.append(c)

# Audio sessions + the transcript chunks inside each.
sess = s["audio_sessions"]
tr = s["transcripts"]
for i, a in enumerate(sess):
    inside = [x for x in tr if a["t0"] - 30 <= x["t0"] <= a["t1"] + 30]
    a["i"] = i
    a["cls"] = Counter(x["cls"] for x in inside if x["cls"] != "silent")
    a["voiced"] = sum(1 for x in inside if x["cls"] != "silent")
    a["speakers"] = max([x["speakers"] or 1 for x in inside if x["cls"] != "silent"] or [0])

days = []
for d in range(7):
    lo, hi = d * DAY, (d + 1) * DAY
    D = {"day": d, "weekday": DN[d]}
    D["stays"] = [f'{hm(max(v["t0"], lo))}–{hm(min(v["t1"], hi - 1))} {v["name"]}' for v in visits
                  if v["t1"] > lo and v["t0"] < hi and min(v["t1"], hi) - max(v["t0"], lo) >= 300]
    D["moves"] = [f'{hm(m["t0"])}–{hm(m["t1"])} {m["mode"]}' + (f' {m.get("from")}→{m.get("to")}' if m.get("from") else "")
                  for m in loc["moves"] if lo <= m["t0"] < hi and m["t1"] - m["t0"] >= 120]
    D["calendar"] = [{"slot": i, "time": f'{hm(c["t0"])}–{hm(c["t1"])}', "all_day": c["all_day"],
                      "attendees": c["attendees"], "where_you_are": where(c["t0"])}
                     for i, c in enumerate(cal) if lo <= c["t0"] < hi]
    D["audio"] = [{"session": a["i"], "time": f'{hm(a["t0"])}–{hm(a["t1"])}', "at": where(a["t0"]),
                   "voiced_chunks": a["voiced"], "max_speakers": a["speakers"], "mix": dict(a["cls"])}
                  for a in sess if lo <= a["t0"] < hi and a["voiced"] > 0]
    tc = Counter(m["thread"] for m in s["messages"] if lo <= m["t"] < hi)
    D["messages"] = {cast["threads"][k][0]: n for k, n in tc.most_common()}
    D["money"] = [{"slot": i, "time": hm(x["t"]), "amount_usd": round(abs(x["amount"]) / 100, 2),
                   "direction": "in" if x["amount"] < 0 else "out", "channel": x["channel"] or "card",
                   "where_you_are": where(x["t"])}
                  for i, x in enumerate(s["transactions"]) if lo <= x["t"] < hi]
    apps = Counter()
    for a in s["app_sessions"]:
        if lo <= a["t0"] < hi and a["app"] not in cast["drop_apps"]:
            apps[cast["app_map"].get(a["app"], a["app"])] += ((a["t1"] or a["t0"]) - a["t0"])
    D["mac_hours"] = {k: round(v / H, 1) for k, v in apps.most_common(8) if v > 600}
    em = [e for e in s["emails"] if lo <= e["t"] < hi]
    D["email"] = {"received": sum(e["dir"] == "received" for e in em),
                  "personal": sum("CATEGORY_PERSONAL" in e["labels"] for e in em),
                  "sent": sum(e["dir"] == "sent" for e in em)}
    D["steps"] = sum(x["v"] for x in s["steps"] if lo <= x["t"] < hi)
    days.append(D)

json.dump({"days": days, "calendar": cal}, open(f"{SRC}/fiction/skeleton.json", "w"), indent=1)
for D in days:
    print(D["weekday"], len(D["stays"]), "stays", len(D["calendar"]), "cal", len(D["audio"]), "audio", sum(D["messages"].values()), "msgs", len(D["money"]), "txn", D["mac_hours"])
