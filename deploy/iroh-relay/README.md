# Virtues iroh-relay — OVH deploy runbook

Replaces the hand-rolled frp blind relay with **iroh-relay 1.0** on the existing
your relay host (`ssh virtues-relay`, `203.0.113.10`). The relay is blind (moves
QUIC/ciphertext only); access is gated by an atlas active-subscription callout.
Boxes home on it and are reached by EndpointId — LAN-direct → hole-punched →
relayed. This is the only public cert in the system (per-box ACME is gone).

Files here:
- `config.toml` → `/etc/iroh-relay/config.toml`
- `virtues-iroh-relay.service` → `/etc/systemd/system/virtues-iroh-relay.service`
- `relay.env` (create on host, NOT in git) → `/etc/iroh-relay/relay.env`

> **Note — the OVH host is already live** (deployed 2026-07-01). It runs the
> binary under a slightly different unit than the reference here: `iroh-relay.service`
> with `DynamicUser=yes` + a drop-in `iroh-relay.service.d/10-access.conf` that
> sets `EnvironmentFile=/etc/iroh-relay/relay.env`, and the bearer comes from that
> env (not from `config.toml`). These files are the **clean-install reference**
> for a fresh host; on the existing host, edit `/etc/iroh-relay/config.toml` +
> `/etc/iroh-relay/relay.env` in place and `systemctl restart iroh-relay`.

## 0. DNS (Route53)

```
relay.virtues.ch  A  203.0.113.10
```
(v4-only host — no AAAA, matches the existing setup.)

## 1. Build / install the `iroh-relay` binary

On the host (or cross-build and scp). The server needs the `server` feature:
```sh
cargo install iroh-relay --version ^1 --features server --root /usr/local
# → /usr/local/bin/iroh-relay
```
(Or build from a pinned checkout like the box, and scp the static binary to
`/usr/local/bin/iroh-relay`.)

## 2. Service user + config

```sh
sudo useradd --system --no-create-home --shell /usr/sbin/nologin iroh-relay
sudo install -d -m 0750 -o iroh-relay -g iroh-relay /etc/iroh-relay
sudo cp config.toml /etc/iroh-relay/config.toml

# Shared secret for the atlas gate. MUST equal atlas VIRTUES_RELAY_AUTH_SECRET.
SECRET=$(openssl rand -hex 32)
printf 'IROH_RELAY_HTTP_BEARER_TOKEN=%s\n' "$SECRET" | \
  sudo tee /etc/iroh-relay/relay.env >/dev/null
sudo chmod 0640 /etc/iroh-relay/relay.env
sudo chown root:iroh-relay /etc/iroh-relay/relay.env
echo "SET atlas VIRTUES_RELAY_AUTH_SECRET to: $SECRET"
```

## 3. Firewall (ufw)

```sh
sudo ufw allow 80/tcp      # LetsEncrypt + plain relay
sudo ufw allow 443/tcp     # relay over HTTPS/WSS
sudo ufw allow 7842/udp    # QUIC (relay + addr discovery)
# keep 22/tcp
```

## 4. Start

```sh
sudo cp virtues-iroh-relay.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now virtues-iroh-relay
journalctl -u virtues-iroh-relay -f    # watch for LetsEncrypt cert issuance
```

## 5. Wire atlas (the gate's other half)

atlas already reads these (services/virtues-atlas/src/config.rs). Set on the
atlas host/env and redeploy:
```
VIRTUES_RELAY_URL=https://relay.virtues.ch
VIRTUES_RELAY_AUTH_SECRET=<the $SECRET from step 2>
```
- `VIRTUES_RELAY_URL` is handed to boxes at `/relay/config`.
- `VIRTUES_RELAY_AUTH_SECRET` is the bearer atlas requires on `/relay/authorize`
  (constant-time compared). If unset, atlas fails **closed** (denies all).

## 6. Verify end-to-end

1. **Cert**: `curl -sI https://relay.virtues.ch` returns a valid LE cert.
2. **Gate deny**: a POST to atlas `/relay/authorize` with the right bearer but a
   random `X-Iroh-NodeId` returns `403 "false"`; with no/blank secret configured
   atlas returns `503 "false"` (fail-closed).
3. **Box homes on it**: a box with `VIRTUES_RELAY_URL` set (via `/relay/config`)
   binds its endpoint and registers via atlas `/iroh/register`; `virtues doctor`
   / box_status shows `endpoint_up`.
4. **Reach**: a paired device dials the box by EndpointId and loads the app over
   the relay; then confirm it upgrades to a hole-punched direct path when both
   ends allow UDP.

## Notes

- The relay never terminates the box's app TLS — it forwards QUIC; the box's
  EndpointId (mutual-key auth) is the identity, and the app-layer bearer/cookie
  is the authorization keystone on top.
- Only `relay.virtues.ch`'s own LE cert lives here; there is no per-box cert.
- The old frp `virtues-relay` unit/env/ufw rules (9443, etc.) can be removed
  once this is confirmed live.
