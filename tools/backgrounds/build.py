#!/usr/bin/env python3
"""Build one image style of home backgrounds from a folder of pictures.

The source folder is laid out as the painting sets are:

    <source>/new-chat-light/<collection>/<name>.<jpg|png|webp>
    <source>/new-chat-dark/<collection>/<name>.<jpg|png|webp>

and a prompts file lists each picture's title, its path and the prompt that
made it. The tool writes, under --out:

    manifest.json                 every picture with its light and dark versions
    <collection>/<theme>/<name>-<width>.avif / .webp
    review.html                   the light and dark pairs side by side

A picture is a light version and a dark version. The two sets hold different
places, so the pairs are chosen by hand and kept in pairs/<style>.tsv beside
this script. When a collection has no pairs yet the tool proposes them from
the words of each picture's title, file name and prompt, and writes them to
that file. Your corrections in the file win on every later run.

The images are large and stay out of git. Only this script and the pairs
files are tracked. See agents/plan/backgrounds-plan.md.

    python3 tools/backgrounds/build.py --style watercolors --title "Watercolors" \\
        --source ~/Desktop/muted-paintings/in-app \\
        --prompts ~/Desktop/muted-paintings/prompts.txt \\
        --out ~/virtues-backgrounds/watercolors
"""

from __future__ import annotations

import argparse
import html
import json
import math
import os
import re
import sys
from collections import Counter
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import numpy as np
from PIL import Image

THEMES = ("light", "dark")
WIDTHS = (1792, 896)
IMAGE_EXTS = (".jpg", ".jpeg", ".png", ".webp")
AVIF = {"quality": 55, "speed": 6}
WEBP = {"quality": 80, "method": 4}
PAIRS_DIR = Path(__file__).resolve().parent / "pairs"

# Words every prompt in a set shares say nothing about the place, and the
# inverse document frequency already discounts them. These are the ones short
# enough to survive that and still match by accident.
STOP = set(
    "a an and the of in on at to with by for from its it is as or into over under "
    "no not one two few very soft wide calm this that their his her up down out "
    "like only fill fills more".split()
)


# ---------------------------------------------------------------- sources


def scan_source(source: Path) -> dict[str, dict[str, dict[str, Path]]]:
    """theme -> collection -> picture name -> file."""
    found: dict[str, dict[str, dict[str, Path]]] = {}
    for theme in THEMES:
        root = source / f"new-chat-{theme}"
        if not root.is_dir():
            sys.exit(f"build.py: {root} is missing; --source must hold new-chat-light and new-chat-dark")
        found[theme] = {}
        for col in sorted(p for p in root.iterdir() if p.is_dir()):
            files = {p.stem: p for p in sorted(col.iterdir()) if p.suffix.lower() in IMAGE_EXTS}
            if files:
                found[theme][col.name] = files
    light, dark = set(found["light"]), set(found["dark"])
    if light != dark:
        sys.exit(f"build.py: collections differ between light and dark: {sorted(light ^ dark)}")
    for col in light:
        nl, nd = len(found["light"][col]), len(found["dark"][col])
        if nl != nd:
            sys.exit(f"build.py: {col} has {nl} light pictures and {nd} dark ones; a pair needs one of each")
    return found


ENTRY = re.compile(r"^(?P<title>\S.*?)\s{2,}\(new-chat-(?P<theme>light|dark)/(?P<col>[^/]+)/(?P<name>[^)]+)\)")
RULE = re.compile(r"^=+\s*$")


