#!/usr/bin/env bash
# Scenarios for the blind-relay e2e harness. Run after `make up`.
# Proves: a browser reaches the box THROUGH the relay over box-terminated TLS,
# the box has no inbound port, and it reconnects after a restart.
set -euo pipefail

SNI="box1.virtues.ch"
PORT="18443"          # host → relay browser port (see docker-compose.yml)
URL="https://${SNI}:${PORT}/"
# --resolve pins the SNI hostname to localhost; -k accepts the box's self-signed
# cert (a real box would present a browser-trusted ACME cert — see README).
CURL=(curl -sk --resolve "${SNI}:${PORT}:127.0.0.1" "${URL}")

reach() { "${CURL[@]}" 2>/dev/null | grep -q "reached the box"; }

echo "[1/3] reach: browser → relay → box (TLS terminated on the box cert)"
for i in $(seq 1 30); do
  if reach; then echo "      ✓ reached the box through the relay"; break; fi
  sleep 1
  if [ "$i" = 30 ]; then echo "      ✗ never reached the box"; docker compose logs --tail=40; exit 1; fi
done

echo "[2/3] anti-bypass: the box exposes no inbound port"
if docker compose port box 8443 >/dev/null 2>&1; then
  echo "      ✗ box published a port (it should only dial out)"; exit 1
else
  echo "      ✓ box has no published port — reachable only via the relay"
fi

echo "[3/3] liveness: restart the box, confirm it reconnects + is reachable again"
docker compose restart box >/dev/null
for i in $(seq 1 60); do
  if reach; then echo "      ✓ box reconnected and is reachable again"; exit 0; fi
  sleep 1
done
echo "      ✗ box did not recover after restart"; docker compose logs --tail=40 box; exit 1
