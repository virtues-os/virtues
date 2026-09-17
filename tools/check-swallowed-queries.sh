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
# ONE awk PASS, NOT `grep -A6 | grep | sed`. BSD grep bus-errors partway through
# this tree, and because the script sets `pipefail` but not `-e`, the crash
# stopped nothing: the walk simply ended after virtues-core, never read applets/
# or services/, and printed "no new swallowed query errors". A check that says
# nothing is wrong because it died is worse than no check. Running --update in
# that state wrote a baseline with 13 signatures silently dropped — a ratchet
# loosened by a segfault. awk does the same window in one portable pass and
# cannot crash on it.
#
# TWO SHAPES ARE SKIPPED, because they are not what this check is about and were
# 31% of everything it had ever found (46 of 139 baselined instances):
#
#   row.try_get("x").ok()    reads a COLUMN off a row already in hand. The
#                            query's own error was handled where the row was
#                            fetched, usually a `?` a few lines up. An absent or
#                            NULL column is a real answer about the data.
#   tx.rollback().await.ok() discards the error from a ROLLBACK, unactionable by
#                            construction: the transaction is already being
#                            abandoned and there is no second thing to try.
#
# Both were caught only for sitting inside the six-line window. Every one had to
# be read and hand-annotated to be dismissed, and the next would too.
#
#   Fix one:      use `?`, or annotate `// absent-ok: <why absence is a real answer>`
#   Re-baseline:  tools/check-swallowed-queries.sh --update  (only to REMOVE entries)

set -uo pipefail
cd "$(dirname "$0")/.."
BASELINE="tools/swallowed-queries.baseline"

scan() {
  # ONE awk pass over every file, rather than a shell loop spawning
  # grep+sed+sed per file inside nested process substitutions.
  #
  # That shape was not merely slow, it was WRONG, in two compounding ways:
  #
  #   - BSD grep bus-errors partway through this tree. `pipefail` is set but
  #     `-e` is not, so the crash stopped nothing and the walk just ended early.
  #   - The counts were not even reproducible. Scanning virtues-core alone
  #     found 62 signatures and applets alone found 5, but the two together
  #     found 65 — the nested `while read < <(...)` loops were losing rows.
  #
  # Both failures are silent and both report success, and `--update` run in
  # that state writes a baseline with the missing rows deleted: a ratchet
  # loosened by a crash. One awk process, no per-file subshells, no pipeline to
  # half-die.
  awk '
    function flush(   i, j, ctx, code, ok) {
      for (i in hit) {
        ok = 0
        for (j = (i > 2 ? i - 2 : 1); j <= i + 2; j++)
          if ((j in L) && L[j] ~ /absent-ok:/) ok = 1
        if (!ok) {
          code = L[i]
          sub(/^[ \t]+/, "", code); sub(/[ \t]+$/, "", code)
          print F "\t" code
        }
      }
      delete hit; delete L; anchor = 0
    }
    # A new file starts: settle the previous one while F and L still describe it.
    FNR == 1 && NR > 1 { flush() }
    { L[FNR] = $0; F = FILENAME }
    # The anchor line itself is never a hit: `grep -A6` printed it as "NNN:"
    # and the old filter accepted only context lines ("NNN-").
    /\.(fetch_one|fetch_all|fetch_optional|execute)\(/ { anchor = FNR; next }
    anchor && FNR > anchor && FNR <= anchor + 6 \
      && /\.(ok\(\)|unwrap_or\(|unwrap_or_default\(\)|unwrap_or_else\()/ \
      && !/try_get|rollback/ { hit[FNR] = 1 }
    END { flush() }
  ' $(find virtues-core crates applets services -name '*.rs' -not -path '*/target/*' 2>/dev/null)
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
