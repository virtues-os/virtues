# Virtues relay — deploy & operations

The blind L4 SNI-passthrough relay. Boxes dial **out** to it (control + work
connections); browsers hit it on `:443` and it splices ciphertext to the right
box by cleartext SNI. It **never terminates TLS** (the box holds its own cert),
holds **no disk state** (RAM-only registry), and holds **no per-box secrets** —
just the shared HMAC secret it uses to verify registration tokens atlas mints.

Design: [`docs/relay-control-plane.md`](../../docs/relay-control-plane.md).
Privacy model: [`docs/privacy-model.md`](../../docs/privacy-model.md).

## Files here

| File | Role |
|---|---|
| `virtues-relay.service` | systemd unit — DynamicUser, `CAP_NET_BIND_SERVICE` only, `LimitNOFILE=1048576`, `MemoryMax=1G`, full sandbox hardening |
| `99-relay.conf` | sysctls — SYN-cookies + backlog/port-range tuning for a busy public TCP front |
| `virtues-relay.env.example` | env template (`/etc/virtues-relay.env`, chmod 600) |
| `provision.sh` | idempotent host setup (sysctls + ufw + unit + placeholder env) |
| `update.sh` | install/upgrade the binary with `.bak` auto-rollback + health check |

The binary is built in CI as a **static x86_64 musl** asset by
[`.github/workflows/release-relay.yml`](../../.github/workflows/release-relay.yml)
(trigger: a `relay-v*` tag, or manual dispatch → workflow artifact). One file, no
runtime deps.

## Stand up a fresh relay host (also the DR path)

On a clean Debian/Ubuntu host (e.g. an OVH VPS):

```sh
git clone … && cd virtues/deploy/relay      # or scp just this dir
sudo ./provision.sh                          # sysctls + ufw(22/443/9443) + unit
sudoedit /etc/virtues-relay.env              # set VIRTUES_RELAY_SECRET (matches atlas)
sudo ./update.sh https://github.com/virtues-os/virtues/releases/download/relay-vX/virtues-relay-x86_64-linux
sudo systemctl start virtues-relay
journalctl -u virtues-relay -f
```

`VIRTUES_RELAY_SECRET` **must equal** atlas's `VIRTUES_RELAY_SECRET` (atlas mints,
relay verifies). Then point DNS (`*.virtues.ch A → this host`) at the new IP.
Low DNS TTL makes the repoint fast — the core of the single-region DR runbook.

## Update the binary

```sh
sudo ./update.sh /path/to/virtues-relay-x86_64-linux   # or a release URL
```

Backs up the running binary to `/usr/local/bin/virtues-relay.bak`, installs, restarts,
and verifies the service is active + listening on `:443` and `:9443`. If it isn't,
it **restores `.bak` and restarts** — a bad binary never leaves the relay down.
Boxes reconnect automatically (jittered backoff) across the brief restart.

## Rotate the relay secret (zero-downtime)

The relay accepts tokens minted under the current **or** previous secret, so:

1. Set `VIRTUES_RELAY_SECRET_PREV` = the current secret, `VIRTUES_RELAY_SECRET` =
   the new one, in **both** `/etc/virtues-relay.env` and atlas's env. Restart both.
2. Wait ≥ one box token-refresh interval (~12h) for the fleet to re-fetch tokens
   minted under the new secret.
3. Clear `VIRTUES_RELAY_SECRET_PREV` (unset/blank) and restart the relay.

> Roadmap (#6′): replace this shared symmetric secret with **asymmetric signing**
> — atlas signs tokens with a private key, the relay verifies with a **public**
> key. The secret then lives only in atlas; the relay holds nothing confidential,
> so a relay compromise leaks nothing and there is no secret to keep in sync here.

## Operations

- Logs: `journalctl -u virtues-relay -f`
- Restart: `sudo systemctl restart virtues-relay`
- Listeners: `ss -ltn '( sport = :443 or sport = :9443 )'`
- Reachability smoke test (from anywhere): `curl -sv https://<fake>.virtues.ch/ --resolve <fake>.virtues.ch:443:<relay-ip>` → a TLS error/`exit 35` means the relay was reached + peeked (no box for that SNI), which is the expected "alive" signal.

## Monitoring

The relay is the critical path for *remote* access, so knowing it's up matters.
Keep it minimal — this is an uptime check, not a metrics platform:

- **Zero-maintenance:** point an external uptime service (healthchecks.io,
  UptimeRobot, or a Route 53 health check) at `tcp://<relay>:443`. Nothing to host.
- **Self-hosted:** `canary.sh <relay-ip>` from **off** the relay host (exit 0 =
  healthy). Compose it with anything — e.g. cron: `*/2 * * * * canary.sh <ip> && curl -fsS https://hc-ping.com/<uuid>` (a missed ping alerts), or a systemd timer with `OnFailure=`.

Deliberately not here (deferred, post-launch): privacy-preserving aggregate
throughput metrics + box→owner remote-access-status notifications.
