#!/usr/bin/env bash
# Install/upgrade the relay binary with an automatic rollback (idempotent).
#
#   sudo ./update.sh <path-or-url-to-virtues-relay>
#
# e.g.  sudo ./update.sh /tmp/virtues-relay
#       sudo ./update.sh https://…/virtues-relay-x86_64-linux
#
# Backs up the current binary to .bak, installs the new one, restarts, and
# verifies the service came up healthy (active + listening on both ports). On
# failure it restores .bak and restarts, so a bad binary never leaves the relay
# down. The static musl binary from CI (release-relay.yml) is the intended input.
set -euo pipefail

SRC="${1:?usage: update.sh <path-or-url-to-virtues-relay>}"
DEST=/usr/local/bin/virtues-relay
CLIENT_PORT="${VIRTUES_RELAY_CLIENT_PORT:-443}"
CONTROL_PORT="${VIRTUES_RELAY_CONTROL_PORT:-9443}"

if [[ $EUID -ne 0 ]]; then echo "run as root (sudo)"; exit 1; fi

TMP="$(mktemp)"
trap 'rm -f "$TMP"' EXIT
if [[ "$SRC" =~ ^https?:// ]]; then
  echo "→ downloading $SRC"
  curl -fsSL "$SRC" -o "$TMP"
else
  cp "$SRC" "$TMP"
fi
chmod 0755 "$TMP"

# Sanity: it must be an executable that at least runs (help/version tolerated).
if ! "$TMP" --help >/dev/null 2>&1 && ! file "$TMP" | grep -q ELF; then
  echo "!! $SRC does not look like a runnable relay binary"; exit 1
fi

if [[ -f "$DEST" ]]; then cp -a "$DEST" "$DEST.bak"; echo "→ backed up current → $DEST.bak"; fi
install -m 0755 "$TMP" "$DEST"
echo "→ installed new binary"

echo "→ restarting"
systemctl restart virtues-relay
sleep 3

healthy() {
  systemctl is-active --quiet virtues-relay || return 1
  ss -ltn "( sport = :$CLIENT_PORT )"  | grep -q ":$CLIENT_PORT"  || return 1
  ss -ltn "( sport = :$CONTROL_PORT )" | grep -q ":$CONTROL_PORT" || return 1
}

if healthy; then
  echo "✓ relay healthy (active + listening on :$CLIENT_PORT and :$CONTROL_PORT)"
else
  echo "!! relay unhealthy after update — rolling back"
  if [[ -f "$DEST.bak" ]]; then
    install -m 0755 "$DEST.bak" "$DEST"
    systemctl restart virtues-relay
    sleep 3
    healthy && echo "✓ rolled back to previous binary; relay healthy" \
            || echo "!! rollback still unhealthy — check: journalctl -u virtues-relay -e"
  else
    echo "!! no .bak to roll back to — check: journalctl -u virtues-relay -e"
  fi
  exit 1
fi
