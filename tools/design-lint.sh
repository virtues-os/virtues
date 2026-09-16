#!/usr/bin/env bash
# Design lint: count the things agents/build/design-grammar.md §9 says to count.
#
# Run from anywhere:   bash tools/design-lint.sh
#
# §9 ("The checklist") states countable limits — shadows 0, mono outside a chart
# or a clock 0, sizes under 11px or with a half 0, radii {12, pills, circles} —
# and nothing has ever enforced them. tools/arch-lint.sh enforces architecture
# with the same rigor and contains zero design lints, so while the Rust
# invariants held, the page grammar drifted: 56 shadows, 305 hex literals and
# 15 distinct radii accumulated inside a document whose §6 says "Nothing else."
# Getting Started was redesigned eight times on 2026-09-04 because every review
# was taste against taste; a countable rule that nobody counts is taste again.
#
# A RATCHET, not a wall — the same shape as tools/check-swallowed-queries.sh and
# for the same reason. The debt is too large to pay today, and a check that is
# red on day one is switched off by the end of the week, which is worse than no
# check. tools/design-lint.baseline freezes today's count per rule; this fails
# only when a count EXCEEDS its baseline, and says so out loud when one drops.
#
# Counts, not signatures. check-swallowed-queries.sh can key on (file, source
# line) because ~200 entries stay legible; 500+ design violations would not, and
# a baseline nobody reads is not a baseline. The cost is that a failure names
# every violation of the offending rule rather than only the new one — which is
# the right trade when the fix is "put it back under the limit", not "find mine".
#
#   Escape one line:  put `design-ok: <why>` in a comment on it or above it
#   See every hit:    bash tools/design-lint.sh --list [rule]
#   Re-baseline:      bash tools/design-lint.sh --update-baseline
#
# PORTABILITY. CI is Linux, most authoring here happens on macOS, and this repo
# has already shipped a lint that passed on one and failed on the other (`grep
# -A6` context behavior differs between BSD and GNU grep). So: no `grep -P`, no
# GNU-only `sed -i`, no `readlink -f`, no `{n}` interval expressions in awk
# regexes (the original awk lacks them). The scan is one POSIX awk program.

set -uo pipefail
cd "$(dirname "$0")/.."

BASELINE="tools/design-lint.baseline"
SRC="apps/web/src"

# ─── Exemptions ────────────────────────────────────────────────────────────
#
# Color literals: `src/themes.css` and `src/app.css` are where literals are
# SUPPOSED to live — they define the tokens everything else must reference.
#
# `wiki/LifelineCanvas.svelte` and `wiki/LifelineMap.svelte` are the two files
# that paint into a canvas 2D context. `ctx.fillStyle` cannot resolve `var()`,
# so they read the token with `getComputedStyle().getPropertyValue('--x')` and
# need a literal as the fallback when the property is empty. That is the
# correct pattern, not debt. Named, not globbed, and deliberately short:
# `DaylineChart.svelte` (56 hex), `DaylineStrip.svelte` (13) and
# `ChapterLifeline.svelte` (2) are SVG and CSS, which take `var()` fine — their
# literals are real debt and stay in the baseline where they can be paid down.
#
# Mono: `themes.css` declares `--font-mono`, and `lib/codemirror/theme.css`
# dresses a code editor, where mono IS the content.
#
# Mono literals: the same two, plus `app.css`, which is where the `@font-face`
# rules for IBM Plex Mono live — naming the family you are REGISTERING is not
# dressing a label in it, exactly as `themes.css` declaring `--font-mono` is
# not. Plus the five surfaces where the mono is the
# rendered artifact rather than a costume on a label — a terminal emulator
# (`DeveloperTerminalView`), a source viewer (`AppletSource`), a code view
# (`CodeInterpreterCard`), a diff view (`EditDiffCard`) and the raw text/CSV
# pane (`asset/TextPane`). Same test `codemirror/theme.css` passes: the thing
# inside the box is code, so a fixed advance is information, not decoration.
# Deliberately NOT exempt: `chat/PageEditResult.svelte`, whose `.result-title`
# is a page TITLE wearing a mono stack — that is exactly the kicker the grammar
# names, and it stays in the baseline where it can be paid down.
COLOR_EXEMPT='apps/web/src/themes.css apps/web/src/app.css apps/web/src/lib/components/wiki/LifelineCanvas.svelte apps/web/src/lib/components/wiki/LifelineMap.svelte'
MONO_EXEMPT='apps/web/src/themes.css apps/web/src/lib/codemirror/theme.css'
MONO_LITERAL_EXEMPT="$MONO_EXEMPT apps/web/src/app.css apps/web/src/lib/components/tabs/views/DeveloperTerminalView.svelte apps/web/src/lib/components/applets/AppletSource.svelte apps/web/src/lib/components/chat/CodeInterpreterCard.svelte apps/web/src/lib/components/chat/EditDiffCard.svelte apps/web/src/lib/components/asset/TextPane.svelte"

