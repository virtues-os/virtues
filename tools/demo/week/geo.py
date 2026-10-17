"""Location stream: real week SHAPE -> fictional geometry for the demo persona.

Reads shape.json (real timing, place tokens, stay offsets, move fractions) and
writes fiction/location.json: visits + points in week-seconds (0 = Sat 00:00
America/Chicago), on the persona's places, along routed roads. No real
coordinate is carried over: stays re-centre on fictional places, moves follow a
fresh route between fictional places at the real move's pace profile.
"""
import json, math, os, random, time, urllib.request, hashlib

from work import SRC
R = random.Random(20261016)
shape = json.load(open(f"{SRC}/shape.json"))
os.makedirs(f"{SRC}/fiction", exist_ok=True)
os.makedirs(f"{SRC}/cache/routes", exist_ok=True)

H = 3600
DAY = 86400
def wk(d, hh, mm=0): return d * DAY + hh * H + mm * 60

# Fictional places. Ids are the demo box's own wiki_places rows where one fits.
PLACES = {
    "home":     ("place_demo_home", "Home", 30.2849, -97.7341),
    "studio":   ("p3y_pl_studio", "The workshop", 30.2951, -97.7303),
    "ylva":      ("p3y_pl_ylva", "Ylva's", 30.2943, -97.7269),
    "aus":      ("p3y_pl_airport", "AUS", 30.1975, -97.6664),
    "ramen":    ("place_demo_ramen", "Ramen Kobo", 30.2898, -97.7434),
    "taqueria": ("p3y_pl_taqueria", "Vera's Taqueria", 30.2910, -97.7307),
    "hardware": ("p3y_pl_hardware", "Cobb Hardware", 30.2928, -97.7290),
    "heb":      ("p3y_pl_grocery", "H-E-B", 30.2920, -97.7373),
    "gym":      ("p3y_pl_gym", "Ironside Barbell", 30.2936, -97.7298),
    "lake":     ("place_demo_ladybird", "Lady Bird Lake", 30.2787, -97.7385),
    "den":      ("wk_pl_den", "Denver International Airport", 39.8561, -104.6737),
    "den_rail": ("wk_pl_den_rail", "A Line, DEN", 39.8476, -104.6740),
    "union":    ("wk_pl_union", "Union Station", 39.7527, -105.0003),
    "tamsin":     ("wk_pl_tamsin", "Tamsin's", 39.7620, -105.0110),
}
TOK = {"P01": "home", "P02": "studio", "P03": "ylva", "P04": "tamsin", "P05": "aus", "P06": "ramen",
       "P07": "union", "P08": "taqueria", "P09": "hardware", "P10": "den_rail", "P11": "heb", "P12": "den"}

def hav(a, b, c, d):
    p = math.radians
    x = math.sin(p(c - a) / 2) ** 2 + math.cos(p(a)) * math.cos(p(c)) * math.sin(p(d - b) / 2) ** 2
    return 6371000 * 2 * math.asin(math.sqrt(x))

def offset(lat, lon, dx, dy):
    return lat + dy / 111320.0, lon + dx / (111320.0 * math.cos(math.radians(lat)))

def route(waypoints, mode):
    key = hashlib.sha1(json.dumps([waypoints, mode]).encode()).hexdigest()[:16]
    path = f"{SRC}/cache/routes/{key}.json"
    if os.path.exists(path):
        return json.load(open(path))
    coords = ";".join(f"{lo},{la}" for la, lo in waypoints)
    base = ("https://router.project-osrm.org/route/v1/driving/" if mode == "car"
            else "https://routing.openstreetmap.de/routed-foot/route/v1/driving/")
    url = f"{base}{coords}?overview=full&geometries=geojson"
    import subprocess
    for attempt in range(4):
        try:
            raw = subprocess.run(["curl", "-s", "-m", "30", "-A", "virtues-demo-week/1", url],
                                 capture_output=True, text=True, check=True).stdout
            d = json.loads(raw)
            line = [(la, lo) for lo, la in d["routes"][0]["geometry"]["coordinates"]]
            json.dump(line, open(path, "w"))
            time.sleep(1.1)
            return line
        except Exception as e:
            time.sleep(2 + attempt * 3)
    raise RuntimeError(f"route failed {waypoints}")

