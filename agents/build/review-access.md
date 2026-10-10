# App Store review access

How an Apple reviewer — who owns no box and cannot reach yours — exercises the
iOS app well enough to clear guideline 2.1.

## The problem

Pairing is limited to the LAN, and not by any check we could relax:

- `/api/pair/consume` is plain HTTP to the box's own origin. The box has no TLS
  surface and no public inbound port (`virtues-core/src/server/mod.rs`).
- The iroh relay cannot carry the pairing step: `crates/virtues-iroh/src/server.rs`
  rejects connections from peers that are not already allowlisted, and pairing
  is *how* a peer gets allowlisted. The relay ticket is the output of consume,
  not an input.

There is no co-location logic to remove. `consume_handler` performs no IP or
subnet validation, and the client's `normalize_server`
(`apps/web/plugins/reach/src/lib.rs`) accepts any `http(s)` origin. "Same
Wi-Fi" is UI copy, not enforcement. So a box that *is* publicly reachable
already pairs from anywhere — the missing piece was only a code that lives long
enough for a review cycle.

Existing code lifetimes are far too short: `oneoff` is 5–30 min, `standing`
rotates every ~20 min. A reviewer may open the app days after submission, and
resubmissions add rounds.

## The mechanism

`VIRTUES_REVIEW_PAIR_CODE` in **`/var/lib/virtues/virtues.env`** — the file
the unit actually reads (`EnvironmentFile=-/var/lib/virtues/virtues.env`), and
the one the installer writes. This doc said `/etc/virtues/env` until 2026-09-03;
on a real box that directory exists and is **empty**, so the variable goes into
a file nothing loads, no row is installed, and the only symptom is a reviewer
who cannot pair. At startup `api::pair::ensure_review_code` installs one
`app_pair_token` row with `kind = 'review'`, `status = 'authorized'`, and a
nominal 10-year expiry.

Nothing else changes. `claim_pair_token` already accepts any authorized,
unexpired row and only consumes `kind = 'oneoff'`, so a review code is
multi-use and permanent for free. The `'review'` value in the `kind` CHECK
constraint arrived as `0058_review_pair_code.sql`, which folded into
`0001_initial.sql` in the 2026-08-18 squash — there is no separate migration to
look for any more, and no release since carries the one without the other.

The code stays **6 digits** because the mobile pairing input is
`inputmode="numeric"` with `maxlength="7"` (`src-tauri/ui/connect.html`). A
longer, higher-entropy token would be untypeable there. Startup refuses
anything that is not exactly 6 digits rather than installing a code a reviewer
cannot enter.

### Why the env gate is the whole safety story

A 1M keyspace on a public origin is acceptable only because two things hold at
once. First, `/api/pair/consume` is rate-limited to **10 attempts per IP per
30-minute window**, which puts a single-source sweep of the space far out of
reach. Second, such a box holds synthetic seed data — a successful guess exposes
a fake life. Absent the env var no review row is ever created, so customer boxes
cannot grow a permanent remote-pairing credential. **Never set this variable on a
box holding a real person's data.** An active review code logs a warning on every
boot for exactly this reason.

> **And the first of those two did not hold, for the whole life of this
> document.** `rate_limit_ip` believes `X-Forwarded-For` only when
> `VIRTUES_TRUSTED_PROXY` is set — off by default, and correctly so, because a
> stock box has no proxy and would otherwise let a LAN client mint a fresh
> budget per request. Behind Caddy, with the variable unset, `consume_handler`
> falls back to the socket peer, which is `127.0.0.1` — **and loopback is exempt
> from the limiter by design**, because an unforwarded loopback request is
> already treated as the owner. So the limiter never ran at all. Measured on the
> review box on 2026-09-03 before the fix: twelve consecutive bad codes, twelve
> 401s, no 429. Unlimited guesses against a 1M keyspace, where a hit earns a
> permanent allowlisted iroh device.
>
> **`VIRTUES_TRUSTED_PROXY=1` is therefore load-bearing on a review box, not a
> tuning knob** — it is provisioning step 6a, and the box now logs
> `REVIEW PAIR CODE IS UNRATE-LIMITED` at boot when the review code is active
> without it. Trusting the header is safe here specifically because the code
> takes the **right-most** entry: a client can prepend arbitrary hops, but
> cannot stop Caddy appending the real peer last. After the fix, the same twelve
> requests give ten 401s and then 429.

Residual risks on the demo box, all bounded and accepted: the rate limit is
per-IP, so a distributed sweep is slowed rather than stopped; a successful guess
earns a permanent iroh allowlist entry, could burn inference credits via
`virtues-api`, and would see anything the reviewer's device synced.

