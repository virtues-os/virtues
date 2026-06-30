# Blind-relay e2e harness

Stands up the **relay** and a **box** (a lean stand-in for the box's relay
subsystem — no Postgres/atlas) in Docker and proves the core launch path:

> a browser reaches the box **through the relay**, over TLS the **box** terminates
> with its own cert, the box has **no inbound port**, and it **reconnects** after
> a drop.

```
browser (test script) ──TLS──▶ relay :8443 ──[peek SNI, splice ciphertext]──▶
        box (dials OUT to relay :9443) ──terminates TLS──▶ local HTTP "ok"
```

## Run

Requires Docker running.

```bash
cd deploy/e2e
make up      # build + start relay + box
make test    # reach / anti-bypass / liveness scenarios
make down    # tear down
```

`make test` checks:
1. **reach** — `curl` (SNI `box1.virtues.ch`) hits the relay's browser port and
   gets `ok: reached the box through the blind relay` back — i.e. the SNI was
   routed, the ClientHello replayed, TLS terminated on the box, and bytes spliced
   both ways.
2. **anti-bypass** — the `box` service publishes no ports; it's reachable only by
   having dialed out to the relay.
3. **liveness** — restart the box; it re-dials, re-registers, and becomes
   reachable again (the dial-out + jittered-reconnect path).

## Blindness (manual)

The relay never decrypts. To see it for yourself, capture on the relay while a
request flows:

```bash
docker compose exec relay sh -c "apt-get update && apt-get install -y tcpdump && tcpdump -A -i any port 8443" # ciphertext only; SNI is the one cleartext field
```

## What this does NOT cover (by design)

- **Real ACME issuance.** The box here serves a **self-signed** cert (so `curl
  -k`). Wiring real ACME against [Pebble](https://github.com/letsencrypt/pebble)
  (LE's test ACME server) needs (a) virtues-core in the image — it owns
  `acme.rs` — and (b) a seam so the box's rustls ACME client trusts Pebble's test
  CA (rustls/webpki won't read `SSL_CERT_FILE`). That's a follow-up; the ACME
  *logic* (DNS-01 TXT grouping, order/cert polling bounds) is unit-tested in
  `virtues-core/src/acme.rs`.
- **Per-SNI HMAC + revocation bucketing.** The harness uses the shared-bearer
  path so it needs no atlas. To exercise the HMAC path here: set
  `VIRTUES_RELAY_SECRET=<s>` on the relay (remove `ALLOW_INSECURE`/`TOKEN`), and
  set the box's `VIRTUES_RELAY_TOKEN` to
  `hex(HMAC-SHA256(<s>, "box1.virtues.ch:<bucket>"))` for the current bucket
  (`floor(unix_secs/86400)`). The unit tests in `services/virtues-relay/tests/`
  already cover this and the ±1-bucket expiry.

## Files

- `docker-compose.yml` — relay + box + a host-exposed relay port.
- `Dockerfile.relay` / `Dockerfile.box` — built from the repo root.
- `box-harness/` — the lean box stand-in (own cargo workspace; reach + TLS +
  liveness only).
- `run-tests.sh` — the scenarios above.