def great_circle(a, b, n=200):
    la1, lo1, la2, lo2 = map(math.radians, (a[0], a[1], b[0], b[1]))
    d = 2 * math.asin(math.sqrt(math.sin((la2 - la1) / 2) ** 2 + math.cos(la1) * math.cos(la2) * math.sin((lo2 - lo1) / 2) ** 2))
    out = []
    for i in range(n + 1):
        f = i / n
        A = math.sin((1 - f) * d) / math.sin(d); B = math.sin(f * d) / math.sin(d)
        x = A * math.cos(la1) * math.cos(lo1) + B * math.cos(la2) * math.cos(lo2)
        y = A * math.cos(la1) * math.sin(lo1) + B * math.cos(la2) * math.sin(lo2)
        z = A * math.sin(la1) + B * math.sin(la2)
        out.append((math.degrees(math.atan2(z, math.sqrt(x * x + y * y))), math.degrees(math.atan2(y, x))))
    return out

def along(line, frac):
    cum = [0.0]
    for a, b in zip(line, line[1:]):
        cum.append(cum[-1] + hav(a[0], a[1], b[0], b[1]))
    target = frac * cum[-1]
    for i in range(1, len(cum)):
        if cum[i] >= target:
            seg = cum[i] - cum[i - 1] or 1
            f = (target - cum[i - 1]) / seg
            a, b = line[i - 1], line[i]
            return a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f
    return line[-1]

def length_m(line):
    return sum(hav(a[0], a[1], b[0], b[1]) for a, b in zip(line, line[1:]))

def P(name): return PLACES[name][2], PLACES[name][3]

# ------------------------------------------------------------------ plan the stays
# Real visits first, re-tokened; then the edits the fiction needs.
visits = [{"place": TOK[v["place"]], "t0": v["t0"], "t1": v["t1"]} for v in shape["visits"]]
points = [p for p in shape["points"]]
moves = {m["id"]: m for m in shape["moves"]}

# Sat return from Denver: everything before 09:50 Sat is replaced.
SAT_CUT = wk(0, 9, 50)
visits = [v for v in visits if v["t1"] > SAT_CUT or v["t0"] >= SAT_CUT or v["place"] != "home"]
visits = [dict(v, t0=max(v["t0"], SAT_CUT)) if v["place"] == "home" and v["t0"] < SAT_CUT else v for v in visits]
points = [p for p in points if p["t"] >= SAT_CUT]
synth_moves = []   # (t0, t1, waypoints, mode)
visits += [
    {"place": "tamsin", "t0": wk(0, 0), "t1": wk(0, 4, 50)},
    {"place": "den", "t0": wk(0, 5, 35), "t1": wk(0, 6, 55)},
    {"place": "aus", "t0": wk(0, 9, 5), "t1": wk(0, 9, 22)},
]
synth_moves += [
    (wk(0, 4, 50), wk(0, 5, 35), [P("tamsin"), P("den")], "car"),
    (wk(0, 6, 55), wk(0, 9, 5), [P("den"), P("aus")], "flight"),
    (wk(0, 9, 22), wk(0, 9, 50), [P("aus"), P("home")], "car"),
]
# Fri night ends at Tamsin's (real P04 runs past midnight into the next week).
for v in visits:
    if v["place"] == "tamsin" and v["t0"] > wk(6, 0):
        v["t1"] = wk(7, 0) - 1

# Gym, Mon + Wed mornings, cut out of a home stay.
def carve(t0, t1, place, there, back):
    global visits
    out = []
    for v in visits:
        if v["place"] == "home" and v["t0"] < t0 and v["t1"] > t1:
            out.append(dict(v, t1=t0))
            out.append({"place": place, "t0": t0 + there, "t1": t1 - back})
            out.append(dict(v, t0=t1))
        else:
            out.append(v)
    visits = out
    synth_moves.append((t0, t0 + there, [P("home"), P(place)], "car"))
    synth_moves.append((t1 - back, t1, [P(place), P("home")], "car"))
    return [p for p in points if not (t0 <= p["t"] < t1)]
points = carve(wk(2, 6, 45), wk(2, 8, 0), "gym", 6 * 60, 7 * 60)
points = carve(wk(4, 7, 0), wk(4, 8, 20), "gym", 6 * 60, 6 * 60)
visits.sort(key=lambda v: v["t0"])

