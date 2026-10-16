"""Fictional content for every slot: messages, transcripts, emails, web/app titles.

Each call sees the persona, the day plan and a slot list (time, sender, length).
Never a real word: slot lengths come from shape.json, text from the model.
"""
import json, os, random, sys
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
from llm import chat_json
from plan import SYSTEM, PERSONA

from work import SRC
s = json.load(open(f"{SRC}/shape.json"))
sk = json.load(open(f"{SRC}/fiction/skeleton.json"))
cast = json.load(open(f"{SRC}/fiction/cast.json"))
plans = json.load(open(f"{SRC}/fiction/plans.json"))
arc = json.load(open(f"{SRC}/fiction/arc.json"))
os.makedirs(f"{SRC}/fiction/content", exist_ok=True)
DAY, H = 86400, 3600
def hm(t): t %= DAY; return f"{t//H:02d}:{t%H//60:02d}"
PEOPLE = cast["people"]
STYLE = ("Jules is gender-neutral: refer to Jules by name or they/them, never he/she. "
         "Return only JSON.")

def name_of(k):
    return PEOPLE[k]["name"] if k in PEOPLE else k

# ------------------------------------------------------------------ messages
def thread_slots():
    by = defaultdict(list)
    for i, m in enumerate(s["messages"]):
        by[(m["t"] // DAY, m["thread"])].append((i, m))
    return by

def sender_for(tok, m):
    name, who = cast["threads"][tok]
    if m["me"]:
        return "Jules"
    if who == ["svc"]:
        svc = cast["sms_services"]
        return svc[int((m.get("sender") or "x.0").split(".")[-1]) % len(svc)][0]
    k = int((m.get("sender") or "x.0").split(".")[-1]) % len(who)
    return name_of(who[k])

def gen_messages(key, slots):
    d, tok = key
    name, who = cast["threads"][tok]
    plan = plans[d]
    topic = plan.get("threads", {}).get(name) or ""
    rows = [{"i": i, "time": hm(m["t"]), "from": sender_for(tok, m), "chars": max(2, min(m["len"], 400)),
             "attachment": m["media"] if m["att"] else None, "is_reaction": m["reaction"], "is_reply": m["reply"]}
            for i, m in slots]
    members = ", ".join(name_of(w) for w in who) if who != ["svc"] else "automated short-code senders"
    prompt = f"""Thread: "{name}" (iMessage{', group' if len(who) > 1 else ''}; members besides Jules: {members}).
Day: {plan['weekday']}. What Jules's day looks like: {json.dumps(plan.get('happenings', [])[:40], ensure_ascii=False)}
Today this thread is about: {topic}
Week storylines: {json.dumps(arc.get('threads_of_the_week', []), ensure_ascii=False)}

Write every message below, in order. Match each one's sender, time and roughly its character
count. is_reaction=true means an iMessage tapback written as e.g. Loved “...” / Liked “...” /
Laughed at “...” quoting an earlier message. attachment means the message is a photo/video/file;
write a short caption or leave body "" if chars is small. Texting register: lowercase is fine,
typos are rare, no hashtags. For short-code senders write realistic automated texts (codes,
deliveries, appointment reminders, flight updates) from the sender named.

Messages: {json.dumps(rows, ensure_ascii=False)}

Return {{"messages": [{{"i": <i>, "body": "..."}}]}}  {STYLE}"""
    out = chat_json(SYSTEM, prompt, max_tokens=16000, tag=f"msg_d{d}_{tok}")
    return {int(x["i"]): x["body"] for x in out["messages"]}

# ------------------------------------------------------------------ transcripts
def sessions():
    out = []
    for a_i, a in enumerate(s["audio_sessions"]):
        chunks = [(i, x) for i, x in enumerate(s["transcripts"])
                  if a["t0"] - 30 <= x["t0"] <= a["t1"] + 30 and x["cls"] != "silent"]
        if chunks:
            out.append((a_i, a, chunks))
    return out

def gen_session(a_i, a, chunks):
    d = a["t0"] // DAY
    plan = plans[min(d, 6)]
    scene = next((x for x in plan.get("audio", []) if x.get("session") == a_i), {})
    rows = [{"i": i, "start": hm(x["t0"]), "seconds": x["dur"], "kind": x["cls"], "speakers": x["speakers"] or 1,
             "chars": int(min(2600, max(60, x["len"] * 0.8)))} for i, x in chunks]
    prompt = f"""An always-on phone microphone records Jules's day in ~60 second chunks, and each chunk is
transcribed. Write the transcripts for one audio session.

Day: {plan['weekday']}. Session {hm(a['t0'])}–{hm(a['t1'])}. Scene: {json.dumps(scene, ensure_ascii=False)}
Around it: {json.dumps([h for h in plan.get('happenings', []) if abs(int(h.get('time','00:00')[:2])*3600 + int(h.get('time','00:00')[3:5])*60 - a['t0'] % DAY) < 3*3600], ensure_ascii=False)}

Chunk kinds: conversation = people talking (use speaker labels like "Jules:", "Bea:", "Speaker 2:"
for strangers); speech = one person talking (often Jules on the phone or thinking aloud);
call = a phone or video call, Jules's side plus some of the other side; media = a podcast, TV,
or video playing (invent fictional shows/hosts, never real ones), with fragments;
quiet = mostly silence, write a short fragment like "[typing] ... okay. [indistinct]" .
Transcripts are imperfect: fillers, cut-off sentences at chunk edges, [inaudible]. Chunks are
consecutive, so a conversation continues from one chunk into the next. Hit each chunk's
character count roughly.

Chunks: {json.dumps(rows, ensure_ascii=False)}

Return {{"chunks": [{{"i": <i>, "text": "...", "tags": ["3-6 lowercase topic tags"], "title": "4-8 word title"}}]}} {STYLE}"""
    out = chat_json(SYSTEM, prompt, max_tokens=32000, tag=f"tr_{a_i}")
    return {int(x["i"]): x for x in out["chunks"]}

# ------------------------------------------------------------------ email
def gen_emails(d):
    lo, hi = d * DAY, (d + 1) * DAY
    em = [(i, e) for i, e in enumerate(s["emails"]) if lo <= e["t"] < hi]
    plan = plans[d]
    personal = [(i, e) for i, e in em if "CATEGORY_PERSONAL" in e["labels"] or e["dir"] == "sent"]
    other = [(i, e) for i, e in em if (i, e) not in personal]
    R = random.Random(d)
    svc = cast["email_services"]
    rows = []
    for i, e in personal:
        rows.append({"i": i, "time": hm(e["t"]), "direction": e["dir"], "kind": "personal",
                     "chars": int(min(1800, max(120, e["len"] * 0.5)))})
    for i, e in other:
        sv = svc[R.randrange(len(svc))] if e["sender"] not in ("S01", "S02", "S03") else svc[int(e["sender"][1:]) - 1]
        rows.append({"i": i, "time": hm(e["t"]), "direction": "received",
                     "kind": "promotion" if "CATEGORY_PROMOTIONS" in e["labels"] else "update",
                     "sender": sv[0], "sender_email": sv[1], "chars": int(min(1200, max(120, e["len"] * 0.15)))})
    people = {k: PEOPLE[k]["name"] for k in cast["email_people"]}
    prompt = f"""Write Jules's email for {plan['weekday']}.
Personal email today (from the plan): {json.dumps(plan.get('personal_email', []), ensure_ascii=False)}
People who email Jules (key: name): {json.dumps(people, ensure_ascii=False)}. Jules is jules@example.com.
Week storylines: {json.dumps(arc.get('threads_of_the_week', []), ensure_ascii=False)}

For kind=personal: pick a sender from the people above that fits the plan (direction=sent means
Jules wrote it, pick the recipient). For update/promotion: write as the named service — receipts,
notifications, newsletters, digests — specific and plausible for this week (a flight to Denver,
board orders, beta builds, a mortgage, design newsletters). Bodies are plain text, roughly the
character count. Newsletters may be cut short with "…".

Emails: {json.dumps(rows, ensure_ascii=False)}

Return {{"emails": [{{"i": <i>, "person": "<key, for personal only>", "subject": "...", "body": "...", "thread_hint": "short slug shared by replies in one thread"}}]}} {STYLE}"""
    out = chat_json(SYSTEM, prompt, max_tokens=32000, tag=f"email_d{d}")
    got = {int(x["i"]): x for x in out["emails"]}
    for r in rows:
        if r["i"] in got and "sender" in r:
            got[r["i"]]["service"] = r["sender"]; got[r["i"]]["service_email"] = r["sender_email"]
    return got

# ------------------------------------------------------------------ web + app title pools
def gen_pools():
    priv = json.load(open(f"{SRC}/cast_private.json"))
    fict = sorted(set(priv["domain_map"].values()), key=lambda x: list(priv["domain_map"].values()).index(x))
    prompt = f"""Jules's browser history for the week needs page titles and URLs. For each domain below,
write 12-25 distinct pages Jules plausibly visited this week (more for the first ten domains), as
full https URLs with realistic paths and the exact page title a browser would show. Tie them to
the week: the hardware build, the beta and its sync bug, the Denver trip, design references,
groceries, a mortgage, the Thursday dinner. For mail.google.com and calendar.google.com use the
inbox/event titles style. For github.com use a fictional org "arden-works" with repos like
"thing-app", "thing-firmware". Invent fictional article titles; never real people.

Domains (most-visited first): {json.dumps(fict)}

Return {{"pages": {{"<domain>": [{{"url": "...", "title": "..."}}]}}}} {STYLE}"""
    web = chat_json(SYSTEM, prompt, max_tokens=48000, tag="pool_web")
    prompt2 = f"""Window titles for Jules's Mac apps this week. For each app write 10-30 realistic window
titles as macOS would report them, tied to the week. Claude: titles of chats Jules had with the
assistant (about firmware, CSS, copy, the sync bug, a tart recipe...). Figma: "<file> – Figma".
Slack: "#channel – Arden Works – Slack" style with fictional channels. Messages: thread names from
the cast. Preview: PDF/image names (invoices, board schematics, boarding pass). Podcasts and
Spotify: invented shows, episodes, artists and tracks only. Terminal: "jules@studio: ~/thing-app — zsh"
style. Notes: note titles. Xcode: project names. Virtues: the persona's own app's views.

Apps: {json.dumps(sorted(set(cast['app_map'].values())))}

Return {{"apps": {{"<app>": ["title", ...]}}}} {STYLE}"""
    apps = chat_json(SYSTEM, prompt2, max_tokens=24000, tag="pool_apps")
    return {"web": web["pages"], "apps": apps["apps"]}

if __name__ == "__main__":
    what = sys.argv[1:] or ["pools", "messages", "emails", "transcripts"]
    if "pools" in what:
        json.dump(gen_pools(), open(f"{SRC}/fiction/content/pools.json", "w"), ensure_ascii=False)
        print("pools done", flush=True)
    if "messages" in what:
        by = thread_slots()
        with ThreadPoolExecutor(8) as ex:
            res = dict(zip(by.keys(), ex.map(lambda kv: gen_messages(*kv), by.items())))
        bodies = {}
        for (d, tok), got in res.items():
            for i, b in got.items():
                bodies[i] = b
        json.dump(bodies, open(f"{SRC}/fiction/content/messages.json", "w"), ensure_ascii=False)
        print("messages", len(bodies), "of", len(s["messages"]), flush=True)
    if "emails" in what:
        with ThreadPoolExecutor(7) as ex:
            res = list(ex.map(gen_emails, range(7)))
        allm = {}
        for r in res: allm.update(r)
        json.dump(allm, open(f"{SRC}/fiction/content/emails.json", "w"), ensure_ascii=False)
        print("emails", len(allm), "of", len(s["emails"]), flush=True)
    if "transcripts" in what:
        ss = sessions()
        with ThreadPoolExecutor(8) as ex:
            res = list(ex.map(lambda x: gen_session(*x), ss))
        allt = {}
        for r in res: allt.update(r)
        json.dump(allt, open(f"{SRC}/fiction/content/transcripts.json", "w"), ensure_ascii=False)
        need = sum(1 for x in s["transcripts"] if x["cls"] != "silent")
        print("transcripts", len(allt), "of", need, flush=True)
