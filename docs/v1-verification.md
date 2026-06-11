# v1 Verification — Fresh Install on Real Hardware

> Paste outputs in-line as you go. Treat any ❌ as a v1 blocker — fix
> before tagging the release as final.

The bar is: a person who is not Adam, on a clean Linux box they own,
running `curl -sSL https://get.virtues.com | sudo sh`, can land in the
web UI, connect a source, and complete a chat without manual
intervention beyond the steps documented in the README.

Two target environments at minimum:

- **Real hardware (Jetson Orin or similar aarch64 + Linux)** — the
  authentic "this is what users will actually run on" test
- **Clean x86_64 VM (Debian 13 or Ubuntu 24.04)** — cheaper to iterate;
  catches distro-specific issues hardware tests would miss

For v1 ship, both must pass. Run the matrix on each.

---

## Pre-flight (one time, before any target tests)

These run on Adam's dev machine, not on the target hardware.

### P1 — Working tree is clean and committed

```bash
git status --porcelain | wc -l    # expect: 0
git rev-parse --abbrev-ref HEAD   # expect: main
git rev-parse HEAD                # expect: matches origin/main
```

If WIP exists, decide what's shipping in v0.1.0 and what's not. Avoid
"ship a tag with half-staged work" at all costs.

### P2 — `get.virtues.com` Caddy is up

```bash
curl -sI https://get.virtues.com | head -1     # expect: HTTP/2 200
curl -sSL https://get.virtues.com | head -2    # expect: "#!/usr/bin/env bash"
```

If 404: the EC2 Caddy reverse-proxy isn't healthy or the Route 53
record drifted. See `docs/deployment.md` for the cloud-sidecar setup.

### P3 — `.sqlx` cache is up to date

```bash
SQLX_OFFLINE=true cargo build -p virtues --bin virtues
# expect: Finished `dev` profile
```

If this fails with "no cached data for this query," run
`cargo sqlx prepare --workspace` against a live local Postgres before
tagging. The release CI uses offline mode.

### P4 — Tag and push v0.1.0

```bash
git tag -a v0.1.0 -m "v0.1.0 — first ship"
git push origin v0.1.0
```

Watch the workflow at
`https://github.com/virtues-os/virtues/actions`. Both arches must
finish green. Once they do, verify both tarballs landed on the
release page:

```bash
curl -sI https://github.com/virtues-os/virtues/releases/download/v0.1.0/virtues-v0.1.0-x86_64-linux.tar.gz  | head -1
curl -sI https://github.com/virtues-os/virtues/releases/download/v0.1.0/virtues-v0.1.0-aarch64-linux.tar.gz | head -1
curl -sI https://github.com/virtues-os/virtues/releases/download/v0.1.0/virtues-v0.1.0-x86_64-linux.tar.gz.sha256 | head -1
```

All three should be `HTTP/2 302` redirects → eventual `HTTP/2 200`.

If the binaries didn't upload, the release pipeline broke; fix CI
before continuing.

---

## Target: Jetson (or other aarch64 Linux box)

Capture before-state if the Jetson is currently doing anything you
care about:

```bash
sudo -u virtues virtues backup --output /mnt/persistent/pre-v1.tar.gz   # if applicable
```

### Install

| Step | Command | Expected | ✅ / ❌ + notes |
|---|---|---|---|
| Fresh install | `curl -sSL https://get.virtues.com \| sudo sh` | Runs to completion. Last 10 lines include the "📡 Anonymized install + crash beacons are ON" notice and the next-steps footer. No `set -e` abort mid-script. | |
| Postgres installed | `sudo systemctl is-active postgresql` | `active` | |
| Virtues binary present | `/usr/local/bin/virtues --version` | `virtues 0.1.0` or matching tag | |
| Systemd unit installed | `sudo systemctl status virtues` | `loaded; enabled; active (running)` | |
| Service didn't restart-loop | `sudo journalctl -u virtues \| grep -c "Started Virtues"` | `1` (one start, not many) | |
| pg_isready gate ran | `sudo journalctl -u virtues \| grep "waiting for postgres"` | Either absent (PG was ready instantly) or present once with a clean `service started` line right after | |
| mDNS broadcasting | `avahi-browse -tap _http._tcp \| grep virtues` (run from a Mac/Linux on the same LAN) | One line containing `virtues.local` on port 8000 | |
| HTTP listener reachable | `curl http://localhost:8000/health` (on the Jetson) | `200 OK` | |
| Install beacon arrived | atlas log shows `POST /diag/install … outcome=ok` from a fresh `box_id` within ~30s of install end | (check atlas SRE channel / DB) | |

### First-pair from the box's own browser

(Other-device pairing waits for the Virtues client daemon — v0.2.)

