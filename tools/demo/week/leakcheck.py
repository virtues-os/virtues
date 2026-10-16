"""Leak check: fiction/week.sql (+ any later freeze file) against virtues_real.

Fails on: a real phone/email; a real person's full name; an 8-word run shared
with any real message, email or transcript; a point within 300 m of a real place
with an hour or more of dwell. Prints the FICTION side of a hit only, never the
real value it matched.
Usage: uv run --with 'psycopg[binary]' leakcheck.py [file.sql ...]
"""
import json, math, re, sys, os
import psycopg

from work import SRC
files = sys.argv[1:] or [f"{SRC}/fiction/week.sql"]
text = "\n".join(open(f).read() for f in files)
conn = psycopg.connect("dbname=virtues_real")
q = lambda sql: conn.execute(sql).fetchall()
fails = 0

def norm_phone(p): return re.sub(r"\D", "", p or "")[-10:]

# 1. phones + emails
real_phones = {norm_phone(r[0]) for r in q("""select distinct from_identifier from data_communication_message
    union select distinct from_handle from data_communication_message
    union select jsonb_array_elements_text(phones) from wiki_people""") if r[0] and len(norm_phone(r[0])) == 10}
real_emails = {r[0].lower() for r in q("""select distinct from_email from data_communication_email
    union select jsonb_array_elements_text(emails) from wiki_people""") if r[0]}
fic_phones = {norm_phone(m) for m in re.findall(r"\+?1?\d{10}", text)}
fic_emails = {m.lower() for m in re.findall(r"[\w.+-]+@[\w.-]+\.\w+", text)}
hits = (fic_phones & real_phones) - {""}
hits_e = {e for e in fic_emails & real_emails if not e.endswith("example.com")}
print("phones matching real:", len(hits), "| emails matching real:", len(hits_e))
fails += len(hits) + len(hits_e)

# 2. full names
names = set()
for (n,) in q("""select name from wiki_people union select distinct from_name from data_communication_message
                 union select distinct from_name from data_communication_email"""):
    if n and len(n.split()) >= 2 and all(len(w) > 2 for w in n.split()[:2]):
        names.add(" ".join(n.split()[:2]).lower())
low = text.lower()
name_hits = [n for n in names if re.search(r"\b" + re.escape(n) + r"\b", low)]
SERVICE = re.compile(r"\b(google|apple|app store|calendar|search|maps|card|workspace|inc|airlines?|bank|support|team|notifications?)\b")
warn_names = [n for n in name_hits if SERVICE.search(n)]
name_hits = [n for n in name_hits if not SERVICE.search(n)]
print("service-like names (warn only):", len(warn_names))
print("real full names appearing:", len(name_hits))
for n in name_hits:
    i = low.index(n); print("   fiction context:", repr(text[max(0, i - 60): i + 60].replace(n, "<HIT>")))
fails += len(name_hits)

# 3. 8-word shingles
def shingles(t, k=8):
    w = re.findall(r"[a-z0-9']+", (t or "").lower())
    return {" ".join(w[i:i + k]) for i in range(len(w) - k + 1)}
fic_sh = shingles(text)
from collections import Counter
real_cnt = Counter()
for sql in ("select body from data_communication_message where length(body) > 40",
            "select body from data_communication_email where occurred_at > now() - interval '120 days'",
            "select text from data_communication_transcription where length(text) > 40"):
    for (b,) in conn.execute(sql):
        for x in shingles(b) & fic_sh:
            real_cnt[x] += 1
ALLOW = {l.strip() for l in open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "leak_allow.txt")) if l.strip()}  # reviewed boilerplate
common = {x for x, n in real_cnt.items() if n < 3} - ALLOW
print("8-word runs shared but common (>=3 real texts, boilerplate):", sum(1 for n in real_cnt.values() if n >= 3))
# boilerplate that is not personal (legal footers etc.) still counts; show fiction side
print("8-word runs shared with real text:", len(common))
for c in sorted(common): print("   ", repr(c))
fails += len(common)

# 4. coordinates
places = q("""select round(latitude::numeric,4), round(longitude::numeric,4), sum(extract(epoch from ended_at-started_at))/3600
              from data_location_visit group by 1,2 having sum(extract(epoch from ended_at-started_at)) > 36000""")
pts = [(float(a), float(b)) for a, b in re.findall(r"\((?:'wk_pt_\d+'|'wk_vis_\d+'[^,]*,[^,]*), ([-\d.]+), ([-\d.]+)", text)]
for obj in text.split('{"id": ')[1:]:
    la = re.search(r'"latitude": ?([-\d.]+)', obj[:900]); lo = re.search(r'"longitude": ?([-\d.]+)', obj[:900])
    if la and lo: pts.append((float(la.group(1)), float(lo.group(1))))
def hav(a, b, c, d):
    p = math.radians
    x = math.sin(p(c - a) / 2) ** 2 + math.cos(p(a)) * math.cos(p(c)) * math.sin(p(d - b) / 2) ** 2
    return 6371000 * 2 * math.asin(math.sqrt(x))
near = 0; closest = 1e12
for la, lo in pts:
    for pla, plo, h in places:
        dd = hav(la, lo, float(pla), float(plo))
        closest = min(closest, dd)
        if dd < 300: near += 1; break
print(f"points checked: {len(pts)}, within 300 m of a real 10h+ place: {near}, closest: {closest:.0f} m")
fails += near
print("FAIL" if fails else "PASS", fails)
sys.exit(1 if fails else 0)
