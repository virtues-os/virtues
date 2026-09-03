#!/usr/bin/env python3
"""Generate the README architecture diagram (light + dark).

GitHub's markdown sanitizer strips inline <svg>, so the diagram ships as two
committed files referenced through the same <picture> switch the headings use.
Edit here and re-run; never hand-edit the .svg files.

    python3 .github/images/architecture/gen.py
"""
import os

OUT = os.path.dirname(os.path.abspath(__file__))
SERIF = "'Times New Roman', Times, serif"
MONO = "ui-monospace, SFMono-Regular, Menlo, monospace"
SANS = "-apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif"

THEMES = {
    "light": dict(ink="#1f2328", dim="#57606a", rule="#d0d7de", wash="#f6f8fa", accent="#0969da"),
    "dark":  dict(ink="#e6edf3", dim="#9198a1", rule="#3d444d", wash="#161b22", accent="#4493f8"),
}

W, H = 900, 545


def box(x, y, w, h, *, dashed=False, t=None):
    d = ' stroke-dasharray="5 4"' if dashed else ""
    return (f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="7" fill="{t["wash"]}" '
            f'stroke="{t["rule"]}" stroke-width="1"{d}/>')


def text(x, y, s, *, size=13, font=SANS, fill=None, weight=400, anchor="start", t=None):
    return (f'<text x="{x}" y="{y}" font-family="{font}" font-size="{size}" '
            f'font-weight="{weight}" fill="{fill or t["ink"]}" text-anchor="{anchor}">{s}</text>')


def arrow(x1, y1, x2, y2, t):
    return (f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{t["dim"]}" '
            f'stroke-width="1.4" marker-end="url(#a)"/>')


def render(t):
    p = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" '
        f'viewBox="0 0 {W} {H}" role="img" aria-label="Virtues architecture: sources flow '
        f'into Virtues Core on port 8000, which uses your own inference endpoints on ports '
        f'18181 and 18182 and reaches external providers only through the virtues-api proxy '
        f'on port 9002.">',
        f'<defs><marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="6" '
        f'markerHeight="6" orient="auto"><path d="M0 0L10 5L0 10z" fill="{t["dim"]}"/>'
        f'</marker></defs>',
    ]
    # ── Sources ──────────────────────────────────────────────────────────
    p += [box(24, 16, 560, 84, t=t),
          text(44, 42, "Sources", size=17, font=SERIF, t=t),
          text(44, 66, "OAuth", size=11.5, fill=t["dim"], t=t),
          text(92, 66, "Google · Notion · Plaid · Strava · GitHub", size=12.5, t=t),
          text(44, 86, "Device", size=11.5, fill=t["dim"], t=t),
          text(92, 86, "HealthKit · Location · Microphone · Contacts · FinanceKit", size=12.5, t=t),
          arrow(304, 100, 304, 134, t)]
    # ── Core ─────────────────────────────────────────────────────────────
    p += [box(24, 136, 560, 196, t=t),
          text(44, 164, "Virtues Core", size=17, font=SERIF, t=t),
          text(154, 164, "one Rust binary · :8000", size=12, font=MONO, fill=t["dim"], t=t)]
    for i, label in enumerate(["Ingest", "Transform", "Wiki &amp;\nEntities", "AI agent\n+ tools"]):
        x = 44 + i * 133
        p.append(box(x, 182, 120, 60, t=t))
        for j, line in enumerate(label.split("\n")):
            p.append(text(x + 60, 209 + j * 15 - (7 if "\n" in label else 0),
                          line, size=12.5, anchor="middle", t=t))
    p += [text(44, 274, "Postgres — the ontology, the wiki, the narrative", size=12.5, t=t),
          text(44, 296, "File store — recordings, documents, attachments", size=12.5, t=t),
          text(44, 318, "Never holds an external API key.", size=12, fill=t["dim"], t=t)]
    # ── Inference (theirs) ───────────────────────────────────────────────
    p += [arrow(584, 214, 626, 214, t),
          box(628, 150, 248, 128, dashed=True, t=t),
          text(648, 178, "Inference — yours", size=16, font=SERIF, t=t),
          text(648, 204, "/v1/embeddings", size=12, font=MONO, t=t),
          text(856, 204, ":18181", size=12, font=MONO, fill=t["dim"], anchor="end", t=t),
          text(648, 224, "/v1/rerank", size=12, font=MONO, t=t),
          text(856, 224, ":18182", size=12, font=MONO, fill=t["dim"], anchor="end", t=t),
          text(648, 252, "loopback, LAN or VPN only —", size=11.5, fill=t["dim"], t=t),
          text(648, 268, "a public address is refused.", size=11.5, fill=t["dim"], t=t)]
    # ── virtues-api ──────────────────────────────────────────────────────
    p += [arrow(304, 332, 304, 372, t),
          box(24, 374, 560, 96, t=t),
          text(44, 402, "virtues-api", size=17, font=SERIF, t=t),
          text(148, 402, "the metered edge · :9002", size=12, font=MONO, fill=t["dim"], t=t),
          text(44, 428, "Holds the external keys — AI gateway, web search, Plaid, OAuth", size=12.5, t=t),
          text(44, 450, "and enforces a per-account budget.", size=12.5, t=t)]
    # ── Footnote ─────────────────────────────────────────────────────────
    p += [text(24, 502, "Sources, Core and inference all run on hardware you own.",
               size=12.5, fill=t["dim"], t=t),
          text(24, 522, "The model that writes is the one thing that leaves, and it "
                        "leaves only through virtues-api.", size=12.5, fill=t["dim"], t=t)]
    p.append("</svg>")
    return "\n".join(p) + "\n"


for name, t in THEMES.items():
    with open(os.path.join(OUT, f"architecture-{name}.svg"), "w") as f:
        f.write(render(t))
    print(f"architecture-{name}.svg  {W}x{H}")