| Step | Command | Expected | ✅ / ❌ + notes |
|---|---|---|---|
| `virtues link` runs | `sudo -u virtues virtues link` | Prints `http://localhost:8000/pair#t=...` plus a LAN fallback URL | |
| Open the URL | Paste into Chromium on the Jetson | Lands in the web UI logged in, on `/onboarding`. No cert warning — loopback is a Secure Context | |
| `/virtues/devices` shows the device | Navigate after onboarding | One row for the laptop, label includes OS + browser, last-seen "just now" | |
| Activity log records the pair | `/virtues/activity` | One `paired` event, IP matches laptop | |

### Onboarding + first source

| Step | Command | Expected | ✅ / ❌ + notes |
|---|---|---|---|
| Onboarding wizard | Click through | First-source CTA points to `/sources?welcome=1` | |
| Connect Google | Click → atlas OAuth → return | Lands back on `/sources` with Google connected; calendar+gmail+drive credentials visible | |
| First sync completes | Wait ~1–2 minutes | At least one `app_action_runs` row with `status='success'` for a Google action | |

### Subscribe + first chat

| Step | Command | Expected | ✅ / ❌ + notes |
|---|---|---|---|
| `virtues subscribe` | `sudo -u virtues virtues subscribe` | Prints QR + device-code URL; opens browser to Stripe checkout | |
| Stripe test card | Pay with `4242 4242 4242 4242` | Atlas link goes to `Ready`; `virtues subscribe` returns "✅ wallet credited — AI ready." | |
| Wallet visible | `/virtues/billing` Wallet & top-up panel | Wallet section renders, auto-top-up toggle present and `On` by default | |
| First chat call | Send any message in the chat UI | 200 OK, streamed response | |
| Wallet decrements | Refresh `/virtues/billing` | Wallet balance lower than before by ~$0.001 (or whatever the call cost) | |

### Lifecycle

| Step | Command | Expected | ✅ / ❌ + notes |
|---|---|---|---|
| Reboot | `sudo reboot` | Box comes back; `virtues.service` is `active` within 60s | |
| Auth survives reboot | Visit `http://localhost:8000/` from Chromium on the Jetson | Still logged in (cookie + device row both present) | |
| Backup | `sudo -u virtues virtues backup` | Tarball written to `/var/lib/virtues/backups/virtues-…tar.gz`; manifest.json valid JSON | |
| Backup integrity | `tar -tzf <tarball> \| head` | Contains `manifest.json`, `db/virtues.dump`, `env/virtues.env`, `lake/…` | |
| Status JSON | `sudo -u virtues virtues status --json \| jq .` | Valid JSON; `schema_version` matches binary; no secrets in output | |
| Upgrade check | `sudo -u virtues virtues upgrade --check` | Reports "already on $version" or "upgrade available" — no panic | |

### Sudo flow

| Step | Command | Expected | ✅ / ❌ + notes |
|---|---|---|---|
| Trigger sudo from web | Settings → AI Provider Key → Save (with any test key) | Modal: "Run this on the box: `sudo -u virtues virtues sudo`" with a 5-min countdown | |
| Approve on Jetson | `sudo -u virtues virtues sudo` | Prints pending request (action: change_byo_key, label, IP, time) and prompts y/N | |
| Approve `y` | | Modal flips to "Confirmed", reloads; BYO key saved | |
| Activity log records the approval | `/virtues/activity` | `sudo_requested` + `sudo_approved` rows visible | |

### Diagnostic opt-out

| Step | Command | Expected | ✅ / ❌ + notes |
|---|---|---|---|
| Disable diag | `echo 'VIRTUES_DIAG=off' \| sudo tee -a /etc/virtues/env && sudo systemctl restart virtues` | Service comes back up | |
| Crash beacon respects opt-out | `sudo systemctl kill -s SIGKILL virtues; sleep 5; sudo journalctl -u virtues -n 20` | `report-crash` runs but `enabled()` returns false; no POST to atlas | |

### Restore on a fresh Jetson reimage (only if you have a second device)

| Step | Command | Expected | ✅ / ❌ + notes |
|---|---|---|---|
| Reimage Jetson, re-install Virtues | (full wipe + reinstall) | `virtues.service` active, fresh state | |
| Restore the earlier backup | `sudo systemctl stop virtues && sudo -u virtues virtues restore /path/to/backup.tar.gz` | Three gates pass; restore prints "next steps" | |
| Service comes back | `sudo systemctl start virtues` | Active. Web UI shows the same paired devices, chat history, sources as pre-wipe | |

---

## Target: clean x86_64 VM (Debian 13 / Ubuntu 24.04)

Same matrix as above. Run after the Jetson pass — distro-specific
differences usually surface in `provision_db` or `install_systemd_unit`
on x86_64 ahead of aarch64.

---

## Sign-off

When every row in the Jetson matrix is ✅ and every row in the VM
matrix is ✅, v0.1.0 is verified for ship. Commit this file with the
outputs filled in as a record.

If a row is ❌:

1. Open an issue with the row name and the actual output
2. Decide whether it blocks v0.1.0 or can ship as a documented
   limitation
3. If it blocks: fix on `main`, tag v0.1.1 candidate, re-run the
   relevant subset of this matrix (not the whole thing)