**Rotate the code whenever the box's address becomes known outside App Review**
— publishing the two together is what turns a slow, bounded guess into a targeted
one. Rotation is a one-line env change plus a restart; see
[Between review rounds](#between-review-rounds).

## Current demo box

**The live box's address, instance ID, security-group ID, and current pair code
are deliberately not in this repo.** They live in the private ops note alongside
the App Review submission record. This file describes the *shape* of the box, not
its coordinates.

The demo box is a small VPS at the same provider as the cloud server (moved
off AWS on 2026-10-05), with nothing else on it: a box install beside billing
would mean a second Postgres, a port clash, and dev-auth owner access next to
the billing service.

| | |
|---|---|
| Machine | VPS, 2 vCPU / 4 GB / 40 GB, x86_64 |
| Address | the VPS's fixed IPv4 + a random `demo-<rand>.virtues.ch` Route 53 record, and `demo.virtues.com` on the same address |
| Firewall | ufw: 22, 80, 443 only — iroh's UDP port stays closed, so reach always goes via the relay |
| Access | SSH, key-only, no root login |
| Monitoring | `virtues-health` every ten minutes with a demo check file (box, sidecars, both re-anchor timers, and the public doors: review identity 200, both hostnames 401 without credentials); the re-anchor and reset units email on failure; the cloud server also probes the review hostname, because a dead box cannot report itself |

An obscure hostname is not a security control — it only keeps opportunistic
scanners away, and it does even that only while it stays unpublished. The real
controls are the per-IP rate limit on `/api/pair/consume` (10 attempts per
30-minute window, keyed on the proxy-appended XFF entry — but ONLY with
`VIRTUES_TRUSTED_PROXY=1`; see the box above and `consume_handler` in
`virtues-core/src/api/pair.rs`), the synthetic seed data, and the box's
disposability. Verify the limit rather than assuming it: eleven bad codes in a
row must produce a 429.

## Provisioning

1. A VPS as in the table (both `x86_64` and `aarch64` releases are built),
   reinstalled with an SSH key so no password is ever mailed; then key-only
   sshd, no root login, ufw 22/80/443, unattended-upgrades.
2. Route 53 A records for both hostnames → the VPS address (TTL 60 while
   moving, so a switch takes effect in a minute).
3. Caddy in front: `demo-<rand>.virtues.ch { reverse_proxy 127.0.0.1:8000 }`.
   Port 80 must stay open for the ACME http-01 challenge.
4. 4 GB swap. 4 GB RAM is enough at rest (Postgres + `virtues` + two Q8_0 CPU
   sidecars ≈ 2.5–3 GB) but the seed index build wants headroom. Slow is fine
   here; OOM is not.
5. Install `virtues` from any current release (`gh release list`), pinning it
   with `VIRTUES_VERSION=vX.Y.Z`. Over a non-interactive ssh or remote-run there is no TTY, and the installer
   asks two questions — so both answers have to arrive as environment:

   ```sh
   curl -fsSL https://virtues.com/sh \
     | VIRTUES_VERSION=vX.Y.Z VIRTUES_INFERENCE=bundled sh -s -- --no-init
   ```

   `--no-init` skips only the interactive pairing handoff; the service is
   enabled and started before that step regardless. `VIRTUES_INFERENCE=bundled`
   is the one that is easy to miss: without it the installer reaches the
   Inference step, finds neither our hardware nor a terminal to ask on, and
   **exits 0** having installed nothing — a failure that reads as success in a
   log you are only skimming.

   **On a release older than the libgomp fix, add `apt-get install -y libgomp1`
   afterwards.** `llama-server` links `libgomp.so.1`, a minimal Ubuntu cloud
   image does not carry it, and both inference sidecars then die at exec with
   status=127 — while the installer still exits 0 and the health check only
   warns. Check `systemctl is-active virtues-embed` (and `virtues-rerank` on
   releases that still installed it) rather than
   trusting the install log.
6. `/var/lib/virtues/virtues.env` (NOT `/etc/virtues/env` — see above):
   `VIRTUES_PUBLIC_URL` + `VIRTUES_REVIEW_PAIR_CODE`. Draw the code randomly
   per round (`shuf -i 100000-999999 -n 1`, on the box, so it never rides in as
   a command parameter) and record it in the private ops note — never in this
   repo, a commit message, or an issue. Then `systemctl restart virtues`.
6a. `VIRTUES_TRUSTED_PROXY=1` in the same file. **Not optional on this box** —
   without it the pair-code rate limit does not run at all behind Caddy, which
   is the whole safety argument for a 6-digit code on a public origin. See
   [Why the env gate is the whole safety story](#why-the-env-gate-is-the-whole-safety-story).
   Prove it after the restart: eleven bad codes in a row must give ten 401s and
   then a 429.
7. **Seed it.** There are two demo seeds, each with its own re-anchor pass —
   the SQL that walks the seeded life forward so its anchor day lands on today,
   because Home asks for the literal current date and has no fallback to the
   newest day holding data. **Exactly one re-anchor may ever run on a box.** The
   two read their anchor from different days — `demo_reanchor.sql` from the day
   with the most location points, `demo3y/99_reanchor.sql` from the newest day
   holding a `p3y_` event — and each moves rows the other reads, so run
   together they undo each other on every pass and the life never rests on
   today.

   - **The base seed**, `virtues seed`: `demo_day.sql` (one richly
     instrumented day), the 12-week `demo_narrative.sql`, `demo_bookmarks.sql`,
     then `demo_reanchor.sql`, compiled into the binary and run by
     `seed_demo_data` (`virtues-core/src/seeding/demo_seed.rs`). **On a release
     older than the `morning_baseline` fix this seeds only a third of the data
     and says nothing about it** (see below); until a release carries the fix,
     run the seed files from a current checkout by hand, ending with
     `demo_reanchor.sql`. Check it: `select occurred_at::date from
     data_location_point group by 1 order by count(*) desc limit 1` should
     return today's date.
   - **demo3y**, three years of raw streams, derived days and events, chats,
     pages, projects and assistant memories — the set that populates Chapters,
     Years and Lifeline, which twelve weeks cannot. It is not in any release:
     `virtues-core/seeds/demo3y/` is gitignored and generated from a checkout
     by `python3 tools/gen-demo-seed.py --check-db <db>` (see
     [seeds/README.md](../../virtues-core/seeds/README.md)). Copy the
     directory to the box and run `DB=virtues sh run.sh` as `postgres`; it
     loads `01_entities` through `05_content` and ends in its own
     `99_reanchor.sql`. **Load it on a box without the base seed, and never
     run `virtues seed` on that box afterwards.** The two sets write the same
     `day_<date>` ids for the twelve weeks they overlap, so in either order
     the second set's events attach, at their original dates, to day rows the
     first set's re-anchor has already moved months away. Check it: `select
     max(d.date) from wiki_days d where exists (select 1 from wiki_events e
     where e.day_id = d.id and e.id like 'p3y!_%' escape '!')` should return
     today's date.
7a. **Install the timers for whichever seed the box carries.** A re-anchor only
   runs when something runs it, so a box seeded once ages a day at a time and a
   reviewer opening the app two weeks after submission meets the empty Home the
   pass exists to prevent — a review cycle is measured in weeks, and a
   rejection round adds more. Both re-anchor files read their anchor out of the
   data, so hourly is free: a run with nothing to do is one SELECT. Each
   re-anchor timer is `OnCalendar=hourly` with `Persistent=true` and
   `OnBootSec=2min` — persistent and on-boot because a box that was down for a
   reboot or a rebuild must catch up when it comes back. Each unit is a
   `Type=oneshot` with `User=postgres` running
   `psql -v ON_ERROR_STOP=1 -d virtues -f <file>`.

   **A box carrying demo3y** (the current demo box) gets two timers, and the
   old one stays off:

   - `virtues-demo3y-reanchor.timer` runs `99_reanchor.sql`, hourly as above.
   - `virtues-demo3y-reset.timer` runs nightly and its service runs **two**
     files in order: `98_reset.sql`, then `04_creation.sql`. The box is shared
     — one reviewer or visitor after another — and everything anyone types on
     it is otherwise visible to the next. `98_reset.sql` deletes what is not
     seeded rather than wiping and re-inserting, because a re-insert would put
     the seeded chats back at their absolute dates three years ago and the
     re-anchor would compute a zero shift and leave them there. It keeps the
     `chat_p3y_` / `page_p3y_` / `p3y_nb_` rows, Getting Started and the
     narrative interview, and clears visitor chats, pages, projects,
     marginalia, pins and drive files. Its table list is an allowlist that
     never names auth, so paired devices and the review pair code survive a
     night mid-round. The second file is there for `app_assistant_memories`
     alone: its ids carry no seed prefix, so the reset clears it wholesale and
     `04_creation.sql` puts the seeded memories back while no-opping on the
     tables whose ids it writes through `ON CONFLICT DO NOTHING`. It does not
     no-op on `wiki_notes`: those ids are generated, nothing conflicts, and
     each run adds the seeded notes again.
   - **`virtues-demo-reanchor.timer` stays disabled and masked** for as long
     as demo3y is on the box. Disabling alone is not enough: it has already been
     re-enabled once on a box carrying demo3y. Step 7a installs both unit files
     into `/etc/systemd/system`, where `systemctl mask` refuses ("already
     exists"), so move them out first:
     `systemctl disable --now virtues-demo-reanchor.timer`, move
     `/etc/systemd/system/virtues-demo-reanchor.{timer,service}` aside, then
     `systemctl daemon-reload && systemctl mask virtues-demo-reanchor.timer
     virtues-demo-reanchor.service`. Both should then report `masked`.
     When both run, the journal shows the two re-anchors moving the life in
     opposite directions, a few days each way, every hour. After any change to
     the box's units, `systemctl list-timers 'virtues-demo*'` should list the
     two demo3y timers and nothing else.

   **A box carrying only the base seed** gets one timer:
   `virtues-demo-reanchor.timer`, running `demo_reanchor.sql` (copied to
   `/usr/local/share/virtues/demo-reanchor.sql`), hourly as above. It has no
   reset; the base seed has no visitor-work cleanup.

   The residual edge, worth knowing rather than fixing: the box anchors on its
   own `current_date` (UTC here) while Home asks for the *browser's* today. A
   reviewer in Pacific time after 5pm is a calendar day behind the box, so they
   land on the day before the anchor day. On the base seed that day still
   carries `wiki_events`, just no raw streams; on demo3y every day carries
   streams, so the cost is only that the reviewer's first day is yesterday's.
   Every other direction lines up.
8. **Give the server a funded api key.** NOT the old "subscribe the account"
   step — that one is genuinely dead, see below — but the server still needs to
   be able to pay for inference, and this is easy to miss now that the two are
   decoupled. The app is chat-first: a reviewer pairs, lands in a chat, types a
   question, and without a key gets a red card reading "Connection failed: no
   virtues_api key — link a subscription first" on every message, with a Retry
   that re-fails. There is no local chat model and the BYO-key screen is
   unreachable from a phone, so the app is simply dead at that point.

   `ensure_bearer` checks `VIRTUES_API_KEY` from the environment before it
   reads the credential vault, so the whole link flow can be skipped:
   generate a 64-hex-char key, insert `(sha256(key), <account_id>, box_id)`
   into **virtues-api**'s `device_keys` — that is the table `bearer_auth`
   actually resolves against; atlas's `box_key` is a mirror and plays no part —
   then put the raw key in the env file and restart. Give `box_id` a real value
   like `review-demo`: a NULL there is retired by the next labeled registration
   on that account. Revoking is a `DELETE` of that one row, and the prepaid
   balance is the damage ceiling.

   Reachability needs no subscription: `relay::DEFAULT_RELAY_URL` is compiled
   into the box, and atlas's relay config (`routes/relay.rs`) carries no
   subscription requirement. A review box needs no atlas account, no link, and no
   card.
9. Confirm `REVIEW PAIR CODE ACTIVE` in the boot log — that is the proof the
   row installed. A missing env var fails silently and looks like success.
10. Test-pair a real phone **over cellular**, not Wi-Fi. Wi-Fi would pass via
    the LAN path and prove nothing about the reviewer's experience.

Models: chat routes to `virtues-api`, so no local LLM is needed. Embeddings and
the reranker do run locally, CPU-only, and slowness is acceptable.

## Before every submission

- **The demo server must run a release at least as new as the app being
  submitted.** Phones update themselves and servers only upgrade when someone
  runs `sudo virtues upgrade`, so the app is the side that runs ahead, and a
  request field it stopped sending can fail every message against an older
  server. Send the app's exact wire shape to the demo box before submitting.
- **Check the claims in this file against the running server.** The first
  bring-up found four of them false, three of which presented as success:
  [review-access-first-run.md](../record/review-access-first-run.md).

## Between review rounds

- Wipe the box and re-seed (`virtues reset --yes`, then provisioning steps
  6–7a; the review code reinstalls itself from the env file on the next start).
  The reviewer's own device data — health, location, contacts, ambient audio —
  syncs onto this box once paired, and it should not persist or bleed into the
  next round; the nightly demo3y reset clears visitor chats and pages but not
  synced device data. Each re-anchor file is idempotent and walks its own seed
  forward to today, but never re-age a demo3y box by re-running `virtues seed`:
  that runs `demo_reanchor.sql` against it, which is the same fight step 7a
  masks the old timer to prevent.
- **Leave it running.** A VPS bills the same stopped, the seed data and the
  review-code row live on its disk, and its fixed address keeps DNS valid.
- **Moving it** to another machine: install the same version, then carry the
  database dump and `/var/lib/virtues/virtues.env` unchanged. The box's iroh
  identity is a row in `box_secrets` sealed with `VIRTUES_ENCRYPTION_KEY`, so
  the two together keep the reviewer's pairing; the `virtues-NNNN` name on
  `/api/box/identity` comes from `/etc/machine-id` and changes with the
  machine, which nothing pairs against. Copy Caddy's certificate directory
  too, and the new box serves both hostnames the moment DNS points at it.
- Revoke by clearing the env var and restarting, which retires the row. Rotating
  the var to a new code does the same and installs the replacement.
- Watch `paired_from_ip` in the pairing audit log for anything unexpected.

## App Review notes

Give them the URL and the code, and say the app requires a paired server with
the demo instance standing in for one. Reviewers do not SSH anywhere or run
`virtues pair` — they type an address and six digits.

**The bare hostname works now — and it did not, for one whole rejection.**
`normalize_server` (`apps/web/plugins/reach/src/lib.rs`) used to turn any
scheme-less name into `http://<host>:8000` — right for `virtues.local` or a
LAN IP, wrong for every public demo server, whose security group opens only
80/443. On 2026-09-05 App Review (1.2.16, iPad) typed exactly
`demo-<rand>.virtues.ch`, as the field's own placeholder teaches, and got
`POST http://demo-<rand>.virtues.ch:8000/api/pair/consume: … operation timed
out` in red under the Pair button. Guideline 2.1 rejection, screenshot
attached. The demo box saw nothing: no device row, no log line, because the
request never reached port 443.

Since that day a dotted public hostname normalizes to `https://<host>`; LAN
names (single label, `localhost`, `.local`/`.lan`/`.home`/`.internal`, IPv4
literal) keep `http://…:8000`; a scheme or an explicit port still passes
through as typed. Unit tests pin the reviewer's exact input. Writing
`https://` into the notes remains harmless and is still the cautious form for
any reviewer on a build older than that fix.

Refused pair attempts (bad code → 401, rate limit → 429) now log a WARN with
the client IP, so the next silent rejection can be told apart from one that
never arrived.

**CallKit.** The same rejection carried guideline 5: MIIT requires CallKit
off for apps available in China, and the audio plugin links `CallKit` for one
read-only `CXCallObserver` (`callActive()`: suppress the "recording paused"
nudge while a phone call owns the mic). Resolved by **removing China from the
app's territories in App Store Connect**, not by touching the recorder. The
obvious code substitute — "a notified interruption hold is standing" — was
tried and reverted the same day: the hold is deliberately cleared by
foreground, route change and media-services reset, all of which happen
mid-call, so the nudge would fire during exactly the call it exists to stay
quiet for. `CXCallObserver.calls` is the only signal that answers "is a call
up right now" independently of our session state. Keep it; keep China off
the territory list.

## Shooting App Store screenshots

The demo server doubles as the screenshot rig — synthetic data that looks like
a life, re-anchored nightly so "today" is always full. Drive a simulator
against it rather than a real phone: exact store canvas sizes, no personal data,
and `xcrun simctl status_bar override` gives the clean 9:41 full-battery bar.

Three things cost time on 2026-09-04 and will again:

- **Check the size App Store Connect actually asks for.** It wanted 1284×2778
  (6.5"); the newest Pro Max simulator gives 1320×2868 (6.9"). Different aspect
  ratios, so rescaling is not an option — `simctl create` the matching device.
- **`bind 127.0.0.1:7117: Address already in use`.** Simulator apps share the
  Mac's loopback, so the Mac app's proxy port collides. Launch with
  `SIMCTL_CHILD_VIRTUES_PROXY_PORT=<free port>`. Simulator-only; a real phone
  has its own loopback.
- **The founder's letter will not scroll under injected touches.** Not a bug —
  verified in a browser at phone size that the page scrolls normally. The way
  past it is the app's own loopback proxy: point a browser at
  `http://127.0.0.1:<proxy port>`, which IS the paired device's session, and
  click through there. `skipOnboarding` writes server-side, so the app comes
  back past the letter.

Delete any probe chats off the server before shooting the drawer. Recents shows
them, and "Reply with exactly: PREFLIGHT OK" is not what you want in a store
listing.

## Alternative not taken

An in-app demo mode (canned local data, no box, no pairing) would remove the
hosted-box dependency, survive review rounds without a babysat server, avoid
holding a reviewer's personal data, and double as a pre-purchase try-before-you-buy.
It is real product work, which is why the demo box came first. Worth revisiting
if review rounds become routine.