def parse_prompts(path: Path) -> tuple[dict[tuple[str, str, str], dict], dict[str, str]]:
    """(theme, collection, name) -> {title, prompt}, and collection -> title.

    A collection's title is the heading between two rules of '=' signs, the
    one before its first entry.
    """
    lines = path.read_text(encoding="utf-8").splitlines()
    entries: dict[tuple[str, str, str], dict] = {}
    col_titles: dict[str, str] = {}
    heading = None
    i = 0
    while i < len(lines):
        line = lines[i]
        if RULE.match(line) and i + 2 < len(lines) and RULE.match(lines[i + 2]):
            heading = lines[i + 1].strip()
            i += 3
            continue
        m = ENTRY.match(line)
        if m:
            prompt = []
            j = i + 1
            while j < len(lines) and lines[j].strip() and not ENTRY.match(lines[j]) and not RULE.match(lines[j]):
                prompt.append(lines[j].strip())
                j += 1
            key = (m["theme"], m["col"], m["name"].strip())
            entries[key] = {"title": m["title"].strip(), "prompt": " ".join(prompt)}
            if heading and m["col"] not in col_titles:
                col_titles[m["col"]] = heading
            i = j
            continue
        i += 1
    return entries, col_titles


def fallback_title(name: str) -> str:
    return name.replace("-", " ").capitalize()


# ---------------------------------------------------------------- pairing


def words(text: str) -> list[str]:
    return [w for w in re.findall(r"[a-z]+", text.lower()) if w not in STOP and len(w) > 2]


def document(title: str, name: str, prompt: str) -> Counter:
    # The title and file name are the place itself; weight them over the prompt.
    return Counter(words(title) * 3 + words(name.replace("-", " ")) * 3 + words(prompt))


def similarity_matrix(lights: list[Counter], darks: list[Counter], idf: dict[str, float]) -> np.ndarray:
    def vec(c: Counter) -> dict[str, float]:
        v = {w: n * idf.get(w, 0.0) for w, n in c.items()}
        norm = math.sqrt(sum(x * x for x in v.values())) or 1.0
        return {w: x / norm for w, x in v.items()}

    lv, dv = [vec(c) for c in lights], [vec(c) for c in darks]
    sim = np.zeros((len(lv), len(dv)))
    for i, a in enumerate(lv):
        for j, b in enumerate(dv):
            sim[i, j] = sum(x * b.get(w, 0.0) for w, x in a.items())
    return sim


def assign(cost: np.ndarray) -> list[int]:
    """Minimum-cost perfect matching on a square matrix (Hungarian method).

    Returns col[i] for each row i. Twelve by twelve, so the O(n^3) textbook
    version is plenty.
    """
    n = cost.shape[0]
    INF = float("inf")
    u, v = [0.0] * (n + 1), [0.0] * (n + 1)
    p, way = [0] * (n + 1), [0] * (n + 1)
    for i in range(1, n + 1):
        p[0] = i
        j0 = 0
        minv = [INF] * (n + 1)
        used = [False] * (n + 1)
        while True:
            used[j0] = True
            i0, delta, j1 = p[j0], INF, 0
            for j in range(1, n + 1):
                if not used[j]:
                    cur = cost[i0 - 1, j - 1] - u[i0] - v[j]
                    if cur < minv[j]:
                        minv[j], way[j] = cur, j0
                    if minv[j] < delta:
                        delta, j1 = minv[j], j
            for j in range(n + 1):
                if used[j]:
                    u[p[j]] += delta
                    v[j] -= delta
                else:
                    minv[j] -= delta
            j0 = j1
            if p[j0] == 0:
                break
        while j0:
            j1 = way[j0]
            p[j0] = p[j1]
            j0 = j1
    col = [0] * n
    for j in range(1, n + 1):
        col[p[j] - 1] = j - 1
    return col


def read_pairs(path: Path) -> dict[str, list[tuple[str, str]]]:
    pairs: dict[str, list[tuple[str, str]]] = {}
    if not path.exists():
        return pairs
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) < 3:
            sys.exit(f"build.py: {path}:{n}: expected collection, light and dark separated by tabs")
        pairs.setdefault(parts[0], []).append((parts[1], parts[2]))
    return pairs


def check_pairs(col: str, pairs: list[tuple[str, str]], found) -> str | None:
    lights, darks = [a for a, _ in pairs], [b for _, b in pairs]
    if sorted(lights) != sorted(found["light"][col]):
        return "its light names do not match the folder, each exactly once"
    if sorted(darks) != sorted(found["dark"][col]):
        return "its dark names do not match the folder, each exactly once"
    return None


