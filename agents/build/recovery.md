# Recovery & Operator Guide

> **Status: Current.** Rewritten 2026-08-28 against the code after an audit
> found ~19 stale or wrong claims in the previous version — including three
> commands and one atlas table that do not exist, and a config path no box has.
>
> **This is the workshop copy.** The user-facing runbook now lives at
> [`docs/operate/recovery.md`](../../docs/operate/recovery.md) and ships to
> `virtues.com/docs/operate/recovery`. Anything an owner reads belongs there;
> **this file is for the people and agents working on the box** — it carries the
> operator-only material (atlas-side actions, the diagnostic surface, the
> list of what is described but not implemented) and the pointers into source.
>
> When the two disagree, **the manual wins, because it is written from source.**

If you are triaging a live box and want one thing to paste:

```bash
virtues status --json
```

---

## Triage cheatsheet

| Symptom | Section |
|---|---|
| Phone/laptop can't reach the box | [Reaching the box](#reaching-the-box) |
| Lost every paired device | [Locked out](#locked-out) |
| Want to replace the BYO AI key | [Reset the BYO key](#reset-the-byo-key) |
| `systemctl status virtues` failing | [Service won't start](#service-wont-start) |
| Postgres won't start | [Postgres won't start](#postgres-wont-start) |
| Search empty or wrong while the box is healthy | [Search is broken](#search-is-broken) |
| `curl -sSL https://virtues.com/sh \| sudo sh` failed partway | [Install failed mid-run](#install-failed-mid-run) |
| Bad release just landed | [Upgrades and rollback](#upgrades-and-rollback) |
| Moving to new hardware | [Migrating to new hardware](#migrating-to-new-hardware) |
| Chat returns 402 | [402 on chat](#402-on-chat) |
| OAuth source-connect fails away from home | [Source-connect and networks](#source-connect-and-networks) |
| Want to disable the crash beacon | [Diagnostics](#diagnostics) |

---

## Where things live

Read this before any recipe below. **The single most damaging error in the
previous version of this doc was pointing at `/etc/virtues/env`** — a path the
installer has never written — so instructions to add a line there silently did
nothing on every real box.

| What | Path |
|---|---|
| Everything the box owns | `/var/lib/virtues` |
| **Config + secrets (`VIRTUES_ENCRYPTION_KEY`, `VIRTUES_DIAG`, …)** | **`/var/lib/virtues/virtues.env`** |
| Data lake (recordings, drive files) | `/var/lib/virtues/lake` |
| Models | `/var/lib/virtues/models` |
| Backups | `/var/lib/virtues/backups` |
| Backup recipient (**public** half only) | `/var/lib/virtues/backup-recipient` |
| Release channel | `/var/lib/virtues/channel` |
| Release slots | `/var/lib/virtues/releases/<slot>/` |
| The binary | `/usr/local/bin/virtues` → symlink into the live slot |

The installer writes `<data_dir>/virtues.env` and points the unit's
`EnvironmentFile=` there (`tools/virtues-installer/src/install.rs:1816`,
`:3293`).

`cli/backup.rs` probes **both** paths — `ENV_CANDIDATES = ["/etc/virtues/env",
"/var/lib/virtues/virtues.env"]` — preferring the FHS one so a box that ever
migrates wins. That fallback is why backups work; it is not a claim that
`/etc/virtues/env` exists. **Two doc comments in the tree still say
`/etc/virtues/env` as if it were the only path** (`middleware/auth.rs:41`,
`api/pair.rs:1310`). They describe an intended layout that no real box has.

`VIRTUES_ENV_FILE` overrides both candidates, for installs that moved
`DATA_DIR`.

---

## Reaching the box

The box serves plain HTTP on **port 8000**. There is no TLS on the box — see
[networking-relay-tee.md](../archive/networking-relay-tee.md) for why.

Transport is **iroh**, not WireGuard. WireGuard was removed; `deploy.rs` keeps
the field name `paired_wg` only for API stability, and any doc sentence about a
"WireGuard handshake" or a "WireGuard remote-access layer" is describing a
transport this codebase no longer has.

What actually reaches the box:

- **On the box itself:** `http://localhost:8000`.
- **The iPhone and desktop apps**, by dialing the box's iroh EndpointId —
  direct on the LAN, hole-punched across the internet, or through the blind
  relay. No inbound port is opened at home.
- **The desktop helper binds `127.0.0.1:7117`** and splices whatever connects
  to it over that iroh stream (`crates/virtues-reach-client/src/proxy.rs`,
  `server/mod.rs:1183`). That is the loopback origin the paired web UI is
  served from on a laptop — **not** `localhost:8000`.
- **A plain browser cannot pair and is not a client.** It has no iroh key, so
  a browser pointed at `http://<box-ip>:8000` from another machine is refused
  like any stranger.

Full model: [`docs/operate/reach.md`](../../docs/operate/reach.md).

---

## Locked out

Pair-only auth means there is no email-magic-link recovery. The recovery path is
**a terminal on the box** — SSH, console, or attached keyboard.

```bash
virtues pair
```

`pair` is the verb (`login` and `link` survive as aliases). It **prints a typed
code and waits** — not a URL, not a QR, because a browser cannot pair and the
desktop app has no camera. `--no-wait` prints and exits, for scripts.

The allowlist as a CLI:

```bash
virtues device ls     # who can reach this box
virtues device rm ID  # de-allowlist — next dial refused at the handshake
virtues device add    # print a pair code (pair, scoped to the allowlist framing)
```

**If the Devices page refused to revoke your last device**, that is the
anti-lockout guard; nothing was deleted. Pair a second device from the box
first, then revoke.

**On appliance hardware there is a physical fallback**: the button behind the
case. A long press forgets every paired device — and nothing else. By
design it is *not* a factory reset: it shares exactly one action with the app's
`/api/pair/reopen-onboarding`, and the claim phrase freezes at first claim and
never returns to the screen. That is what prevents someone with a screwdriver
from taking over the box. See `virtues-core/src/maintenance/reset_button.rs`.

---

## Reset the BYO key

The key is one `credentials` row: `kind = 'source'`, `key = '__byo_ai_key__'`
(`BYO_SOURCE_ID` in `virtues-core/src/api/settings_byo.rs`). Every AI call
reads the active row, so a change applies on the next call.

From the UI: `Settings → Billing → Bring your own AI → Manage`. Remove clears
the key (`delete_handler`). Saving a new one revokes the old row and inserts
a new one (`save_handler`), so you do not need to remove first. Both ask for
the `change_byo_key` approval; approve at the box with `sudo -u virtues
virtues sudo`.

When no paired device can reach the UI, run the same Remove from a shell on
the box. The server treats a request to `localhost:8000` with no
forwarding header as the box's local console (`virtues-core/src/middleware/auth.rs`), so `curl`
can ask for the approval and then call `delete_handler`:

```bash
# 1. Ask for the approval. Copy the "id" from the answer.
curl -s -X POST http://localhost:8000/api/sudo/request \
  -H 'content-type: application/json' -d '{"action":"change_byo_key"}'

# 2. Approve it within five minutes.
sudo -u virtues virtues sudo --id REQ

# 3. Remove the key. A working remove answers {"ok":true}.
curl -s -X DELETE http://localhost:8000/api/settings/byo-key \
  -H 'content-type: application/json' -d '{"sudo_request_id":"REQ"}'
```

Step 3 runs the same handler as Remove, so it changes the row and writes the
`byo_key_cleared` audit row exactly as Remove does. If the server is not
running, nothing is calling the provider; get it running first
([Service won't start](#service-wont-start)).

Do not revoke the row by hand with `psql`. An `UPDATE` that only sets
`status` leaves the old key sealed in the revoked row, and nothing removes it
later, because Remove and a new save only touch the active row. Do not
delete the row either: `first_key_during_setup` counts revoked rows, so
deleting it lets the first paired device save a key without the approval
while setup is still open.

With no active row, AI calls go to the Virtues subscription.

### The sudo gate

Two actions need an approval from `virtues sudo`, listed in `GATED_ACTIONS`
(`virtues-core/src/api/sudo.rs`): `change_byo_key` and `import_applet_package`.
A request lasts five minutes and can be used once.

The approval shows that someone ran a command as the `virtues` user on the
box. The comments in `api/sudo.rs` call that physical access, but the web
terminal of any paired device also runs as that user, so a paired device can
approve its own request
([record/secrets-audit-2026-10-27.md](../record/secrets-audit-2026-10-27.md),
section 7). The owner decided to replace this gate with the owner proof
([plan/secrets-plan.md](../plan/secrets-plan.md), "Who can read what"). Until
that ships, it works as below.

```bash
sudo -u virtues virtues sudo          # list open requests, prompt for each
sudo -u virtues virtues sudo --id REQ # approve one
```

Answering no at the prompt denies a request. `--deny` has no effect today:
clap refuses it beside `--id`, and the interactive list ignores it.

---

## Service won't start

```bash
sudo systemctl status virtues
sudo journalctl -u virtues -n 100 --no-pager
```

Everything logs to the journal; there is no Virtues log file.

- **"waiting for postgres to accept connections…"** — normal on first boot;
  Postgres takes 10–30s to finish WAL recovery. Past ~2 minutes, see below.
- **"VIRTUES_ENCRYPTION_KEY not set"** — `/var/lib/virtues/virtues.env` is
  missing or the key line is empty. Restoring from a backup brings the key with
  it. If there is nothing to restore, the box is uninitialized: re-run the
  installer.
- **"Failed to bind 0.0.0.0:8000"** — `sudo ss -lntp '( sport = :8000 )'`.
- **OOM kill** — `dmesg | grep -i oom`, then `free -h`. The embedding
  sidecar wants about 1 GB resident, two during a model change.

### The other units

| Unit | What it is |
|---|---|
| `virtues` | the server, port 8000 |
| `virtues-embed` | embedding sidecar, `127.0.0.1:18181` |
| `virtues-embed-next` | only during a search model change, `127.0.0.1:18183` |
| `virtues-rerank` | older installs only; `virtues upgrade` retires it |
| `virtues-qnnd` | on NPU hardware, replaces the sidecars and serves `:18181` and `:18182` |
| `virtues-display` | the on-box screen, where there is one |

**The kiosk caches the SPA**, so after an upgrade the panel can draw the old
interface: `sudo systemctl restart virtues-display`.

---

## Postgres won't start

```bash
sudo systemctl status postgresql
sudo journalctl -u postgresql -n 50 --no-pager
```

- **Disk full.** `df -h /var/lib/postgresql`. Below ~10 MB free it won't accept
  writes or start cleanly.
- **Permission damage.** `/var/lib/postgresql/18/main/PG_VERSION` should be
  `postgres:postgres`; after a bad restore,
  `sudo chown -R postgres:postgres /var/lib/postgresql`.
- **WAL corruption** (usually hardware). `invalid checkpoint record` or `record
  with incorrect prev-link` in the journal. Restore from backup if you have
  one; do not reach for `pg_resetwal` without knowing what it costs.
- **Version mismatch.** Postgres refuses a data directory from another major
  version. Use `virtues backup` / `virtues restore`, never a raw data-dir copy.

---

## Search is broken

Search failing while the box is otherwise healthy is usually a sidecar or an
index built by a different model than the one now answering.

```bash
virtues doctor                # accelerator, CUDA linkage, which models are present — no DB needed
systemctl status virtues-embed
journalctl -u virtues-embed -n 50
virtues configure-inference   # re-probe the endpoint after a model change; --reembed to skip the prompt
virtues reindex               # rebuild vector + BM25 from source; source data untouched
```

`virtues doctor` reads no database, which is exactly why it still answers when
other things are broken.

---

## Install failed mid-run

The installer is idempotent; re-running picks up where it stopped.

```bash
curl -sSL https://virtues.com/sh | sudo sh
```

Common recoverable causes: an unreachable package mirror, a `sudo` timeout (run
`sudo -v` first), or `postgresql-18` missing from the distro's default repos —
the installer adds the PGDG repo for Ubuntu 24.04/25.04, but older distros need
upgrading first.

**There is no install beacon.** The previous version of this doc said a failed
step name is posted to atlas at the end of the run. Atlas has the receiver
(`services/virtues-atlas/src/routes/diag.rs` routes `/diag/install`), but
**nothing on the box ever posts to it** — the installer contains no beacon code
and no `VIRTUES_DIAG` handling at all. A failed install is therefore invisible
to us unless someone says so. Its "pre-set `VIRTUES_DIAG=off` before installing"
workaround was guarding against a request that is never made.

Also note that installer bugs hide in both directions — dev and CI each mask a
different class of them (see [installer env divergence](deployment.md#installer-env-divergence)); passing
CI does not show that the installer works.

---

## Backups

```bash
virtues backup --init-key   # ONCE, from a terminal you are watching
virtues backup
```

**`--init-key` is required and nothing else mints the key.** Not a first
backup, and never a scheduled run — a key minted where nobody is reading
produces archives nobody can ever open. A first `virtues backup` on a box with
no recipient **fails**; the previous version of this doc said the first backup
mints one, which would have left owners believing they had been shown a secret
they never saw.

The archive is `tar → gzip → age`, written to
`/var/lib/virtues/backups/virtues-<utc-iso>.tar.gz.age`. It carries the Postgres
dump, the lake, the applet state, the env file (i.e. the encryption key), and a
`manifest.json` with a sha256 of every member. **Because the env file is inside,
the tarball is as sensitive as the box itself.**

The box keeps only the **public** half of the age keypair at
`/var/lib/virtues/backup-recipient`, so by design a box cannot decrypt its own
backups. The secret half is shown once by `--init-key` and cannot be
recovered from the box. Archives are standard [age](https://age-encryption.org)
files and open with the `age` CLI anywhere.

Other verbs worth knowing before you need them:

```bash
virtues backup --verify ARCHIVE --key-file KEY   # decrypt, re-hash, compare to manifest
virtues backup --volume ID|all                   # full + increment to a registered drive
virtues volumes                                  # register and inspect destinations
virtues backup --allow-missing-key               # dev boxes only; the result cannot decrypt itself
```

A backup that nobody has ever opened has not been shown to restore. `--verify`
is the cheap way to check one.

## Restoring

```bash
sudo systemctl stop virtues
sudo virtues restore --key-file /path/to/recovery-key /path/to/virtues-….tar.gz.age
sudo systemctl start virtues
```

`--from-volume PATH` restores from a backup drive instead: its newest full
archive, then every increment in order. It takes a **path** (the mount point),
not a registered volume id — the registry lives in the database being restored,
so on replacement hardware there is nothing to look up.

Three checks run before anything is touched: the service is stopped (`--force`
overrides), the archive's schema version is not newer than this binary
(never bypassable — upgrade first), and every sha256 matches the manifest
(never bypassable).

It then drops and recreates the database, replaces the lake and applet state,
and writes the env file **to whichever candidate path already exists on this
box**, falling back to `ENV_CANDIDATES[0]` (`cli/restore.rs:29`).

Owner-facing version: [`docs/operate/backup-and-restore.md`](../../docs/operate/backup-and-restore.md).

---

## Migrating to new hardware

```bash
# old box
virtues backup --volume all      # or: virtues backup --output /tmp/migration.tar.gz.age
```

```bash
# new box, fresh distro
curl -sSL https://virtues.com/sh | sudo sh
sudo systemctl stop virtues
sudo virtues restore --key-file /path/to/recovery-key /tmp/migration.tar.gz.age
sudo systemctl start virtues
virtues pair
```

Then revoke the old box's devices from the new box (`virtues device rm`).

Note that a restore brings the old box's **identity** with it — the database
stores the iroh secret. If you are imaging hardware rather than migrating one
box, that is `virtues deprovision` followed by `virtues image-check`, not this;
see [appliance-image.md](appliance-image.md).

---

## Upgrades and rollback

Upgrades stage a whole release into `releases/<slot>/`, preflight it (the
**staged** binary must pass `migrate --check` plus a version smoke test), then
activate by flipping the `current` symlink — binary, web, and actions move
together. Failures before the flip leave the box untouched; failures after it
flip straight back.

```bash
virtues upgrade --check      # report only
virtues upgrade              # follow the box's channel
virtues upgrade --pre        # one-off: newest prerelease
virtues channel prerelease   # persist it — --pre forgets itself
virtues prepare              # stage + preflight, don't install
virtues activate             # install what prepare staged
sudo virtues rollback        # flip back to the previous slot and restart
```

**Rollback is `sudo virtues rollback`.** There is no `/usr/local/bin/virtues.bak`
— that file exists nowhere in this codebase, and the three-command `mv` recipe
the previous version of this doc gave would have deleted the live binary.

Schema is not rolled back; migrations only go forward and the previous release
tolerates a newer schema. If the previous release genuinely cannot read the
migrated schema, restore from backup instead.

`virtues upgrade --only web,actions` refreshes the payload inside the current
release with no binary swap, no migration, and no restart — the fast path for UI
iteration.

Full model: [update-paradigm.md](../record/update-paradigm.md),
[`docs/operate/upgrading.md`](../../docs/operate/upgrading.md).

---

## 402 on chat

Codes the box branches on (`virtues_api/renew.rs`, `virtues_api/client.rs`):

- **`wallet_empty` / `insufficient_budget`** — the wallet hit zero and
  auto-top-up didn't fire or failed. `Settings → Billing`: re-enable auto-top-up
  (it disables itself after 3 failures in 24h) or top up via the Stripe portal.
- **`topup_disabled`** — auto-top-up is off, by you or by that breaker. Fix the
  card in the portal first, then re-enable.
- **`card_declined` / `authentication_required`** — Stripe needs the card
  updated, or the payment needs 3-D Secure completed in the portal.
- **`monthly_cap_reached`** — the locked monthly cap. Wait, or raise it in
  Settings.
- **`subscription_inactive`** — Stripe says cancelled or past-due. Stripe portal
  from `Settings → Billing`.
- **`bearer_expired`** — renewed automatically on the next call. Repeatedly, run
  `virtues subscribe` (hidden) to relink.

A BYO provider key bypasses the wallet entirely: chat routes box → provider
directly and Virtues leaves the AI path.

> **`voucher_too_soon` is gone.** The previous version of this doc carried a
> whole atlas-operator section for it. The 25-day anti-stacking gate was
> removed: `last_voucher_issued_at` was dropped by
> `services/virtues-atlas/migrations/0009_rename_billing_token.sql:17`, and the
> error code appears nowhere in the codebase. The unstick SQL it published would
> now fail on a column that does not exist.

---

## Source-connect and networks

The OAuth return hop goes back to the box, so **the browser's location matters,
though less than it used to.**

`services/virtues-api/src/routes/oauth.rs` classifies the `return_url`:

- **Loopback** (`localhost`, `127.0.0.1`) gets the seamless 302 by design.
  That covers a browser on the box *and* the desktop helper's
  `127.0.0.1:7117` origin — so connecting a source through the desktop app works
  off-LAN.
- **A private IP or a `.local` name** gets the "Almost done — continue on my
  home network" interstitial. The click still fails when you are away, but it
  fails with a clear message instead of hanging on a TCP timeout.

In short, a browser aimed straight at the box's LAN address needs you to be home. The
apps do not.

---

## Diagnostics

There is exactly **one** beacon on the box, and it is the crash beacon.

**Crash beacon** — `POST <atlas>/diag/crash`, from systemd's `ExecStopPost=`
hook, only when `SERVICE_RESULT` is `signal`/`core-dump`/`watchdog`/`abort`/
`oom-kill` or a non-zero `exit-code`. A clean `systemctl stop` sends nothing.
Payload: `{box_id, version, service_result, exit_code, exit_status,
journal_tail, ts}` (`cli/report_crash.rs`).

Two corrections to what the previous version of this doc claimed:

- **The journal tail is not bounded to 16 KB.** It is `journalctl -u
  virtues.service -n 50` with no byte cap — 50 lines can be arbitrarily large.
  If that matters for your box, turn the beacon off; there is no partial knob.
- **`box_id` never reads `/etc/machine-id`.** `cli/diag.rs:74` returns
  `VIRTUES_BOX_ID` if set — and **nothing sets it**; no installer, no unit, no
  firstboot script — so in practice every box falls through to
  `h:<sha256(hostname)[..16]>`. Stable and anonymous, but derived from the
  hostname, not the machine id.

**To disable it**, add to `/var/lib/virtues/virtues.env` (the path systemd's
`EnvironmentFile=` actually loads — `diag::enabled()` reads the process env, so
a line anywhere else has no effect):

```
VIRTUES_DIAG=off
```

then `sudo systemctl restart virtues`. `false`, `0`, `no`, and `disabled` all
work, case-insensitively. Default is on.

`virtues status --json` reports the current state as `diag_enabled`.

---

## When this doesn't cover it

```bash
virtues status --json
```

Emits `{schema_version, virtues_version, diag_enabled, box_id, auth, sudo,
pair, billing, actions, network, recent_events}` — device count, pending and
consumed sudo requests, pair-token counts, auto-top-up and BYO state, applet
health, and the last 10 auth events. Excluded by design: secrets of any kind,
user content, and paired-device IPs — so it is safe to paste.

If the daemon won't start far enough to run it:

```bash
sudo journalctl -u virtues -n 100 --no-pager
virtues doctor
```

`doctor` needs no database and answers when almost nothing else does.
