#!/usr/bin/env bash
# Fail on a NEW query result whose error is discarded.
#
# `.ok()`, `.unwrap_or(0)` and `.unwrap_or_default()` on a `fetch_*` turn a
# broken query into a plausible number, and nothing ever surfaces. This is the
# most-repeated bug in this repo: it is why sleep read "0.0 hours", why every
# date-scoped search returned nothing, why resting heart rate was a hardcoded
# 62.0, and why the box reported zero paired devices on every box forever.
#
# It keeps recurring for a specific reason. In the 2026-08-28 audit, six of the
# fifteen worst live instances sat DIRECTLY BENEATH a comment describing this
# very bug class: the postmortem gets written, the SQL gets fixed, and the
# `.unwrap_or` survives because it still compiles and the test still passes.
# Prose has failed at this five times, so this is a check instead.
#
# A RATCHET, not a wall. There are ~200 of these already; a check that fails on
# all of them on day one gets switched off by the end of the week, which is
# worse than no check. The baseline freezes today's debt and this fails only on
# additions — so the count can go down and never up.
#
# Signatures are (file, offending source line), never line numbers: moving code
# must not look like a new bug, and a genuinely new swallow must not hide behind
# a line that shifted.
#
#   Fix one:      use `?`, or annotate `// absent-ok: <why absence is a real answer>`
#   Re-baseline:  tools/check-swallowed-queries.sh --update  (only to REMOVE entries)

set -uo pipefail
cd "$(dirname "$0")/.."
BASELINE="tools/swallowed-queries.baseline"

scan() {
  while IFS= read -r file; do
    while IFS=: read -r line _; do
      [ -n "$line" ] || continue
      ctx=$(sed -n "$((line > 2 ? line - 2 : 1)),$((line + 2))p" "$file")
      case "$ctx" in *absent-ok:*) continue ;; esac
      code=$(sed -n "${line}p" "$file" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')
      printf '%s\t%s\n' "$file" "$code"
    done < <(
      grep -nE -A6 '\.(fetch_one|fetch_all|fetch_optional|execute)\(' "$file" 2>/dev/null |
        grep -E '^[0-9]+-.*\.(ok\(\)|unwrap_or\(|unwrap_or_default\(\)|unwrap_or_else\()' |
        sed 's/^\([0-9]*\)-.*/\1/'
    )
  done < <(find virtues-core crates applets services -name '*.rs' -not -path '*/target/*' 2>/dev/null)
}

current=$(scan | sort | uniq -c | sed 's/^ *//')

if [ "${1:-}" = "--update" ]; then
  printf '%s\n' "$current" > "$BASELINE"
  echo "baselined $(wc -l < "$BASELINE" | tr -d ' ') signature(s)"
  exit 0
fi

[ -f "$BASELINE" ] || { echo "missing $BASELINE — run with --update"; exit 1; }

# `comm -13`: present now, absent from the baseline. A count that GREW changes
# the "<n> <file> <code>" line, so it shows up here too.
added=$(comm -13 <(sort "$BASELINE") <(printf '%s\n' "$current" | sort))
if [ -n "$added" ]; then
  echo "New swallowed query error(s) — a failed query must not answer with a plausible value:"
  echo
  printf '%s\n' "$added" | sed 's/^[0-9]* /  /'
  echo
  echo "Use \`?\`, or annotate with \`// absent-ok: <why absence is a real answer here>\`."
  exit 1
fi
echo "✓ no new swallowed query errors"
