#!/usr/bin/env bash
# Architectural lint guards. Fail early on patterns that cause long-term cruft.
#
# Run from the repo root:   bash scripts/arch_lint.sh
# Returns non-zero on the first violation.
#
# Each lint codifies a charter invariant from ACTIONS.md. The goal is that a
# year from now, none of these patterns sneak back in via code review fatigue.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail=0

report() {
    local title="$1"
    local detail="$2"
    local violations="$3"
    echo "ERROR: $title"
    echo "$detail"
    echo
    echo "$violations"
    echo
    fail=1
}

# ─── Lint 1: No `match … source_id` in core/src/api/ ───────────────────────
# Catches `match source_id { "google" => ..., "plaid" => ... }` provider-
# specific branching. Quirks belong in proxy routes or sync binaries.
violations=$(grep -rEn 'match[[:space:]]+[a-zA-Z_]*source_id' core/src/api/ --include='*.rs' || true)
if [ -n "$violations" ]; then
    report \
        "provider-specific 'match … source_id' detected in core/src/api/" \
        "Provider quirks belong in apps/oauth-proxy routes (for via_proxy) or in source-tagged sync binaries. Core stays catalog-driven." \
        "$violations"
fi

# ─── Lint 2: No provider names in auth helpers ─────────────────────────────
# Auth helpers must be provider-agnostic. They branch on auth.kind, not on
# specific provider ids.
if [ -d "crates/virtues-helpers/src/auth" ]; then
    pattern='(google|plaid|notion|spotify|github|strava|stripe|microsoft|slack|discord|linear|linkedin)'
    violations=$(grep -rEni "$pattern" crates/virtues-helpers/src/auth/ --include='*.rs' \
        | grep -vE '^\S+:\s*//' \
        || true)
    if [ -n "$violations" ]; then
        report \
            "provider name detected in crates/virtues-helpers/src/auth/" \
            "Auth helpers must be provider-agnostic — branch on auth.kind, not on specific provider ids." \
            "$violations"
    fi
fi

# ─── Lint 3: No HMAC primitives outside crates/virtues-helpers/src/crypto/ ─
# All HMAC/AES/OAuth-state primitives flow through the crypto submodule. CI
# rejects any `Hmac::<Sha256>` or `Hmac<Sha256>` usage elsewhere.
# (Doc comments — `//`, `///`, `//!` — are excluded; they reference the type.)
violations=$(grep -rEn 'Hmac::<Sha256>|Hmac<Sha256>|use hmac::' \
    --include='*.rs' \
    crates/ core/ actions/ \
    2>/dev/null \
    | grep -v 'crates/virtues-helpers/src/crypto/' \
    | grep -vE ':[[:space:]]*(///|//!|//)' \
    || true)
if [ -n "$violations" ]; then
    report \
        "HMAC primitive used outside crates/virtues-helpers/src/crypto/" \
        "All HMAC/AES/state-token crypto must live in the crypto submodule. Wrap it via virtues_helpers::auth::* or virtues_helpers::crypto::*." \
        "$violations"
fi

# ─── Lint 4: No println!/print! in actions/src/bin/ ────────────────────────
# Action subprocesses write structured JSON to stdout. A stray println! breaks
# the runner's parser. Use tracing to stderr instead.
violations=$(grep -rEn '\bprintln!|\bprint!' actions/src/bin/ --include='*.rs' || true)
if [ -n "$violations" ]; then
    report \
        "println!/print! in actions/src/bin/ corrupts the stdout JSON contract" \
        "Action subprocesses write {result, config} JSON to stdout — anything else makes the runner fail to parse the output. Use tracing::info/warn/error to stderr." \
        "$violations"
fi

# ─── Lint 5: No application code reads of legacy action_credentials table ─
# The credentials Vault is the canonical store (migration 055). The legacy
# action_credentials table is dead code awaiting drop in migration 056. Any
# new application-code SELECT/UPDATE/INSERT against it indicates a regression.
#
# Exclusions (legitimate historical references):
#   - core/migrations/*.sql        — append-only schema history
#   - core/src/credentials/migrate.rs — the one-time 055 re-encryption hook
violations=$(grep -rEn 'FROM[[:space:]]+action_credentials|UPDATE[[:space:]]+action_credentials|INTO[[:space:]]+action_credentials' \
    --include='*.rs' --include='*.sql' \
    core/ actions/ crates/ \
    2>/dev/null \
    | grep -v 'core/migrations/' \
    | grep -v 'core/src/credentials/migrate.rs' \
    || true)
if [ -n "$violations" ]; then
    report \
        "read or write against legacy action_credentials table" \
        "The credentials Vault (migration 055) is the canonical store. action_credentials is awaiting drop in migration 056 — no new code should reference it." \
        "$violations"
fi

if [ "$fail" -eq 0 ]; then
    echo "arch_lint: OK (5 invariants enforced)"
else
    exit 1
fi
