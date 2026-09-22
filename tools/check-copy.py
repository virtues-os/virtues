#!/usr/bin/env python3
"""Copy lint — the UI-copy rules in agents/build/voice.md, counted.

Not a style nag. Every check here is a rule that has already cost something:
a passive sentence with no actor shipped on the interview's close; "the
machine" and "this box" named the same thing three ways across screens a
person walks between; "endpoint" and "implementation server" reached a
settings page; an em dash is the letter's punctuation, not the product's.

RATCHET, NOT GATE. The tree carries a backlog, so this fails only when a
count goes UP against tools/copy-baseline.json. Lower a number, commit the
new baseline with the fix, and the number can never climb back. Run with
--update to write the baseline after a pass that genuinely lowers one.

Scope is the INTERFACE only. docs/ is the manual and agents/ is the
workshop; both keep their em dashes by rule, and neither is scanned. The
founder's letter is signed by a person and is exempt for the same reason.
"""
import json, pathlib, re, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASELINE = ROOT / "tools" / "copy-baseline.json"

# The interface. Not docs/, not agents/ — those are a different register.
SURFACES = [
    ("apps/web/src", ("*.svelte", "*.ts")),
    ("apps/web/src-tauri/ui", ("*.html",)),
    ("virtues-core/src/api", ("*.rs",)),
    ("virtues-core/src/server", ("*.rs",)),
    ("applets", ("manifest.toml",)),
    ("services/virtues-atlas/src", ("email.rs",)),
    ("apps/web/plugins/reach/ios", ("*.swift",)),
]
# The letter is Adam's and literary on purpose; the components gallery is a
# developer page that quotes bad copy as examples; a test name is written for
# whoever reads the failure, not for a person using the product.
EXEMPT = ("onboarding/document/", "routes/(public)/components/", ".test.")

# Strings that match a rule and are right anyway. Each needs a reason, and
# "it reads fine to me" is not one — the point of the ratchet is that taste
# does not get a vote. Keep this list short enough to read.
ALLOW = (
    # A drawn rectangle on the lifeline, not the server.
    "Drag the box to move the window",
    # On the models and bring-your-own screens "the model" IS the subject of
    # the page. Calling it "your assistant" there would name the wrong thing.
    "Only if this endpoint names the model differently",
    # A state, not a hidden actor: nothing did the untouching or the setting up.
    "your subscription are untouched",
    "This release is already downloaded",
    "Your server is set up and works on the build it has",
    "Remote access is switched off",
    "while it waits to be found",
    # The wiki's colophon contrasts two authors on purpose: the record writes
    # everything else, and the person writes this one. Flattening either half
    # to the active voice loses the contrast that is the whole sentence.
    "Everything else here is written from the record",
    "Your document is written from the interview",
    # Addressed to a model (a tool result), or to whoever is reading the
    # known-gaps catalog in the code. Neither is a person using the product.
    "The content shown is truncated",
    "GAP: holdings are collected",
    "GAP: debts are collected",
    "What these runs spent with the model",
    "your server goes by the model id",
    "Every model the gateway carries",
)

# A past participle, for finding passives. "ne" and "nt" are NOT endings
# here: they matched "one", "done", "gone", "went" and turned "a chapter is
# one of the major arcs" into a passive. Irregulars are listed instead.
IRREGULAR = (
    "built|sent|kept|left|lost|made|found|put|set|read|held|told|meant|"
    "brought|bought|caught|taught|paid|said|sold|spent|split|shut|cut|hit|"
    "let|run|won|begun|done|gone|drawn|grown|known|shown|thrown|blown|flown"
)
PARTICIPLE = rf"(?:\w+(?:ed|en|wn)|{IRREGULAR})"

# Participles that are ordinary adjectives after "be": "the chat is open",
# "your server may be offline". A state, not an action with a hidden actor.
ADJECTIVAL = (
    "open|closed|done|involved|silent|offline|online|connected|linked|"
    "attached|ready|able|unable|empty|false|true|gone|gone|gray|grey|"
    "limited|advanced|mixed|related|interested|tired|used to"
)
CHECKS = {
    "passive-no-actor": re.compile(
        rf"\b(?:is|are|was|were|be|been|being)\s+(?:not\s+|never\s+|already\s+|still\s+)?"
        rf"(?!(?:{ADJECTIVAL})\b)(?:{PARTICIPLE})\b"),
    "em-dash":       re.compile(r"—"),
    "wrong-name":    re.compile(
        r"\b(?:the machine|this box|the box|your box|the AI|the model)\b", re.I),
    "dev-word":      re.compile(
        r"\b(?:endpoint|payload|instance|backend|partition|entity|provenance|"
        r"daemon|handler|implementation server)\b", re.I),
    "software-subject": re.compile(
        r"\b(?:lets? you|allows? you|enables? you|capability|functionality)\b", re.I),
    "banned-word":   re.compile(
        r"\b(?:please|simply|utilise|leverage|streamline|in order to|via|etc\.|e\.g\.|i\.e\.)\b", re.I),
    "we-in-prose":   re.compile(r"\b(?:we're|we'll|we could not|we couldn't|we recommend)\b", re.I),
    "exclamation":   re.compile(r"!(?:\s|$|\"|')"),
}