RULES='shadow tiny-type half-px-type mono mono-literal hex-literal rgb-literal radius off-grid no-reduced-motion serif-impossible'

describe() {
    case "$1" in
    shadow)            echo "box-shadow in the pane (§6: a card is a hairline on --surface, never a shadow)" ;;
    tiny-type)         echo "font-size under 11px (§4: nothing under 11px)" ;;
    half-px-type)      echo "half-pixel font-size (§4: no 12.5 / 13.5)" ;;
    mono)              echo "var(--font-mono) outside a chart axis or a clock time (§4)" ;;
    mono-literal)      echo "a raw monospace font stack outside a chart axis or a clock time (§4)" ;;
    hex-literal)       echo "hardcoded hex color in a component (§5: theme tokens, never literals in the pane)" ;;
    rgb-literal)       echo "hardcoded rgb()/rgba() in a component (§5)" ;;
    radius)            echo "border-radius outside {0, 6px, 12px, 50%, a pill} (§6: Nothing else)" ;;
    off-grid)          echo "off-grid px in padding/margin/gap (§6: the 8pt grid — 8·12·16·20·24·32…)" ;;
    no-reduced-motion) echo "file animates with no prefers-reduced-motion block (§7)" ;;
    serif-impossible)  echo "the serif set bold/medium or italic — JJannon ships ONE cut (agents/build/typography.md)" ;;
    esac
}

# `routes/(public)/components/` is the design gallery: a workbench that renders
# every primitive, every token and a contrast matrix. It is a TOOL, not a
# product surface, and the page grammar governs pages. It legitimately needs
# what the grammar forbids — mono for token names and hex values, sub-11px type
# for a dense matrix, off-grid gaps in a spec table — and linting it would
# force the one page that makes design debt visible to fake compliance.
# Excluded whole rather than per-rule, because no rule applies to it.
#
# `serif-impossible` below depends on this exclusion for correctness, not only
# for noise: that page renders "Faked bold" and "Faked italic" side by side as
# SPECIMENS of the very thing the rule forbids, to show what a browser does
# with a one-cut family. Flagging a demonstration of a bug as the bug is how a
# lint teaches people to delete the explanation. If this exclusion is ever
# narrowed, those two lines need `design-ok:` first.
file_list() {
    find "$SRC" -type f \( -name '*.svelte' -o -name '*.css' \) \
        ! -name '*.test.ts' \
        ! -path '*/routes/(public)/components/*' \
        | LC_ALL=C sort
}

