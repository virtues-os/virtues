# The review box's first real run

**Written 2026-09-29, about the bring-up of 2026-09-03.** The App Store review
box described in [review-access.md](../build/review-access.md) was launched
2026-07-21 and never actually brought up: Caddy and the binary were installed,
then it was stopped the same day. There was no env file, no systemd unit, and
the `virtues` database had no tables, not even `_sqlx_migrations`. No review
round had exercised the path, and every iOS submission from July to September
went out with review notes pointing at a box that was switched off.

## What broke

Four problems kept that box from working, all fixed in the same change. The
first was the most serious.

- **The pair-code rate limit was not running.** The build doc justified a
  6-digit code on a public origin with "10 attempts per IP per 30 minutes",
  and behind Caddy that limiter never executed: every request looked like
  loopback, and loopback is exempt. Twelve bad codes gave twelve 401s. Fixed
  by `VIRTUES_TRUSTED_PROXY=1` (now a provisioning step) plus a boot-time
  error when a review code is active without it (`server/mod.rs`).
- **`virtues seed` was broken.** `demo_narrative.sql` still inserted
  `wiki_days.morning_baseline`, a column migration 0011 dropped. `raw_sql`
  runs a file as one unit, so the 12-week narrative and the bookmarks failed
  silently and only `demo_day.sql` landed. Every developer who seeded after
  that migration got a third of the data.
- **The bundled inference sidecars could not start.** `llama-server` links
  `libgomp`, which a minimal Ubuntu image does not carry; the installer did
  not install it, and the install still reported success. The installer now
  installs `libgomp1`.
- **The seed's dates were fixed in February.** `virtues-core/seeds/demo_reanchor.sql`
  now moves the instrumented day onto today at seed time.

None of these shows up in a green CI run, and three of them look like
success.

## An app newer than its server

The app and the server can be too far apart to talk. `POST /api/chat`
required `model` through v0.1.5 and validated it against the allowed list;
the change that made it optional (the server resolves the turn's model from
its slot) shipped in v0.1.6. An app built after that omits the field, so every
message failed to deserialize against an older server, including the demo
server, which was on v0.1.5 at the time. Sending the app's exact wire shape
gave a 422 before the upgrade and a real answer after.

Servers upgrade only when someone runs `sudo virtues upgrade`, while phones
update themselves, so the app is usually the newer of the two.

## The failure class

**A security control can exist in the code and work on a stock box, yet never
run on the one deployment shape the doc prescribes.** The rate limit was
tested, reviewed and documented; it never ran behind a reverse proxy.
The rule we adopted: verify a claimed control on the deployment it
protects (eleven bad codes must produce a 429), and treat every line of a
runbook as untested until a bring-up has exercised it.
