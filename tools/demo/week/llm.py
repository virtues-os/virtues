"""Cached chat calls through the AI gateway. Prompts carry fiction + shape only."""
import json, os, hashlib, subprocess, time, re

from work import SRC
os.makedirs(f"{SRC}/cache/llm", exist_ok=True)
REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "../../.."))
KEY = next(l.split("=", 1)[1].strip().strip('"') for l in open(f"{REPO}/.env") if l.startswith("AI_GATEWAY_API_KEY="))
MODEL = os.environ.get("WEEK_MODEL", "anthropic/claude-sonnet-5.5")

def chat(system, user, max_tokens=16000, model=MODEL, tag=""):
    key = hashlib.sha1(json.dumps([model, system, user, max_tokens]).encode()).hexdigest()[:20]
    path = f"{SRC}/cache/llm/{tag}{'_' if tag else ''}{key}.txt"
    if os.path.exists(path):
        return open(path).read()
    body = json.dumps({"model": model, "max_tokens": max_tokens, "reasoning": {"enabled": False},
                       "messages": [{"role": "system", "content": system}, {"role": "user", "content": user}]})
    for attempt in range(5):
        r = subprocess.run(["curl", "-s", "-m", "600", "https://ai-gateway.vercel.sh/v1/chat/completions",
                            "-H", f"Authorization: Bearer {KEY}", "-H", "Content-Type: application/json",
                            "--data-binary", "@-"], input=body, capture_output=True, text=True)
        try:
            d = json.loads(r.stdout)
            text = d["choices"][0]["message"]["content"]
            fr = d["choices"][0].get("finish_reason")
            if fr == "length" or not text.strip():
                raise RuntimeError(f"bad finish {fr} usage={d.get('usage', {}).get('completion_tokens')}")
            open(path, "w").write(text)
            return text
        except Exception as e:
            err = f"{e} :: {r.stdout[:200]}"
            print("llm retry", tag, err[:160], flush=True)
            time.sleep(3 + attempt * 5)
    raise RuntimeError(f"llm failed: {err}")

def _parse(text):
    m = re.search(r"```(?:json)?\s*(.*?)```", text, re.S)
    raw = m.group(1) if m else text
    try:
        return json.loads(raw)
    except json.JSONDecodeError:
        start = min(i for i in (raw.find("{"), raw.find("[")) if i >= 0)
        return json.loads(raw[start: max(raw.rfind("}"), raw.rfind("]")) + 1])

def chat_json(system, user, **kw):
    for attempt in range(4):
        u = user if attempt == 0 else user + f"\n\n(Attempt {attempt+1}: your last reply was not valid JSON. Escape every double quote inside strings.)"
        text = chat(system, u, **kw)
        try:
            return _parse(text)
        except Exception:
            print("json retry", kw.get("tag"), flush=True)
    raise RuntimeError("no valid JSON")