# ─── The scan ──────────────────────────────────────────────────────────────
# Emits one TAB-separated record per violation: rule, file, line, source.
# Rules 1–8 and 10 are per-line and therefore safe if xargs splits the file list
# across several awk invocations. Rule 9 (no-reduced-motion) is whole-file, so
# it runs as its own pass below rather than depending on awk's END block. Rule
# 11 (serif-impossible) is per-BLOCK: it buffers a declaration block and reports
# when the block closes, which still happens inside the file it belongs to, so a
# split file list cannot lose or mix it either.
scan() {
    file_list | tr '\n' '\0' | xargs -0 awk -v color_exempt="$COLOR_EXEMPT" -v mono_exempt="$MONO_EXEMPT" -v mono_literal_exempt="$MONO_LITERAL_EXEMPT" '
    function emit_at(rule, ln, src,   s) {
        s = src
        gsub(/\t/, " ", s)
        sub(/^[ \t]+/, "", s); sub(/[ \t]+$/, "", s)
        if (length(s) > 110) s = substr(s, 1, 107) "..."
        printf "%s\t%s\t%d\t%s\n", rule, FILENAME, ln, s
    }
    function emit(rule, src) { emit_at(rule, FNR, src) }

    # Close one declaration block for rule 11: if this block asked for the
    # serif, every weight/slope request buffered inside it is impossible.
    function serif_close(d,   i) {
        if (serifFamily[d])
            for (i = 1; i <= serifN[d]; i++) emit_at("serif-impossible", serifLn[d, i], serifSrc[d, i])
        serifFamily[d] = 0; serifN[d] = 0
    }
    function serif_reset(   d) { for (d = 0; d <= MAXD; d++) { serifFamily[d] = 0; serifN[d] = 0 } depth = 0 }

    BEGIN {
        MAXD = 32
        n = split(color_exempt, a, " "); for (i = 1; i <= n; i++) noColor[a[i]] = 1
        n = split(mono_exempt,  a, " "); for (i = 1; i <= n; i++) noMono[a[i]]  = 1
        n = split(mono_literal_exempt, a, " "); for (i = 1; i <= n; i++) noMonoLit[a[i]] = 1
    }
    FNR == 1 { prev = ""; serif_reset() }
    {
        line = $0
        t = line; sub(/^[ \t]+/, "", t)
        # Commented-out CSS is not shipped CSS. Same exclusion arch-lint.sh makes
        # for `//` doc comments naming a forbidden type.
        isComment = (t ~ /^\/\//) || (t ~ /^\/\*/) || (t ~ /^\*/) || (t ~ /^<!--/)
        # The escape hatch, modelled on `// absent-ok:` in the swallowed-query
        # check: a real chart axis or clock time says so, on the line or above it.
        escaped = (line ~ /design-ok:/) || (prev ~ /design-ok:/)
        prev = line
        if (isComment || escaped) next

        # ── 1. Shadows ────────────────────────────────────────────────────
        # §6: "No shadows in the pane." A shadow fakes depth that the spread
        # gets from the painting; every card that grew one read as a widget.
        # `box-shadow: none` is a REMOVAL of a shadow, so it is not one.
        if (line ~ /box-shadow[ \t]*:/ && line !~ /box-shadow[ \t]*:[ \t]*none/)
            emit("shadow", line)

        # ── 2/3. Type size ────────────────────────────────────────────────
        # §4: nothing under 11px, no half-pixel sizes. Both were the register
        # tell — 9.5px mono kickers are what made eight builds read as "an old
        # Windows XP thing" rather than as a page.
        s = line
        while (match(s, /font-size[ \t]*:[ \t]*[0-9]+(\.[0-9]+)?px/)) {
            num = substr(s, RSTART, RLENGTH)
            s = substr(s, RSTART + RLENGTH)
            sub(/^font-size[ \t]*:[ \t]*/, "", num); sub(/px$/, "", num)
            if (num + 0 < 11)  emit("tiny-type", line)
            if (num ~ /\./)    emit("half-px-type", line)
        }

        # ── 4. Mono ───────────────────────────────────────────────────────
        # §4: "No mono outside a chart’s own labels and a clock time." Mono
        # kickers were named the single strongest tell of the dated register.
        # A genuine chart label or clock annotates itself with `design-ok:`.
        if (!(FILENAME in noMono)) {
            s = line
            while (match(s, /var\(--font-mono\)/)) {
                s = substr(s, RSTART + RLENGTH)
                emit("mono", line)
            }
        }

        # ── 10. Mono spelled as a literal stack ───────────────────────────
        # Rule 4 only sees the exact string `var(--font-mono)`, which is one
        # spelling out of many: `var(--font-mono, ui-monospace, monospace)`,
        # `ui-monospace, SFMono-Regular, Menlo, monospace` and `"IBM Plex
        # Mono", monospace` all set the same face and were all invisible to it,
        # so the mono count read as a fifth of the truth. §4 governs the FACE,
        # not the syntax that reaches it — a mono kicker is the strongest tell
        # of the dated register whether it arrives through a token or a stack.
        #
        # Requires the property on the line (`font-family`, `fontFamily`, or
        # the `font:` shorthand) rather than matching the word `monospace`
        # anywhere: app.css and typography prose discuss monospace in running
        # comments that are not full-line comments, and a lint that flags a
        # sentence about a rule is a lint people learn to ignore. The cost is
        # that a stack handed to `style:font-family={...}` across several lines
        # is missed (there is one, in `pages/DisplaySettingsPopover.svelte`).
        # One record per line: a font-family declaration is one decision.
        if (!(FILENAME in noMonoLit) \
            && line ~ /(font-family|fontFamily|font)[ \t]*:/ \
            && line ~ /monospace|SFMono|SF Mono|Menlo|Monaco|Consolas|Courier|IBM Plex Mono/)
            emit("mono-literal", line)

        if (!(FILENAME in noColor)) {
            # ── 5. Hex literals ───────────────────────────────────────────
            # §5: "Paper and ink from the theme tokens, never literals in the
            # pane." A literal cannot follow the theme, so every one of these
            # is a light-mode color sitting in a dark-mode page (or the
            # reverse) waiting for someone to switch themes and see it.
            # 3/6/8 digits only, and the next character must not be a word
            # character — that is what keeps `{#each}` and `#some-anchor` out
            # without needing a lookahead (which POSIX ERE has not got).
            s = line
            while (match(s, /#[0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]([0-9a-fA-F][0-9a-fA-F][0-9a-fA-F]([0-9a-fA-F][0-9a-fA-F])?)?/)) {
                after = substr(s, RSTART + RLENGTH, 1)
                s = substr(s, RSTART + RLENGTH)
                if (after !~ /[0-9A-Za-z_-]/) emit("hex-literal", line)
            }

            # ── 6. rgb()/rgba() literals ──────────────────────────────────
            # Same rule as the hex one, different spelling. `rgba(var(--x-rgb),
            # …)` IS token-driven — that is the supported way to take a token
            # at partial opacity — so it is not a violation.
            s = line
            while (match(s, /rgba?\(/)) {
                s = substr(s, RSTART + RLENGTH)
                if (s !~ /^[ \t]*var\(/) emit("rgb-literal", line)
            }
        }

        # ── 7. Radius ─────────────────────────────────────────────────────
        # §6: "Radius is {0, 6px, 12px, 50%, a pill} and nothing else." 12px
        # on a card in the pane; a pill on a button; 50% on a dot; 6px inside
        # the shell. design.md yields to the grammar on radius specifically, so
        # there is one answer. (This quoted the struck frontispiece and the
        # struck stepper’s dots until 2026-09-16 — the doc moved, the lint’s
        # own citation did not, which is the failure this whole ratchet is
        # against.)
        # Custom properties whose name ends in -radius are checked too: four of
        # them (--card-radius: 4px, --cell-radius: 2px, --ref-pill-radius,
        # --md-inline-code-radius) currently launder an off-grammar value
        # behind a var() that the value check would otherwise wave through.
        # A px value of 100 or more is a pill by construction — the radius
        # exceeds any plausible half-height, so the corner renders as a capsule.
        s = line
        while (match(s, /(border[a-z-]*radius|--[a-z-]*radius[a-z-]*)[ \t]*:[^;}"'"'"']*/)) {
            val = substr(s, RSTART, RLENGTH)
            s = substr(s, RSTART + RLENGTH)
            sub(/^[^:]*:[ \t]*/, "", val)
            gsub(/![ \t]*important/, "", val)
            gsub(/var\([^)]*\)/, "", val)
            gsub(/calc\([^)]*\)/, "", val)
            nv = split(val, parts, /[ \t]+/)
            for (i = 1; i <= nv; i++) {
                p = parts[i]
                if (p == "" || p == "0" || p == "50%" || p == "100%") continue
                if (p == "12px" || p == "6px") continue
                if (p ~ /^[0-9]+px$/ && p + 0 >= 100) continue
                emit("radius", line)
                break
            }
        }

        # ── 8. Off-grid spacing ───────────────────────────────────────────
        # §6: "8pt grid. Gaps of 8 · 12 · 16 · 20 · 24 · 32 · 40 · 48 · 56 ·
        # 64." Two off-grid values are sanctioned by the same section and are
        # allowed here: 6px (the shell’s tight gap, and its radius) and 22px
        # (§6 spells buttons as `padding: 0 22px`). Everything else — 3px,
        # 5px, 7px, 9px, 10px — is what a page accumulates when spacing is
        # nudged by eye instead of chosen, and is why two sections never line
        # up. Only px is judged: rem is measured against the root, not the grid.
        s = line
        while (match(s, /(^|[^a-zA-Z0-9])(padding|margin|gap|row-gap|column-gap)[a-z-]*[ \t]*:[^;}"'"'"']*/)) {
            val = substr(s, RSTART, RLENGTH)
            s = substr(s, RSTART + RLENGTH)
            sub(/^[^:]*:[ \t]*/, "", val)
            while (match(val, /-?[0-9]+(\.[0-9]+)?px/)) {
                num = substr(val, RSTART, RLENGTH)
                val = substr(val, RSTART + RLENGTH)
                sub(/px$/, "", num)
                x = num + 0; if (x < 0) x = -x
                if (x == 6 || x == 22) continue
                if (x != int(x) || int(x) % 4 != 0) emit("off-grid", line)
            }
        }

        # ── 11. A serif weight or slope that cannot exist ─────────────────
        # JJannon ships ONE cut. `app.css` registers that single woff2 three
        # times (JJannon 300, JJannon 400, and JJannon UI with corrected
        # metrics), so the family reads as richer in the stylesheet than it is
        # on disk — and CSS font matching stays INSIDE the family for weight
        # and style, falling through to ui-serif/Georgia only for a missing
        # glyph or while the file loads. So `font-weight: 500` on the serif is
        # a request that is silently dropped and renders 400, and `font-style:
        # italic` is a browser-synthesized oblique: a mechanical slant of the
        # roman, not a drawn italic, and it looks it. Neither is loud enough to
        # catch in review, which is the whole reason to count them. The full
        # account is agents/build/typography.md and the @font-face comment in
        # apps/web/src/app.css. A serif hierarchy is set with size, case, color
        # and space — §4 already asks for exactly that.
        #
        # Per-BLOCK, not per-line, because the family and the weight are two
        # declarations and either can come first (`pages/PageToolbar.svelte`
        # writes the weight above the family). Candidates are buffered per
        # brace depth and released when the block closes, so a weight is
        # reported only if the block it sits in actually asked for the serif.
        # The brace count skips full-line comments with the rest of the scan,
        # so a comment holding an unbalanced brace would drift the depth; that
        # costs a missed hit, never a false one, because the family and the
        # candidate must land at the SAME depth to pair.
        #
        # Inheritance is out of scope on purpose: `font-style: italic` on a
        # block that names no family may or may not be sitting in serif prose
        # (`.markdown em`, `.markdown blockquote` in app.css are), and no
        # per-file scan can tell. This rule counts what it can prove.
        o = line; opens = gsub(/\{/, "{", o)
        c = line; closes = gsub(/\}/, "}", c)
        if (opens > 0) { depth += opens; if (depth > MAXD) depth = MAXD }
        if (line ~ /(font-family|fontFamily)[ \t]*:/ && line ~ /var\(--font-serif(-ui)?[,)]/)
            serifFamily[depth] = 1
        if (line ~ /font-weight[ \t]*:[ \t]*(500|600|700|800|900|bold|bolder)/ \
            || line ~ /font-style[ \t]*:[ \t]*(italic|oblique)/) {
            serifN[depth]++
            serifLn[depth, serifN[depth]] = FNR
            serifSrc[depth, serifN[depth]] = line
        }
        if (closes > 0)
            for (k = 0; k < closes; k++) { serif_close(depth); depth--; if (depth < 0) depth = 0 }
    }'
}

