#!/usr/bin/env bash
# Provision a fresh Virtues relay host (idempotent). Debian/Ubuntu with systemd.
#
#   sudo ./provision.sh
#
# Installs sysctls + firewall + the systemd unit, and creates a placeholder env
# file if none exists. It does NOT install the binary — run update.sh for that
# (keeps the "provision host" and "ship binary" steps independent).
#
# Env file (/etc/virtues-relay.env) is NEVER overwritten if it already exists,
# so re-running is safe and won't clobber the secret.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENV_FILE=/etc/virtues-relay.env
UNIT=/etc/systemd/system/virtues-relay.service

if [[ $EUID -ne 0 ]]; then echo "run as root (sudo)"; exit 1; fi

echo "→ sysctls"
install -m 0644 "$HERE/99-relay.conf" /etc/sysctl.d/99-relay.conf
sysctl --system >/dev/null

echo "→ firewall (ufw)"
if command -v ufw >/dev/null; then
  ufw allow 22/tcp   >/dev/null || true   # SSH
  ufw allow 443/tcp  >/dev/null || true   # client/browser (TLS passthrough)
  ufw allow 9443/tcp >/dev/null || true   # box dial-out (control + work)
  ufw --force enable >/dev/null || true
  echo "  ufw: $(ufw status | head -1)"
else
  echo "  ufw not installed — skipping (ensure 22/443/9443 are open another way)"
fi

echo "→ systemd unit"
install -m 0644 "$HERE/virtues-relay.service" "$UNIT"
systemctl daemon-reload

if [[ ! -f "$ENV_FILE" ]]; then
  echo "→ env file (placeholder — EDIT $ENV_FILE and set VIRTUES_RELAY_SECRET)"
  install -m 0600 "$HERE/virtues-relay.env.example" "$ENV_FILE"
  echo "  !! $ENV_FILE has REPLACE_ME — set the real secret before starting."
else
  echo "→ env file exists at $ENV_FILE (left untouched)"
fi

systemctl enable virtues-relay >/dev/null 2>&1 || true
echo "✓ provisioned. Next: sudo $HERE/update.sh <binary-path-or-url>, then"
echo "  sudo systemctl start virtues-relay && journalctl -u virtues-relay -f"