def propose_pairs(found, entries) -> tuple[dict[str, list[tuple[str, str]]], dict[tuple[str, str, str], float]]:
    docs: dict[tuple[str, str, str], Counter] = {}
    for theme in THEMES:
        for col, files in found[theme].items():
            for name in files:
                e = entries.get((theme, col, name), {})
                docs[(theme, col, name)] = document(e.get("title", ""), name, e.get("prompt", ""))
    df = Counter(w for d in docs.values() for w in set(d))
    total = len(docs)
    idf = {w: math.log(total / n) for w, n in df.items()}

    proposed: dict[str, list[tuple[str, str]]] = {}
    scores: dict[tuple[str, str, str], float] = {}
    for col in sorted(found["light"]):
        ln, dn = list(found["light"][col]), list(found["dark"][col])
        sim = similarity_matrix([docs[("light", col, n)] for n in ln], [docs[("dark", col, n)] for n in dn], idf)
        match = assign(-sim)
        proposed[col] = [(ln[i], dn[match[i]]) for i in range(len(ln))]
        for i in range(len(ln)):
            scores[(col, ln[i], dn[match[i]])] = float(sim[i, match[i]])
    return proposed, scores


def write_pairs(path: Path, style: str, pairs: dict[str, list[tuple[str, str]]], scores) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    out = [
        f"# Light and dark pairs for the {style} style. One picture per line:",
        "# collection, light name, dark name, separated by tabs. Edit freely; build.py",
        "# keeps this file and only proposes pairs for a collection that has none.",
        "# A fourth column, where present, is the proposal's word similarity (0-1).",
    ]
    for col in sorted(pairs):
        for light, dark in sorted(pairs[col]):
            score = scores.get((col, light, dark))
            out.append("\t".join([col, light, dark] + ([f"{score:.2f}"] if score is not None else [])))
    path.write_text("\n".join(out) + "\n", encoding="utf-8")


# ---------------------------------------------------------------- images