# ─── 9. Motion without a reduced-motion answer ─────────────────────────────
# §7: "Under prefers-reduced-motion, keep the information and drop the travel."
# A file that transitions or animates and never mentions the query has no
# answer for a reader who asked the OS for one — and vestibular motion
# sensitivity is not a preference, it is a symptom. Whole-file by nature, so it
# is its own pass: one record per offending FILE, reported at the first line
# that moves, which is the line an author should look at.
scan_motion() {
    while IFS= read -r f; do
        hit=$(awk '
            /design-ok:/ { next }
            /prefers-reduced-motion/ { guarded = 1; exit }
            !moves && /(^|[^a-zA-Z0-9-])(transition|animation)[a-z-]*[ \t]*:/ { moves = FNR; src = $0 }
            END { if (!guarded && moves) { gsub(/\t/, " ", src); sub(/^[ \t]+/, "", src); print moves "\t" src } }
        ' "$f")
        [ -n "$hit" ] || continue
        printf 'no-reduced-motion\t%s\t%s\n' "$f" "$hit"
    done < <(file_list)
}

TMP=$(mktemp) || exit 1
trap 'rm -f "$TMP"' EXIT
{ scan; scan_motion; } | LC_ALL=C sort > "$TMP"

count_of() { awk -F'\t' -v r="$1" '$1 == r { n++ } END { print n + 0 }' "$TMP"; }

# ─── --list ────────────────────────────────────────────────────────────────
if [ "${1:-}" = "--list" ]; then
    want="${2:-}"
    awk -F'\t' -v want="$want" '
        want == "" || $1 == want { printf "%-18s %s:%s\n    %s\n", $1, $2, $3, $4 }
    ' "$TMP"
    exit 0
fi

# ─── --update-baseline ─────────────────────────────────────────────────────
if [ "${1:-}" = "--update-baseline" ]; then
    {
        echo "# Design debt frozen by tools/design-lint.sh. One line per rule: <count> <rule>."
        echo "# The count may only go DOWN. Regenerate with:"
        echo "#     bash tools/design-lint.sh --update-baseline"
        echo "# Raising a number here is how the ratchet stops being one — fix the page instead,"
        echo "# or annotate the single line with \`design-ok: <why>\` if the grammar allows it."
        for r in $RULES; do printf '%s %s\n' "$(count_of "$r")" "$r"; done
    } > "$BASELINE"
    echo "baselined $(printf '%s\n' $RULES | wc -l | tr -d ' ') rule(s) into $BASELINE"
    exit 0
fi

[ -f "$BASELINE" ] || { echo "missing $BASELINE — run: bash tools/design-lint.sh --update-baseline"; exit 1; }

fail=0
improved=""
for rule in $RULES; do
    now=$(count_of "$rule")
    was=$(awk -v r="$rule" '/^#/ { next } $2 == r { print $1 + 0; found = 1 } END { if (!found) print 0 }' "$BASELINE")

    if [ "$now" -gt "$was" ]; then
        echo "ERROR: $rule — $now, baseline $was (+$((now - was)))"
        echo "$(describe "$rule")"
        echo
        awk -F'\t' -v r="$rule" 'BEGIN { max = 40 }
            $1 == r { n++; if (n <= max) printf "  %s:%s\n    %s\n", $2, $3, $4 }
            END { if (n > max) printf "  … and %d more — bash tools/design-lint.sh --list %s\n", n - max, r }
        ' "$TMP"
        echo
        fail=1
    elif [ "$now" -lt "$was" ]; then
        improved="$improved$rule|$was|$now
"
    fi
done

if [ -n "$improved" ]; then
    echo "Design debt went DOWN — tighten the baseline so it cannot come back:"
    printf '%s' "$improved" | awk -F'|' 'NF { printf "  %-18s %s → %s\n", $1, $2, $3 }'
    echo "  bash tools/design-lint.sh --update-baseline"
    echo
fi

if [ "$fail" -ne 0 ]; then
    echo "The page grammar is agents/build/design-grammar.md. A limit it states as a"
    echo "number is not advice; put the count back under its baseline, or annotate the"
    echo "one line the grammar actually permits with \`design-ok: <why>\`."
    exit 1
fi

echo "✓ design_lint: no new violations ($(printf '%s\n' $RULES | wc -l | tr -d ' ') rules, $(wc -l < "$TMP" | tr -d ' ') held at baseline)"
