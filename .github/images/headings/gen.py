#!/usr/bin/env python3
"""Generate Times-400 serif heading SVGs (light + dark) for the README."""
import os, re
from fontTools.ttLib import TTFont

FONT = "/System/Library/Fonts/Supplemental/Times New Roman.ttf"
OUT = os.path.dirname(os.path.abspath(__file__))
LIGHT, DARK = "#1f2328", "#e6edf3"

font = TTFont(FONT)
upm = font["head"].unitsPerEm
cmap = font.getBestCmap()
hmtx = font["hmtx"]

def text_width(s, size):
    total = 0
    for ch in s:
        gid = cmap.get(ord(ch))
        adv = hmtx[gid][0] if gid else hmtx["space"][0]
        total += adv
    return total / upm * size

def slug(s):
    return re.sub(r"[^a-z0-9]+", "-", s.lower()).strip("-")

def emit(text, size, weight, prefix):
    w = round(text_width(text, size)) + 2
    vh = round(size * 1.32)
    baseline = round(size * 1.02)
    for name, fill in (("light", LIGHT), ("dark", DARK)):
        svg = (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{vh}" '
            f'viewBox="0 0 {w} {vh}" role="img" aria-label="{text}">\n'
            f'  <text x="0" y="{baseline}" font-family="\'Times New Roman\', Times, serif" '
            f'font-size="{size}" font-weight="{weight}" fill="{fill}">{text}</text>\n'
            f'</svg>\n'
        )
        with open(os.path.join(OUT, f"{prefix}-{name}.svg"), "w") as f:
            f.write(svg)
    return w, vh

# (text, font-size, weight, file-prefix)
HEADINGS = [
    ("Virtues", 34, 400, "h1-virtues"),
]
H2 = [
    "What It Does", "Architecture", "Data Sources", "Overview",
    "Install (Linux home server)", "Reaching your box",
    "Development", "Cloud sidecar", "iOS App", "Project Structure",
    "Database Schema", "The Day Pipeline", "Features", "License",
]
H3 = [
    "Requirements", "Inference", "Install in one command", "Prerequisites",
    "Setup", "Run",
    "virtues-api (required for AI features)", "Segmentation & Narration",
    "Event Novelty", "Topic & Entity Novelty", "Live Rhythm",
]
for h in H2:
    HEADINGS.append((h, 24, 400, "h2-" + slug(h)))
for h in H3:
    HEADINGS.append((h, 20, 400, "h3-" + slug(h)))

for text, size, weight, prefix in HEADINGS:
    w, vh = emit(text, size, weight, prefix)
    print(f"{prefix:40s} {w:>4}x{vh:<3} h={vh}")