STRING = re.compile(r"""(['"`])((?:(?!\1)[^\\\n]|\\.){20,240})\1""")
TEXTNODE = re.compile(r">([^<>{}]{15,240})<")


def is_prose(s: str) -> bool:
    """A sentence a person reads, not an identifier, path, or fragment."""
    if re.search(r"[<>{}$#\\]|::|/api/|https?://|\.\w{2,4}$|^\s*\d", s):
        return False
    if not re.search(r"[a-z]{3}\s+\w+\s+\w+", s):
        return False
    return len(s.split()) >= 5


def strip_comments(src: str, suffix: str) -> str:
    """Remove everything written for a developer rather than a person."""
    src = re.sub(r"/\*.*?\*/|<!--.*?-->", " ", src, flags=re.S)
    if suffix == ".toml":
        return re.sub(r"^\s*#.*$", " ", src, flags=re.M)
    src = re.sub(r"^\s*(?://|///).*$", " ", src, flags=re.M)
    if suffix == ".rs":
        # A *_PROMPT constant is addressed to a model, not to a person. It is
        # governed by the prompt rules in agents/build/, not by this file.
        src = re.sub(r"const\s+\w*PROMPT\w*\s*:\s*&str\s*=\s*r#\".*?\"#;", " ",
                     src, flags=re.S)
        # A log line and a test assertion are both addressed to whoever is
        # debugging. They read like copy and are not: `tracing::error!(…,
        # "failed to resolve the model")` is a journal entry, and an
        # assert's message is only ever seen as a test failure.
        src = re.sub(r"(?:tracing::\w+!|println!|eprintln!|panic!)\s*\(.*?\);", " ",
                     src, flags=re.S)
        src = re.sub(r"^\s*assert(?:_\w+)?!\(.*?\);", " ", src, flags=re.M | re.S)
        src = re.sub(r'^\s*".*"\s*$(?=\s*\);)', " ", src, flags=re.M)
    return src


def scan():
    counts = {k: 0 for k in CHECKS}
    examples = {k: [] for k in CHECKS}
    scanned = 0
    for root, pats in SURFACES:
        base = ROOT / root
        if not base.exists():
            continue
        for pat in pats:
            for p in sorted(base.rglob(pat)):
                rel = str(p.relative_to(ROOT))
                if any(x in rel for x in EXEMPT):
                    continue
                src = strip_comments(p.read_text(errors="ignore"), p.suffix)
                cands = [m.group(2) for m in STRING.finditer(src)]
                if p.suffix in (".svelte", ".html"):
                    cands += [m.group(1) for m in TEXTNODE.finditer(src)]
                for raw in cands:
                    s = " ".join(raw.split())
                    if not is_prose(s):
                        continue
                    scanned += 1
                    if any(a in s for a in ALLOW):
                        continue
                    for name, rx in CHECKS.items():
                        if rx.search(s):
                            counts[name] += 1
                            if len(examples[name]) < 3:
                                examples[name].append(f"{rel}: {s[:88]}")
    return scanned, counts, examples


def main() -> int:
    scanned, counts, examples = scan()
    update = "--update" in sys.argv
    base = json.loads(BASELINE.read_text()) if BASELINE.exists() else {}

    worse, better = [], []
    for name in CHECKS:
        was, now = base.get(name), counts[name]
        if was is None:
            continue
        if now > was:
            worse.append((name, was, now))
        elif now < was:
            better.append((name, was, now))

    print(f"check-copy: {scanned} interface strings")
    for name in CHECKS:
        was = base.get(name)
        mark = "" if was is None or was == counts[name] else (
            f"  (was {was})" if counts[name] < was else f"  ↑ WAS {was}")
        print(f"  {name:<18} {counts[name]:>4}{mark}")

    if update:
        BASELINE.write_text(json.dumps(counts, indent=2, sort_keys=True) + "\n")
        print("\nbaseline written.")
        return 0

    for name, was, now in better:
        print(f"\n  ↓ {name}: {was} → {now}. Commit the new baseline: tools/check-copy.py --update")
    if worse:
        print("\nFAIL — new copy broke a rule that was already being worked down:\n")
        for name, was, now in worse:
            print(f"  {name}: {was} → {now}")
            for ex in examples[name][:3]:
                print(f"      {ex}")
        print("\n  The rules are in .claude/rules/copy.md. The one that matters most:")
        print("  a passive sentence is a missing subject — the person is the actor")
        print("  where the thing is theirs, the server is named where it failed.")
        return 1
    print("\nok — nothing got worse.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
