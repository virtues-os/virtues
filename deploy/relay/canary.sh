#!/usr/bin/env bash
# Minimal external reachability canary for the relay. Run this from OFF the relay
# host (a laptop, a cron elsewhere, a CI ping) — a canary on the same box can't
# tell you the box fell off the internet.
#
#   ./canary.sh <relay-host-or-ip> [client_port] [control_port]
#   ./canary.sh 15.204.248.171
#
# Checks both listeners are reachable. Exit 0 = healthy, non-zero = down — so it
# composes with anything: `canary.sh … && curl https://hc-ping.com/<uuid>`, a
# systemd timer with OnFailure, or a cron whose failure mail is your alert.
#
# This is deliberately tiny. For zero-maintenance monitoring, point an external
# uptime service (healthchecks.io, UptimeRobot, a Route53 health check) at
# tcp://<relay>:443 instead — same signal, nothing to host.
set -euo pipefail

HOST="${1:?usage: canary.sh <relay-host-or-ip> [client_port] [control_port]}"
CLIENT_PORT="${2:-443}"
CONTROL_PORT="${3:-9443}"
TIMEOUT=5

check() { # port label
  if timeout "$TIMEOUT" bash -c "exec 3<>/dev/tcp/$HOST/$1" 2>/dev/null; then
    echo "ok   $2 ($HOST:$1) reachable"
    exec 3>&- 3<&- 2>/dev/null || true
    return 0
  fi
  echo "FAIL $2 ($HOST:$1) unreachable"
  return 1
}

rc=0
check "$CLIENT_PORT"  "client/browser" || rc=1
check "$CONTROL_PORT" "box control"    || rc=1
[[ $rc -eq 0 ]] && echo "✓ relay healthy" || echo "✗ relay DEGRADED"
exit $rc