def analyze(img: Image.Image) -> dict:
    """Luminance, focus box and fade side, measured on a small copy.

    - luminance: mean relative luminance (0-1, as WCAG defines it) over the
      painted pixels, weighted by alpha, so the app can tell a bright picture
      from a dark one.
    - focus: [x, y, w, h] as fractions of the picture, the box holding the
      middle 70% of edge energy on each axis. A rough stand-in for where the
      subject is, for cropping; not a saliency model.
    - fade: which edge is left unpainted (transparent): top, bottom or none.
    """
    small = img.convert("RGBA").resize((448, round(448 * img.height / img.width)), Image.LANCZOS)
    a = np.asarray(small).astype(np.float64) / 255.0
    rgb, alpha = a[..., :3], a[..., 3]
    linear = np.where(rgb <= 0.04045, rgb / 12.92, ((rgb + 0.055) / 1.055) ** 2.4)
    lum = 0.2126 * linear[..., 0] + 0.7152 * linear[..., 1] + 0.0722 * linear[..., 2]
    weight = alpha.sum() or 1.0
    luminance = float((lum * alpha).sum() / weight)

    gy, gx = np.gradient(lum)
    energy = np.hypot(gx, gy) * alpha

    def span(profile: np.ndarray) -> tuple[float, float]:
        c = np.cumsum(profile)
        if c[-1] <= 0:
            return 0.0, 1.0
        c /= c[-1]
        lo = int(np.searchsorted(c, 0.15))
        hi = int(np.searchsorted(c, 0.85))
        return lo / len(profile), (hi + 1) / len(profile)

    x0, x1 = span(energy.sum(axis=0))
    y0, y1 = span(energy.sum(axis=1))

    q = max(1, alpha.shape[0] // 4)
    top, bottom = float(alpha[:q].mean()), float(alpha[-q:].mean())
    if min(top, bottom) > 0.95:
        fade = "none"
    else:
        fade = "top" if top < bottom else "bottom"

    return {
        "luminance": round(luminance, 3),
        "focus": [round(x0, 3), round(y0, 3), round(x1 - x0, 3), round(y1 - y0, 3)],
        "fade": fade,
    }


def process(job: tuple[str, str, str, str, str]) -> tuple[str, str, str, dict]:
    """Encode one source picture at every width and measure it."""
    src, out_dir, col, theme, name = job
    img = Image.open(src)
    img.load()
    has_alpha = img.mode in ("RGBA", "LA") or (img.mode == "P" and "transparency" in img.info)
    img = img.convert("RGBA" if has_alpha else "RGB")
    files: dict[str, dict[str, str]] = {"avif": {}, "webp": {}}
    src_mtime = os.path.getmtime(src)
    for w in WIDTHS:
        if w > img.width:
            continue
        sized = img if w == img.width else img.resize((w, round(w * img.height / img.width)), Image.LANCZOS)
        for fmt, opts in (("avif", AVIF), ("webp", WEBP)):
            rel = f"{col}/{theme}/{name}-{w}.{fmt}"
            dest = Path(out_dir) / rel
            if not dest.exists() or dest.stat().st_mtime < src_mtime:
                dest.parent.mkdir(parents=True, exist_ok=True)
                sized.save(dest, **opts)
            files[fmt][str(w)] = rel
    meta = analyze(img)
    meta.update({"width": img.width, "height": img.height, "files": files})
    return col, theme, name, meta


# ---------------------------------------------------------------- outputs


def write_review(out: Path, style: str, manifest: dict, scores) -> None:
    rows = []
    for col in manifest["collections"]:
        rows.append(f'<h2>{html.escape(col["title"])} <code>{col["id"]}</code></h2><div class="grid">')
        for pic in col["pictures"]:
            lt, dk = pic["light"], pic["dark"]
            score = scores.get((col["id"], lt["source"], dk["source"]))
            note = f"{score:.2f}" if score is not None else "set by hand"
            rows.append(
                '<figure class="pair">'
                f'<div class="v light"><img loading="lazy" src="{lt["files"]["webp"]["896"]}" alt=""></div>'
                f'<div class="v dark"><img loading="lazy" src="{dk["files"]["webp"]["896"]}" alt=""></div>'
                f'<figcaption><span>{html.escape(lt["title"])}</span><span>{html.escape(dk["title"])}</span>'
                f'<code>{lt["source"]}\t{dk["source"]}</code><em>{note}</em></figcaption></figure>'
            )
        rows.append("</div>")
    doc = f"""<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{html.escape(style)} pairs</title>
<style>
:root {{ --bg:#f5f5f4; --ink:#1d2433; --mute:#6b7280; --line:#e2e2e0; --light:#ecebe8; --dark:#121315; }}
body {{ margin:0; padding:24px 16px 64px; background:var(--bg); color:var(--ink);
  font:14px/1.45 -apple-system, BlinkMacSystemFont, "Avenir Next", sans-serif; }}
main {{ max-width:1200px; margin:0 auto; }}
h1 {{ font-size:20px; margin:0 0 4px; }} p {{ color:var(--mute); margin:0 0 24px; }}
h2 {{ font-size:15px; margin:32px 0 12px; }} h2 code {{ color:var(--mute); font-weight:400; }}
.grid {{ display:grid; grid-template-columns:repeat(auto-fill, minmax(360px, 1fr)); gap:16px; }}
.pair {{ margin:0; border:1px solid var(--line); border-radius:10px; overflow:hidden; background:#fff; }}
.v {{ aspect-ratio:1792/768; }} .v img {{ width:100%; height:100%; display:block; }}
.light {{ background:var(--light); }} .dark {{ background:var(--dark); }}
figcaption {{ display:grid; grid-template-columns:1fr 1fr; gap:2px 12px; padding:8px 10px; font-size:13px; }}
figcaption code, figcaption em {{ grid-column:1 / -1; color:var(--mute); font-size:12px; font-style:normal; }}
</style></head><body><main>
<h1>{html.escape(style)}: light and dark pairs</h1>
<p>Each card is one picture: its light version over its dark one. To change a pair, edit
tools/backgrounds/pairs/{html.escape(style)}.tsv and run build.py again. The number is the
proposal's word similarity; "set by hand" means the pair came from the file as you edited it.</p>
{''.join(rows)}
</main></body></html>
"""
    (out / "review.html").write_text(doc, encoding="utf-8")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--style", required=True, help="style id, as it appears in the setting path")
    ap.add_argument("--title", required=True, help="the style's name in the pickers")
    ap.add_argument("--source", required=True, type=Path)
    ap.add_argument("--prompts", required=True, type=Path)
    ap.add_argument("--out", required=True, type=Path)
    ap.add_argument("--repropose", action="store_true", help="replace the pairs file with fresh proposals")
    ap.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) - 2))
    args = ap.parse_args()

    source, out = args.source.expanduser(), args.out.expanduser()
    found = scan_source(source)
    entries, col_titles = parse_prompts(args.prompts.expanduser())

    pairs_path = PAIRS_DIR / f"{args.style}.tsv"
    pairs = {} if args.repropose else read_pairs(pairs_path)
    proposed, scores = propose_pairs(found, entries)
    changed = False
    for col in sorted(found["light"]):
        if col in pairs:
            problem = check_pairs(col, pairs[col], found)
            if problem:
                sys.exit(f"build.py: {pairs_path}: {col}: {problem}. Fix the file or run with --repropose.")
        else:
            pairs[col] = proposed[col]
            changed = True
    for col in set(pairs) - set(found["light"]):
        print(f"build.py: {col} is in the pairs file but not in the source; dropping it", file=sys.stderr)
        del pairs[col]
        changed = True
    if changed:
        # Keep a score only where the pair is still the proposal.
        write_pairs(pairs_path, args.style, pairs, scores)
        print(f"wrote {pairs_path}")

    # Every id becomes part of a stored setting, which the apps read only in
    # this shape (lib/backgrounds/resolve.ts); any other name would make the
    # setting read as the default.
    names = [args.style, *(col for t in THEMES for col in found[t]), *(n for t in THEMES for c in found[t].values() for n in c)]
    bad = sorted({n for n in names if not re.fullmatch(r"[a-z0-9-]{1,64}", n)})
    if bad:
        sys.exit("names must be lowercase letters, digits and hyphens, at most 64 characters: " + ", ".join(bad))

    jobs = [
        (str(path), str(out), col, theme, name)
        for theme in THEMES
        for col, files in found[theme].items()
        for name, path in files.items()
    ]
    out.mkdir(parents=True, exist_ok=True)
    measured: dict[tuple[str, str, str], dict] = {}
    with ProcessPoolExecutor(max_workers=args.jobs) as pool:
        for n, (col, theme, name, meta) in enumerate(pool.map(process, jobs, chunksize=4), 1):
            measured[(theme, col, name)] = meta
            if n % 50 == 0 or n == len(jobs):
                print(f"  {n}/{len(jobs)} pictures", flush=True)

    collections = []
    for col in sorted(found["light"]):
        pictures = []
        for light, dark in sorted(pairs[col]):
            pic = {"id": light}
            for theme, name in (("light", light), ("dark", dark)):
                e = entries.get((theme, col, name), {})
                pic[theme] = {"source": name, "title": e.get("title") or fallback_title(name), **measured[(theme, col, name)]}
            pictures.append(pic)
        collections.append({"id": col, "title": col_titles.get(col) or fallback_title(col), "pictures": pictures})

    narrow = [
        f"{c['id']}/{p['id']} ({theme})"
        for c in collections
        for p in c["pictures"]
        for theme in THEMES
        if not p[theme]["files"]["webp"]
    ]
    if narrow:
        sys.exit(f"narrower than {min(WIDTHS)} px, so no file was made: " + ", ".join(narrow))

    manifest = {"version": 1, "style": args.style, "title": args.title, "renderer": "image", "collections": collections}
    (out / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n", encoding="utf-8")
    write_review(out, args.style, manifest, scores)
    print(f"wrote {out / 'manifest.json'} and {out / 'review.html'}: {len(collections)} collections, "
          f"{sum(len(c['pictures']) for c in collections)} pictures")


if __name__ == "__main__":
    main()