# ------------------------------------------------------------------ moves -> geometry
def move_line(m):
    a, b = m["from"] and TOK[m["from"]], m["to"] and TOK[m["to"]]
    a = a or "home"; b = b or "home"
    speed = m["real_km"] / max(1 / 60, (m["t1"] - m["t0"]) / 3600)
    if m["flight"]:
        return great_circle(P("aus"), P("den")), "flight"
    mode = "car" if speed > 14 else "foot"
    if a == b:   # a loop: run or walk out to the lake and back
        far = offset(*P("lake"), R.uniform(-300, 300), R.uniform(-150, 150)) if m["real_km"] > 2 else\
              offset(*P(a), R.uniform(-500, 500), R.uniform(-500, 500))
        return route([P(a), far, P(a)], "foot"), "run" if speed > 7 else "walk"
    return route([P(a), P(b)], mode), mode

out_pts = []
for p in points:
    if p["kind"] == "stay":
        name = TOK[p["place"]]
        la, lo = offset(*P(name), p["dx"] * 0.8, p["dy"] * 0.8)
        out_pts.append({"t": p["t"], "lat": la, "lon": lo, "acc": p["acc"], "speed": p["speed"], "alt": p["alt"]})
lines = {}
for mid, m in moves.items():
    if m["t1"] < SAT_CUT and m["t0"] < SAT_CUT:
        continue
    lines[mid] = move_line(m)
for p in points:
    if p["kind"] == "move" and p["seg"] in lines:
        line, mode = lines[p["seg"]]
        la, lo = along(line, p["frac"])
        jit = 0 if mode == "flight" else 4
        la, lo = offset(la, lo, R.gauss(0, jit), R.gauss(0, jit))
        out_pts.append({"t": p["t"], "lat": la, "lon": lo, "acc": p["acc"], "speed": p["speed"], "alt": p["alt"]})

def synth(t0, t1, line, mode):
    step = {"car": 25, "foot": 40, "run": 30, "walk": 45, "flight": 180}[mode]
    L = length_m(line)
    n = max(2, (t1 - t0) // step)
    for i in range(n + 1):
        t = t0 + (t1 - t0) * i // n
        f = i / n
        # ease in/out so a drive leaves slowly and arrives slowly
        f = 0.5 - 0.5 * math.cos(math.pi * f)
        la, lo = along(line, f)
        sp = L / max(1, t1 - t0) * (1 - abs(1 - 2 * (i / n)) ** 3) * 1.4
        alt = (10600 * math.sin(math.pi * i / n) ** 0.4 if mode == "flight" else 160 + R.uniform(-4, 4))
        out_pts.append({"t": t, "lat": la, "lon": lo, "acc": round(R.uniform(4, 14), 1) if mode != "flight" else 65.0,
                        "speed": round(sp, 2), "alt": round(alt, 1)})

for t0, t1, wps, mode in synth_moves:
    line = great_circle(*wps) if mode == "flight" else route(wps, mode)
    synth(t0, t1, line, mode)

# Stays the fiction added (gym, Denver Sat morning, AUS) get sparse stay points.
covered = sorted(p["t"] for p in out_pts)
def has_point_near(t, w=600):
    import bisect
    i = bisect.bisect_left(covered, t - w)
    return i < len(covered) and covered[i] <= t + w
for v in visits:
    t = v["t0"] + 120
    while t < v["t1"] - 60:
        if not has_point_near(t, 420):
            la, lo = offset(*P(v["place"]), R.gauss(0, 12), R.gauss(0, 12))
            out_pts.append({"t": t, "lat": la, "lon": lo, "acc": round(R.uniform(5, 35), 1), "speed": 0.0,
                            "alt": 160.0 if P(v["place"])[0] < 35 else 1610.0})
        t += R.randint(300, 900)

out_pts.sort(key=lambda p: p["t"])
for v in visits:
    v["place_id"], v["name"] = PLACES[v["place"]][0], PLACES[v["place"]][1]
    v["lat"], v["lon"] = P(v["place"])
json.dump({"places": PLACES, "visits": visits, "points": out_pts,
           "moves": [{"t0": m["t0"], "t1": m["t1"], "mode": lines[k][1], "from": m["from"] and TOK[m["from"]],
                      "to": m["to"] and TOK[m["to"]]} for k, m in moves.items() if k in lines]
                    + [{"t0": a, "t1": b, "mode": md, "synth": True} for a, b, _, md in synth_moves]},
          open(f"{SRC}/fiction/location.json", "w"))
print("visits", len(visits), "points", len(out_pts))
for v in visits:
    d = ["Sat", "Sun", "Mon", "Tue", "Wed", "Thu", "Fri"][min(6, v["t0"] // DAY)]
    print(f'{d} {v["t0"]%DAY//H:02d}:{v["t0"]%H//60:02d}-{v["t1"]%DAY//H:02d}:{v["t1"]%H//60:02d} {v["name"]}')
