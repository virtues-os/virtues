#!/usr/bin/env bash
# Architectural lint guards. Fail early on patterns that cause long-term cruft.
#
# Run from the repo root:   bash tools/arch-lint.sh
# Returns non-zero on the first violation.
#
# Each lint codifies a charter invariant from docs/architecture.md. The goal is that a
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

# ─── Lint 1: No `match … source_id` in virtues-core/src/api/ ───────────────────────
# Catches `match source_id { "google" => ..., "plaid" => ... }` provider-
# specific branching. Quirks belong in proxy routes or sync binaries.
violations=$(grep -rEn 'match[[:space:]]+[a-zA-Z_]*source_id' virtues-core/src/api/ --include='*.rs' || true)
if [ -n "$violations" ]; then
    report \
        "provider-specific 'match … source_id' detected in virtues-core/src/api/" \
        "Provider quirks belong in services/virtues-api oauth routes (for via_proxy) or in source-tagged sync binaries. Core stays catalog-driven." \
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
    crates/ virtues-core/ actions/ \
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
#   - virtues-core/migrations/*.sql        — append-only schema history
#   - virtues-core/src/credentials/migrate.rs — the one-time 055 re-encryption hook
violations=$(grep -rEn 'FROM[[:space:]]+action_credentials|UPDATE[[:space:]]+action_credentials|INTO[[:space:]]+action_credentials' \
    --include='*.rs' --include='*.sql' \
    virtues-core/ actions/ crates/ \
    2>/dev/null \
    | grep -v 'virtues-core/migrations/' \
    | grep -v 'virtues-core/src/credentials/migrate.rs' \
    || true)
if [ -n "$violations" ]; then
    report \
        "read or write against legacy action_credentials table" \
        "The credentials Vault (migration 055) is the canonical store. action_credentials is awaiting drop in migration 056 — no new code should reference it." \
        "$violations"
fi

