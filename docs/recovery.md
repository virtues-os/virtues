# Recovery & Operator Guide

> When something on the box breaks, this is where to look first. Each
> failure mode below has a concrete recipe — no investigation required.
>
> If your problem isn't here, paste the output of
> `sudo -u virtues virtues status --json` into a support channel; it
> contains everything we need to triage without asking follow-up
> questions.

---

## Triage cheatsheet

| Symptom | Section |
|---|---|
| Browser can't reach the box from another device | [Reaching the web UI](#reaching-the-web-ui) |
| Lost browser session, no other paired device | [Lost session / locked out](#lost-session--locked-out) |
| Revoked your only paired device by mistake | [Lost session / locked out](#lost-session--locked-out) |
| Forgot which BYO AI key you saved | [Reset BYO key](#reset-byo-key) |
| `systemctl status virtues` shows the service failing | [Service won't start](#service-wont-start) |
| Postgres won't start | [Postgres won't start](#postgres-wont-start) |
| `curl -sSL https://virtues.com/sh \| sudo sh` failed partway through | [`tools/bootstrap.sh` failed mid-run](#toolsbootstrapsh-failed-mid-run) |
| Want to copy your box to new hardware | [Migrating to new hardware](#migrating-to-new-hardware) |
| Want to roll back after a bad upgrade | [Rolling back a `virtues upgrade`](#rolling-back-a-virtues-upgrade) |
| Chat returns 402 / `insufficient_budget` | [402 on chat](#402-on-chat) |
| Connecting Google / Notion fails from outside home | [Source-connect requires LAN](#source-connect-requires-lan) |
| Want to disable / verify the diagnostic beacon | [Diagnostic beacons](#diagnostic-beacons) |

---

## Reaching the web UI

Virtues serves the web UI over **plain HTTP on port 8000**. There is no TLS
on the box — see [networking.md](networking.md) for the rationale.

On the box itself, the web UI is always reachable from a browser at:

```
http://localhost:8000
```

Loopback is a Secure Context per W3C spec, so all modern browsers treat it
as if it were HTTPS (cookies, Service Workers, WebAuthn, no warnings).

Since the Virtues client daemon shipped (v0.2), other devices pair via
WireGuard and reach the box at `http://localhost:8000` on their own machine
through a local proxy — the WireGuard handshake is the trust pin, so the
remote browser also lands in a loopback Secure Context.

From other devices on the LAN *without* the daemon, you can still reach the
box directly at `http://<box-ip>:8000`, but the box's pair-only auth model
applies — you'll see the pairing prompt unless you have a session.

---

## Lost session / locked out

You closed the browser, the cookie expired, or you accidentally clicked
"Revoke" on the device you were sitting at. There is no email-magic-link
recovery — pair-only auth means the recovery path is **physical access to
the box**.

**From the box itself (SSH, console, or attached keyboard):**

```bash
sudo -u virtues virtues link
```

Print the URL it returns, open on a browser, follow the CA-trust recipe
above if it's a fresh client, land logged in. Run as often as you need —
each `virtues link` mints a fresh one-time URL.

**If you revoked your only paired device and the Devices page refused
the delete:** the refusal is a guard against accidental lockout. The
device wasn't actually deleted. Pair a second device first (run
`virtues link` from the box), then revoke the original from the new
session.

---

## Reset BYO key

If you forgot which provider key you saved, or want to rotate to a new
one, you don't need to recover the old key — you just replace it.

1. Open `Settings → AI Provider Key`
2. Click "Remove" (sudo gate fires — approve at the box CLI with
   `sudo -u virtues virtues sudo`)
3. Paste the new key with the right provider
4. Click "Save key" (sudo gate again)

If you can't reach the web UI at all and need to wipe the BYO credential
from the CLI:

```bash
sudo -u virtues psql -d virtues -c \
  "UPDATE credentials SET status='revoked', secret_lookup_hash=NULL \
   WHERE source_id='__byo_ai_key__' AND status='active';"
```

Chat will fall back to the Virtues wallet on the next call.

---

## Service won't start

`sudo systemctl status virtues` shows `failed` or `activating
(auto-restart)` in a loop. Check the journal first:

```bash
sudo journalctl -u virtues -n 100 --no-pager
```

Common patterns:

- **"waiting for postgres to accept connections…"** — this is **normal**
  on first boot. Postgres takes 10–30s to finish WAL recovery; the
  daemon waits. If you see this line for >2 minutes, see [Postgres
  won't start](#postgres-wont-start).
- **"VIRTUES_ENCRYPTION_KEY not set"** — `/etc/virtues/env` is missing
  or the key line is empty. The installer writes this; if you restored
  from a backup tarball, the key is included. If neither, the box is
  uninitialized — re-run `curl -sSL https://virtues.com/sh | sudo sh`.
- **"Failed to bind 0.0.0.0:8000"** — something else is using port
  8000. Find it with `sudo ss -lntp '( sport = :8000 )'`.
- **Out of memory (OOM) kill** — `dmesg | grep -i oom` will confirm.
  Check `free -h`; Virtues needs ~6 GB RAM resident for the embedding +
  reranker models. Add swap or trim cohabitating workloads.

After fixing the cause:

```bash
sudo systemctl restart virtues
sudo journalctl -u virtues -f
```

---

## Postgres won't start

```bash
sudo systemctl status postgresql
sudo journalctl -u postgresql -n 50 --no-pager
```

Common causes:

- **Disk full.** `df -h /var/lib/postgresql`. Postgres won't accept
  writes (or even start cleanly) below ~10 MB free. Clear space,
  then restart.
- **Permission damage.** `/var/lib/postgresql/18/main/PG_VERSION` should
  be owned by `postgres:postgres`. Wrong perms after a restore — fix
  with `sudo chown -R postgres:postgres /var/lib/postgresql`.
- **WAL corruption** (rare; usually a hardware issue). The journal
  will say `invalid checkpoint record` or `record with incorrect prev-
  link`. If you have a recent `virtues backup` tarball, the fastest
  recovery is to wipe and restore. If not, contact Postgres support;
  do not run `pg_resetwal` without understanding what you'll lose.
- **Version mismatch.** If you copied the data directory from another
  box, Postgres refuses to open a cluster from a different major
  version. Use `virtues backup` / `virtues restore` instead of raw
  data-dir copies.

---

## `tools/bootstrap.sh` failed mid-run

The installer is **idempotent** — re-running picks up where it left off
without breaking what's already installed. If a previous run failed
partway through:

```bash
curl -sSL https://virtues.com/sh | sudo sh
```

Common reasons for an install to fail and recover on retry:

- **`apt update` / `dnf check-update` failed** because the repo
  mirror was unreachable. Retry on a different network or wait a
  minute.
- **`sudo` timed out** waiting for your password. Run with `sudo -v`
  first so a fresh credential is cached, then re-run the curl.
- **`postgresql-18` not available in your distro.** v1 supports Debian
  13+, Ubuntu 24.04 LTS+, Fedora 40+. The installer adds the
  [PGDG repo](https://www.postgresql.org/download/linux/)
  automatically for Ubuntu 24.04/25.04 (where PG18 isn't in the
  default repos). For older distros, upgrade first.

If the installer has a real bug, the install beacon at the end posts the
failed step name to atlas (unless `VIRTUES_DIAG=off`). That tells us
where it broke without a support ticket.

---

## How do I back up my data?

```bash
sudo -u virtues virtues backup
```

Writes one encrypted archive to
`/var/lib/virtues/backups/virtues-<utc-iso>.tar.gz.age`. Contents:

- Full Postgres dump (chat, sources, devices, credentials, day pages,
  events, everything)
- The data lake (raw stream archives, drive files) — from wherever
  `STORAGE_PATH` actually points, not a fixed path
- The env file (`VIRTUES_ENCRYPTION_KEY` — required to decrypt
  credentials in the DB)
- `manifest.json` (version, schema migration, sha256 of every member)

### The recovery key — read this once, properly

The **first** `virtues backup` on a box mints an age keypair, prints the
secret half, and stores only the **public** half at
`/var/lib/virtues/backup-recipient`.

That means the box **cannot decrypt its own backups**, deliberately. A
stolen box gives an attacker an encryption key and nothing to decrypt; a
stolen backup drive gives them ciphertext and no key.

It also means **the secret is shown exactly once and cannot be recovered.**
There is nowhere on the box it could have been kept that an attacker with
the box could not also read. Put it in a password manager, or print it and
file it. Without it, every archive this box has ever written is
permanently unreadable.

Archives are standard [age](https://age-encryption.org) files, so they can
also be opened with the `age` CLI on any machine — a backup that only
Virtues can read would be a poor backup.

To customize the output path:

```bash
sudo -u virtues virtues backup --output /mnt/external/virtues.tar.gz.age
```

---

## How do I restore from a backup?

```bash
sudo systemctl stop virtues
sudo virtues restore --key-file /path/to/recovery-key /path/to/virtues-...tar.gz.age
sudo systemctl start virtues
```

`--key-file` holds the recovery key printed at first backup. Archives
written before encryption landed still restore without it — the format is
sniffed, not assumed from the filename.

`virtues restore` enforces three checks before touching anything:

1. The service is stopped (`--force` to override)
2. The backup's schema version is not newer than this binary's (never
   bypassable — upgrade the binary first if needed)
3. Every artifact's sha256 matches the manifest (never bypassable)

It then drops + recreates the Postgres database, restores the data lake,
writes `/etc/virtues/env`, and prints the next-step command.

---

## Migrating to new hardware

The full migration is just backup + install + restore:

**On the old box:**

```bash
sudo -u virtues virtues backup --output /tmp/migration.tar.gz
scp /tmp/migration.tar.gz me@new-box:/tmp/
```

**On the new box (fresh distro install, never run Virtues before):**

```bash
curl -sSL https://virtues.com/sh | sudo sh
sudo systemctl stop virtues
sudo -u virtues virtues restore /tmp/migration.tar.gz
sudo systemctl start virtues
sudo -u virtues virtues link
```

After the new box is up, revoke the old box's paired devices from the
new Devices page so the old hardware can't reach anything (the data is
already cloned, but you don't want lingering tunnels).

---

## Rolling back a `virtues upgrade`

`virtues upgrade` keeps one rollback copy at `/usr/local/bin/virtues.bak`.
If the new binary has a problem, the exact rollback command is printed
at the failure boundary. The standalone form is:

```bash
sudo systemctl stop virtues
sudo mv /usr/local/bin/virtues.bak /usr/local/bin/virtues
sudo systemctl start virtues
```

The `.bak` is overwritten on the next successful upgrade — there is only
ever one rollback level, not a history.

If the upgrade applied a schema migration and the rollback binary
can't read the migrated schema, **stop and restore from backup**
instead. Migrations are not generally reversible.

---

## 402 on chat

The chat call returns `402` with one of these error codes:

- **`insufficient_budget` / `wallet_empty`** — your wallet hit $0 and
  auto-top-up either didn't fire or failed. Open `Settings →
  Billing`; either flip auto-top-up back on (it auto-disables after 3
  consecutive failures) or top up manually via Stripe portal.
- **`topup_disabled`** — auto-top-up is off (by you or by the 3-strike
  breaker). Same fix as above; if the breaker tripped it's because
  Stripe declined the card three times in 24h. Update the payment
  method in the Stripe portal first, then re-enable auto-top-up.
- **`bearer_expired`** — voucher needs renewal. The box does this
  automatically on the next call; if you see this code repeatedly,
  run `sudo -u virtues virtues subscribe` to relink.
- **`monthly_cap_reached`** — you hit the locked monthly cap ($100
  default in the v3 economic model). Wait until next month or raise
  the cap from Settings.
- **`subscription_inactive`** — Stripe says the subscription was
  cancelled or payment is past-due. Open the Stripe portal from
  Settings → Billing → Manage Subscription.

You can also bypass the wallet entirely by setting a BYO provider key
under `Settings → AI Provider Key` — chat then routes box → upstream
provider directly with your key, and Virtues is out of the AI path.

---

## `voucher_too_soon` — stuck account (operator-only)

If `virtues login` or the renew cron repeatedly returns:

```
429 Too Many Requests — {"error":{"code":"voucher_too_soon",
  "message":"a voucher was issued recently; wait until near expiry"}}
```

The customer is locked out by atlas's 25-day anti-stacking gate. This
should be rare after the box-side double-renew bug fix (`deploy.rs`),
but legacy state from older boxes can still trigger it.

**Atlas operator unstick** (requires Postgres access on atlas):

```sql
-- Look up the customer by Stripe ID or email
SELECT stripe_customer_id, last_voucher_issued_at
  FROM customers
  WHERE stripe_customer_id = 'cus_XXXX';

-- Clear the gate
UPDATE customers
   SET last_voucher_issued_at = NULL
 WHERE stripe_customer_id = 'cus_XXXX';
```

Next `virtues login` / next renew cron tick will mint a fresh voucher
cleanly. No box-side action required.

---

## Source-connect requires LAN

Connecting Google / Notion / Plaid / Strava / GitHub from a browser
**requires you to be on the same home network as the box.** The OAuth
provider redirects the final hop to `http://localhost:8000/oauth/callback`,
which only resolves on your home WiFi.

v1 ships an intermediary "Almost done — click to continue on your home
network" page when atlas detects the `.local` callback target, so the
failure mode is at least clear instead of a blank DNS error page.

If you're traveling and want to add a source, wait until you're home.
Remote source-connect ships in v1.1 with the WireGuard remote-access
layer.

---

## Diagnostic beacons

By default, the box sends two kinds of anonymized beacons to
`atlas.virtues.com`:

- **Install beacon** (`POST /diag/install`) — fires once at the end of
  `tools/bootstrap.sh`. Payload: `{box_id, distro, version, arch, outcome,
  failed_step}`. No personal data, no source content.
- **Crash beacon** (`POST /diag/crash`) — fires from systemd's
  `ExecStopPost=` hook when the daemon exits abnormally (signal,
  core-dump, watchdog, non-zero exit). Payload: `{box_id, version,
  service_result, exit_code, journal_tail (last 50 lines)}`. The
  journal tail is included so we can triage without a support ticket;
  if your logs happen to contain something sensitive, the tail
  capture is bounded to 16 KB and you can disable beacons entirely
  (below).

The `box_id` is a SHA-256 prefix of `/etc/machine-id` (or hostname as
fallback). It's enough to dedupe retries from the same box, not enough
to identify you.

**To disable both beacons:** add a line to `/etc/virtues/env`:

```
VIRTUES_DIAG=off
```

Then restart the service:

```bash
sudo systemctl restart virtues
```

(Or `false`, `0`, `no`, `disabled` — all case-insensitive.)

The install beacon respects the same flag, but the installer reads it
from the env file *after* it's written, so the first install ever
will always send one `outcome=ok` beacon before the off-switch takes
effect. If that one is unacceptable, set the env var in your shell
before running the installer:

```bash
sudo VIRTUES_DIAG=off bash -c 'curl -sSL https://virtues.com/sh | sh'
```

---

## When this guide doesn't cover your problem

Paste the output of:

```bash
sudo -u virtues virtues status --json
```

That captures: binary version, schema version, paired device count,
sudo state, pair-token state, billing flags, action subprocess health,
recent auth events. It's the boring-but-complete diagnostic; with it,
support can triage without a back-and-forth.

If the daemon won't even start enough to run `virtues status`, paste
the last 100 lines of the journal instead:

```bash
sudo journalctl -u virtues -n 100 --no-pager
```