# ─── Lint 6: No AI provider SDKs in client code ────────────────────────────
# AI provider keys live only on the server (virtues-api → Vercel AI Gateway).
# Client code uses `@ai-sdk/*` (Vercel AI SDK) to talk to OUR backend, not
# provider SDKs directly. Forbid direct-provider imports in web + iOS.
violations=$(grep -rEn '@anthropic-ai/sdk|from[[:space:]]+["'\'']openai["'\'']|@google/generative-ai|@google-ai/generativelanguage|@aws-sdk/client-bedrock-runtime' \
    apps/web/src/ apps/ios/Virtues/ 2>/dev/null \
    | grep -vE ':[[:space:]]*(///|//!|//|\*)' \
    || true)
if [ -n "$violations" ]; then
    report \
        "AI provider SDK imported in client code" \
        "AI provider keys live only on the server (virtues-api → Vercel AI Gateway). Client code should use @ai-sdk/* (Vercel AI SDK) which talks to OUR backend, never a provider SDK directly." \
        "$violations"
fi

# ─── Lint 7: No AI provider API keys in client code ────────────────────────
# Catches both raw key formats and env-var refs that imply client-side keys.
# virtues-api holds the AI_GATEWAY_API_KEY; nothing in apps/web or apps/ios
# should reference these names. Server-side code (virtues-core/, services/virtues-api/) is
# excluded from this lint.
violations=$(grep -rEn 'sk-ant-[A-Za-z0-9]|sk-proj-[A-Za-z0-9]|ANTHROPIC_API_KEY|OPENAI_API_KEY|GEMINI_API_KEY|GOOGLE_AI_API_KEY|AI_GATEWAY_API_KEY|XAI_API_KEY' \
    apps/web/src/ apps/ios/Virtues/ 2>/dev/null \
    | grep -vE ':[[:space:]]*(///|//!|//|\*)' \
    || true)
if [ -n "$violations" ]; then
    report \
        "AI provider API key referenced in client code" \
        "Provider keys (incl. AI_GATEWAY_API_KEY) live only on the server. Client code must never see them — calls go through our backend." \
        "$violations"
fi

# ─── Lint 8: virtues-api schema must be counter-only ─────────────────────────
# virtues-api's privacy claim is no events table by construction. Forbids
# event/usage_log/request_log/audit_log table definitions in any virtues-api
# SQL. Inactive today (virtues-api has no SQL files yet — RAM only); activates
# automatically when WS-6b lands virtues-api migrations.
violations=$(find services/virtues-api -name '*.sql' -print0 2>/dev/null \
    | xargs -0 grep -rEni 'CREATE[[:space:]]+TABLE[[:space:]]+(IF[[:space:]]+NOT[[:space:]]+EXISTS[[:space:]]+)?(public\.)?(events|usage_log|usage_logs|request_log|request_logs|audit_log|audit_logs|activity_log|activity_logs|event_log|event_logs)[[:space:](]' \
    2>/dev/null \
    || true)
if [ -n "$violations" ]; then
    report \
        "events/usage_log/request_log/audit_log table in virtues-api schema" \
        "virtues-api is counter-only by construction: per-token mutable integers, never an events ledger. A subpoena must yield 'token X has budget Y' — never a list of requests. Use counters in counters tables." \
        "$violations"
fi

# ─── Lint 9: No stable device-ID as bearer token ───────────────────────────
# The doc forbids using a stable device identifier (UUID, serial, etc.) as a
# bearer token — that's a tracking identifier, not an entitlement proof.
# WS-2 replaces apps/ios/Virtues/Models/DeviceConfiguration.swift:64 with
# per-pair credentials from QR pairing; the file is excluded here until then.
violations=$(grep -rEn 'deviceToken[[:space:]]*[:{=][[:space:]]*[^/]*\bdeviceId\b|\bdeviceToken\b[[:space:]]+\{[[:space:]]*deviceId' \
    apps/ios/Virtues/ --include='*.swift' 2>/dev/null \
    | grep -v 'apps/ios/Virtues/Models/DeviceConfiguration.swift' \
    | grep -vE ':[[:space:]]*(///|//!|//|\*)' \
    || true)
if [ -n "$violations" ]; then
    report \
        "stable device-ID used as bearer token" \
        "A stable device identifier (UUID, serial, etc.) used as a bearer is a tracking identifier — explicitly forbidden by the privacy charter. Use per-pair credentials provisioned at QR pairing (see WS-2)." \
        "$violations"
fi

# ─── Lint 10: The wall — no shared identity column across the two services ─
# The whole privacy guarantee is that Atlas (billing) and virtues-api (usage)
# share NO field, so a subpoena of both still can't join customer↔usage.
# This enforces it concretely: customer-identity columns must never appear in
# virtues-api's schema, and bearer/usage columns must never appear in Atlas's.
# See docs/Virtues-API.md and docs/entitlement.md.

# Customer-identity columns forbidden in virtues-api migrations.
if [ -d "services/virtues-api/migrations" ]; then
    violations=$(grep -rEni \
        'stripe_customer_id|billing_token|customer_id|activation_handle|payment_token|\bemail\b' \
        services/virtues-api/migrations/ --include='*.sql' \
        | grep -vE ':[[:space:]]*--' \
        || true)
    if [ -n "$violations" ]; then
        report \
            "customer-identity column found in virtues-api schema" \
            "virtues-api must never hold a customer identifier — that would create a join key to Atlas and collapse the wall. Keep identity in Atlas only." \
            "$violations"
    fi
fi

# Bearer/usage columns forbidden in Atlas migrations.
if [ -d "services/virtues-atlas/migrations" ]; then
    violations=$(grep -rEni \
        'bearer_hash|voucher_code|\bbearer\b' \
        services/virtues-atlas/migrations/ --include='*.sql' \
        | grep -vE ':[[:space:]]*--' \
        || true)
    if [ -n "$violations" ]; then
        report \
            "bearer/usage column found in Atlas schema" \
            "Atlas must never hold a usage bearer — that would create a join key to virtues-api and collapse the wall. Atlas mints vouchers and forgets them; it stores no bearer." \
            "$violations"
    fi
fi

if [ "$fail" -eq 0 ]; then
    echo "arch_lint: OK (10 invariants enforced)"
else
    exit 1
fi
