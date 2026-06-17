# Onboarding / Setup / Pairing — Audit Findings (2026-06-17)

A point-in-time issues register for the new-user journey: `curl virtues.com/sh | sh` →
`virtues init` → desktop app → `/pair` → `/setup` → `/get-started` → dashboard. Synthesized
from four independent read-only audits (flow/state-machine, product/UX, code quality,
pairing/transport/trust). **This is a findings list, not a plan — no fixes prescribed here.**

Severity: **S0** breaks the primary path · **S1** serious correctness/UX failure ·
**S2** real but bounded · **S3** hygiene/polish.

---

## Resolution log (2026-06-17 remediation push)

Correctness/security/hygiene tiers landed (all compiling; web at pre-existing baseline):

- **A1 ✅** — pairing collapsed to one model (authed mint = `authorized`); dead `confirm`
  handler/route/`pairConfirm` removed; `deny` repurposed to cancel an outstanding token.
- **A2 ⏏️ refuted** — CI (`release-mac.yml`) stages the sidecars; production DMG is fine. Dev-only gap.
- **A3 ✅** — desktop verifies the QR `fpr` + TOFU-pins the server key (keychain), mirroring iOS.
- **B1 ✅** — `setup_complete` uses a positive `REQUIRED_SETUP_STEPS` allow-list; `network` is display-only.
- **B2 ✅** — get-started no longer stops the shared store; `finished` is revivable, not a permanent latch.
- **B3 ✅** — `(onboarding)/+layout.ts` detects redirects via `'status' in error`.
- **C1 + C2 ✅** — `NOTIFY wg_reconcile` on pair/revoke → daemon reconciles in ~1s (was up to 15s,
  racing the client's 6s handshake); daemon is the single writer of kernel `wg0` (direct
  `remove_peer` removed). Also refuted the "needs reboot" / "revoke doesn't cut transport" worst cases.
- **B5 ✅ (substrate)** — server-backed `chat_imported` step (the old client flag never flipped);
  required/informational split. Shared step-ID constants + presentation dedupe → folded into **D2**.
- **E1 ✅** — phantom `*_user_domain` Tauri bridge fns deleted (no callers; domain model is dead).
- **E2 ✅** — forwarder half-open thread leak fixed (`shutdown(Both)` unblocks the parked copy).
- **E3 ✅** — `into_split` contract documented to match reality (write half owns the lifetime).
- **E4 ✅** — `/auth/session` probe parses status + body only (header text can't false-match) + tests.
- **E5 ✅** — handled at the consumer: the collector card's start-watchdog polls `running` instead
  of trusting the Swift installer's exit code.
- **E6 ✅** — `name_handler` logs a refused avahi reload instead of swallowing it (rename still succeeds).

Product tier (Tier 3) landed:

- **D1 ⏸️ descoped** — wow-landing deferred (core-first, per owner). "Finish" still lands in chat.
- **D2 ✅** — killed the under-chat `NextWinsChecklist` + floating `BoxSetupNudge` (both components
  deleted); one dismissible sidebar **"Finish setup (N left)"** entry → `/get-started`. (Shared step-ID
  consts dropped — only one consumer remains after the deletion.)
- **D3 ✅** — "Add your iPhone" + explicit "Android is coming soon" (no silent wall).
- **D4 ✅** — `ServerProvisioning` rewritten to appliance voice; "Contact Support" → "run `virtues doctor`".
- **D5 ✅** — privacy reassurance added at the two high-disclosure consents (Full Disk Access, chat import).
- **D6 ✅** — `DevicePairModal`: raw "Server endpoint" + Copy removed (and dead fetch/state); sentence-cased.
- **D7 ✅** — living-vs-one-time mental model planted at the sources step, not just the final import card.

---

## F. virtues-core whole-crate structural audit (2026-06-17)

Three parallel read-only audits (dead routes, module org, dead-code/over-engineering). Findings only
— **no structural refactor performed** (the big physical regroup is explicitly rejected by
[[project_repo_structure_plan]]; see the bottom note). One confirmed latent bug fixed inline.

### Fixed now
- **F-BUG ✅** — `api/usage.rs:404` generated UUIDs with SQLite-only `randomblob()`/`random()` inside a
  **Postgres** INSERT → the first-insert (non-`ON CONFLICT`) path errored at runtime. Replaced with
  `gen_random_uuid()::text`. Only occurrence in the crate (verified). Contradicted the
  "Postgres migration complete, zero SQLite remnants" record.

### High-signal findings (recommend acting on — plan-aligned)
- **F1 · Response-helper duplication + status drift.** `server/api.rs::error_response` and
  `error.rs::Error::http_status()` disagree (the former omits `Configuration→503`, `Authentication→401`).
  ~50 hand-rolled `(StatusCode, Json(json!({"error":…})))` tuples across pair/settings_byo/setup/devices/…
  re-implement it. **One fix kills it: `impl IntoResponse for Error` + make the `api.rs` helpers `pub`.**
- **F2 · Dead HTTP routes** (registered, no caller in web/iOS/mac/desktop/internal): `/api/seed/*`,
  `/api/metrics/activity`, `/api/usage/check`, `/api/billing/claim`, `/api/search/web`, `/api/storage/objects*`
  (superseded by `/api/drive/*`), `/api/entities/places*` + `/api/places/{autocomplete,details}` (superseded
  by `/api/wiki/place*`), `/api/agents*`, `/api/namespaces*`, `/api/devices/health`, `/api/wiki/day/:date/streams`
  (retired streams concept), and the `wiki/orgs`+`wiki/org/:id` duplicate aliases. **Verify-then-delete.**
- **F3 · Broken/dead *client* calls** (web fetches with no backend route): `/api/changelog`,
  `/api/ontologies/*` (retired registry/ontology), `/api/devices/pending-pairings`, `/api/chats/:id/action*`,
  `/api/mcp/servers*`. Either dead frontend code or a real gap — triage.
- **F4 · Dead symbols.** ~33 unused ID-prefix consts in `ids/mod.rs` (retired narrative/health/location/
  money/source/stream/job domains); `storage/models.rs` `StreamObject`/`StreamTransformCheckpoint` (retired
  streams); a scattering of orphan fns (`cli/restore.rs:274`, `api/namespaces.rs:105/111`,
  `observability/mod.rs:89/97`, etc.); compat-only shells in `api/spaces.rs`.
- **F5 · `api/` vs `virtues_api/` + two `client.rs`.** `api/` = inbound REST; `virtues_api/` = outbound client
  to the remote service; plus a root `client.rs` (crate facade) and `virtues_api/client.rs` (HTTP client).
  Three overloaded "api/client" names → constant misnavigation. **Rename `virtues_api/` (e.g. `*_client`) +
  `virtues_api/client.rs`→`remote.rs`.** Pure naming, no regroup.
- **F6 · No-op cargo features.** `cuda`/`tensorrt` in `Cargo.toml` have zero `#[cfg(feature)]` sites (sidecars
  own GPU selection) — corrects [[project_model_resolver]]'s "cuda behind a cargo feature".

### Structural (bigger; weigh against the locked plan)
- **F7 · HTTP handlers are double-homed.** ~15 of 53 `api/*.rs` files contain axum handlers AND the
  3,350-LOC `server/api.rs` holds handlers for the *same* features (profile, agents, tools, models, personas…),
  split only by comment banners. "Where is X handled?" requires checking both, with no rule. The single biggest
  navigability tax. Fix = pick ONE home for handlers and split `server/api.rs` along its existing banners.
- **F8 · Loose root files** that belong in a module: `action_git_import.rs` (→actions), `net_check.rs`+
  `http_client.rs` (→a net home near `wireguard/`), `inference_report.rs` (→`search/`), `geo.rs` (→util).
- **F9 · Oversized fns** worth extracting: `api/chat.rs` `create_agent_stream` (333 LOC), `chat_handler` (325).
- **F10 · Single-impl traits behind `Arc<dyn>`** — `storage::StorageBackend`, `search::Embedder` — premature
  extensibility (the appliance doctrine rules out a 2nd backend).

### Cleared (look suspicious, are NOT problems)
`action_git_import.rs`, `mcp/`, `dayline/`, `entity_resolution/` are all wired and reachable.
`McpClientManager`/`AppRegistry`/`ServiceSupervisor` own real state. `pair.rs::hash_token` must stay distinct
from the device-bearer HMAC.

### Note vs [[project_repo_structure_plan]]
That plan **rejected** the deep 30-module → ~6-domain physical regroup ("flat, well-named modules are fine").
So F7–F10 (big moves) are in tension with it; the plan's governing rule ("every entry answers what/why at a
glance") *does* endorse the naming/dedup/dead-removal subset — **F1–F6**. Recommendation: act on F1–F6;
treat F7 (handler double-home) as the one structural change worth a dedicated decision; leave F8–F10 as
opportunistic tidy-ups, not a sweep.

---

## A. Showstoppers — the primary path is broken or unverifiable

### A1 · S0 · The web pairing modal can never complete a pairing
`mint_handler` starts every token `pending` and requires the minting device to call
`/api/pair/confirm/:id` to authorize ([pair.rs:200](virtues-core/src/api/pair.rs#L200)); `consume_handler`
only claims `status='authorized'` tokens ([pair.rs:653](virtues-core/src/api/pair.rs#L653)). But
`DevicePairModal` (`initiateQRPairing`, `initiateMacPairing`) mints and immediately polls — it
**never calls `pairConfirm`**, and `pairConfirm` ([client.ts:458](apps/web/src/lib/api/client.ts#L458))
has **zero callers** anywhere in the web app. So every QR/Mac pairing via the modal hits an
unauthorized token and spins until the 10-minute timeout. Only the collector path works, because
`mint_collector_handler` self-authorizes ([pair.rs:283](virtues-core/src/api/pair.rs#L283)).
**Phone pairing — the "richest source" — is dead.**

### A2 · ~~S0~~ → S3 (dev-only) · VERIFIED REFUTED for production
`apps/web/src-tauri/binaries/` holds only a 0-byte `.gitkeep`, but **`release-mac.yml` stages both
sidecars correctly** before `tauri build`: it builds `virtues-collector` (Swift, arm64+x86_64) and
`virtues-client` (Rust, both targets), copies them to the triple-named files
(`…-aarch64-apple-darwin`, `…-x86_64-apple-darwin`, `…-universal-apple-darwin`) and `chmod +x`s them
([release-mac.yml:76-101](.github/workflows/release-mac.yml#L76)). So the **production DMG ships the
sidecars** — this is NOT the cause of a real user's failure. Residual (S3): any DMG built *outside*
`release-mac.yml`, and all local dev runs, have no sidecars, so pairing/collector silently fail with
"program not found." **Implication for the live "collector failed to install":** since the user's app
is the CI DMG, the cause is downstream of the sidecar — token/pair-consume or `launchctl bootstrap`
(see E5), not a missing binary.

### A3 · S0 · Desktop never verifies the box's SPKI fingerprint — the trust doctrine is not enforced
The box mints `fpr` into the pair URL specifically to defeat a LAN MITM substituting the WG server
key ([pair.rs:231-236](virtues-core/src/api/pair.rs#L231)). The desktop `parse_pair_url` extracts
only `t` and **silently drops `fpr`** ([pair.rs:210](apps/desktop/src/pair.rs#L210)). The bundle's
`server_public_key` arrives over spoofable HTTP and is fed straight into the tunnel with no
comparison ([tunnel.rs:56](apps/desktop/src/tunnel.rs#L56)). `print_status` computes a fingerprint
*from the bundle's own key* — self-referential, not a check. iOS does this correctly (TOFU + throw
on mismatch, `VirtuesTunnelManager.swift`); **the desktop has no equivalent and no pin store.** On a
hostile/MITM'd LAN at first pair, an attacker who intercepts `/consume` can hand themselves the
bearer. This is the single largest gap vs the locked "trust = SPKI over the WG handshake" doctrine.

---

## B. Flow / state-machine correctness

### B1 · S1 · `setup_complete` couples a flaky network signal into a hard redirect gate
`setup_complete` requires `network` done, where `network = primary_ip().is_some()`
([box_status.rs:421](virtues-core/src/api/box_status.rs#L421)). `(app)/+layout.ts` redirects to
`/setup` whenever `setup_complete === false` ([+layout.ts:36](apps/web/src/routes/(app)/+layout.ts#L36)).
A Wi-Fi blip (the exact case the doctrine plans for) flips `network` false → an already-set-up user
is bounced from the dashboard back into the wizard. The wizard itself treats network as
*non-blocking* and lets the user `finish()` while `setup_complete` is still false — so the "You're
set up" screen and the server actively contradict each other, and the gate re-bounces.

### B2 · S1 · One global `setupStateStore` singleton, driven by multiple components, no ownership
- `/get-started`'s `onDestroy` calls `setupStateStore.stop()`, which clears the interval **and** the
  visibility handler the `(app)` layout believed it owned. Navigating get-started → dashboard
  silently kills the dashboard's checklist/toast polling until a full reload
  ([get-started onDestroy](apps/web/src/routes/(onboarding)/get-started/+page.svelte), [setupState.svelte.ts:71](apps/web/src/lib/stores/setupState.svelte.ts#L71)).
- A `finished` latch ([setupState.svelte.ts:31](apps/web/src/lib/stores/setupState.svelte.ts#L31))
  permanently no-ops `start()` once an all-done state is ever observed; a later new-device pairing or
  step regression never revives the autonomous poll. No reset path.

### B3 · S1 · `(onboarding)/+layout.ts` punts to `/pair` on any transient error
The redirect re-throw checks `error instanceof Response` ([+layout.ts:19](apps/web/src/routes/(onboarding)/+layout.ts#L19)),
but SvelteKit throws a `Redirect`, not a `Response` (the `(app)` layout correctly uses `'status' in error`).
Any transient `/auth/session` hiccup mid-wizard dumps a legitimately-paired user to `/pair`.

### B4 · S1 · Re-pair onto a reinstalled box can load the stale proxy
`probe_box_session` returns `None` (not `Some(false)`) when the proxy is unreachable/starting, and
the launch decision loads the box on `None` ([main.rs:64](apps/web/src-tauri/src/main.rs#L64)). The
disk `is_paired()` stays true across a box reinstall (local bundle/keychain not cleared), so re-pair
correctness depends entirely on `probe_box_session` returning a clean `Some(false)`, which it can't
guarantee mid-transition — the app can load `7117` against a stale bundle/dead bearer.

### B5 · S2 · Step model is duplicated and drifts between server and clients
- The stepper presents **4** conceptual steps; the server `onboarding` vec has **8**
  ([box_status.rs:429](virtues-core/src/api/box_status.rs#L429)), rendered verbatim by
  NextWinsChecklist. So the stepper says "Connect calendar & email" while the backlog shows two rows
  ("Connect a source" + "Sync your living spine"), and `first_device` appears in the backlog but was
  never a stepper step. Counts disagree across surfaces.
- **Chat `import` has no server step at all** — its done-state is a local `imported` flag that
  vanishes on refresh and never reaches the persistent backlog. Skip it once and there's no
  breadcrumb back.
- Step IDs are bare string literals duplicated across `box_status.rs`, `get-started`,
  `NextWinsChecklist`, and the store, with **no shared constant**. Renaming any server ID silently
  makes a step never light up (`?? false`), no error.
- `first_device` / `device_collecting` / `first_phone` overlap: pairing the Mac satisfies two of
  them, showing two green rows for one action.

### B6 · S2 · Pair-consume side-effects are post-commit best-effort with no UI signal
Action fan-out and WG-bundle assembly happen after the device row commits and are recoverable only
via a manual `virtues reconcile` ([pair.rs:842](virtues-core/src/api/pair.rs#L842)). Nothing in the
`device_collecting`/`first_device` derivation distinguishes a "paired but half-wired" device — the
checklist shows the step satisfied while data can never flow.

---

## C. Pairing / transport / trust (beyond A3)

### C1 · ~~S1~~ → S2 · VERIFIED NARROWED · Revoke cuts transport, but with up to ~15s lag
`revoke_credential` sets `status='revoked'` and does **not** itself evict the peer
([credentials.rs:248](virtues-core/src/api/credentials.rs#L248)) — but the `virtues-wireguard`
daemon's 15s reconcile loop calls `rebuild_interface` → `load_all_peers` (which selects
**only `status='active'`**, [peers.rs:44](crates/virtues-wg/src/peers.rs#L44)) → `bring_up`, and
`bring_up`/`configure_interface` applies **`WGDEVICE_F_REPLACE_PEERS`** (full peer-set replace,
verified in defguard 0.9.6 `host.rs:242`). So a revoked credential drops out of `wg0` at the next
reconcile tick. **Revocation does cut transport — within ≤15s, not "until reboot."** Residual issues:
(1) up to ~15s of continued transport after revoke; (2) the device-delete path's *direct*
`remove_peer` ([devices.rs:268](virtues-core/src/api/devices.rs#L268)) is redundant with — and can
race — the reconcile loop, so there are two uncoordinated writers to kernel state.

### C2 · ~~S1~~ → S2 · VERIFIED NARROWED · There IS a reconciler; the real issue is a first-pair race
The `virtues-wireguard` daemon runs a **15-second poll loop** that calls `rebuild_interface`
([virtues-wireguard.rs:33-55](crates/virtues-wg/src/bin/virtues-wireguard.rs#L33)), rebuilding `wg0`
from the DB. So a freshly-paired peer **is installed within ~15s — no reboot needed** (the "needs a
reboot" worst case is refuted). The real residual: the desktop client dials WG with only a **6s
handshake budget** ([tunnel.rs:28](apps/desktop/src/tunnel.rs#L28)) while the box may take up to ~15s
to reconcile the new peer into the kernel. On first pair the handshake can time out before the peer
exists → spurious BYO fallback (and, per C4, the fallback is one-shot, so WG is never retried until
the daemon restarts). Plus the `add_peer`/`remove_peer`-direct vs `rebuild_interface` split (C1) means
kernel state has no single owner.

### C3 · S1 · The Tauri install path forces `--upstream`, undercutting the WG-first default
`install_helpers` always bakes `up --upstream <paired-address>` into the LaunchAgent
([main.rs:176](apps/web/src-tauri/src/main.rs#L176), [install.rs:167](apps/desktop/src/install.rs#L167)),
its own docstring calling it "the direct path… No WireGuard tunnel." `run_up` does still try WG
first, but on the 6s-handshake failure it falls back to sending the bearer over whatever origin the
user paired against (LAN/Tailscale) **in cleartext** — combined with A3 (no SPKI check), the average
desktop "happy path" is closer to BYO-with-unverified-identity than SPKI-over-WG. Doc/behavior drift
with a real security implication.

### C4 · S2 · WG-first → BYO fallback is one-shot at startup
The transport decision is made once; a mid-session WG death (prefix rotation, endpoint change) has
no in-process re-evaluation — recovery depends entirely on the LaunchAgent's `KeepAlive`
([tunnel.rs:28](apps/desktop/src/tunnel.rs#L28)). A foreground `virtues-client up` has no recovery.
A slow-but-reachable box that handshakes in >6s is misclassified as unreachable and dropped to BYO.

### C5 · S2 · Single-box assumptions: `default-box` keychain + fixed port 7117
`ACCOUNT_BUNDLE = "default-box"` ([keychain.rs:20](apps/desktop/src/keychain.rs#L20)) — pairing a
second box silently overwrites the first. The proxy binds a fixed `7117` with **no retry on bind
failure** ([proxy.rs:121](apps/desktop/src/proxy.rs#L121)), so two boxes or a stale daemon can't
coexist. `is_paired()` keys on file existence and can't tell which box.

### C6 · S3 · Stale CA / GotaTun residue in comments
The CA was removed, but comments still describe it ([pair.rs:909](virtues-core/src/api/pair.rs#L909),
[pairing.rs:6](virtues-core/src/wireguard/pairing.rs#L6), `box_status.rs` `ReadinessGates.identity`),
and a CLI help line still says "via GotaTun" ([main.rs:19](apps/desktop/src/main.rs#L19)). Misleading
to the next reader (who may go looking for CA verification that should instead be the missing SPKI
check from A3). No live punch/coordinator/gotatun code remains — verified.

---

## D. Product / UX / information architecture

### D1 · S0(product) · The "wow" is never delivered in onboarding
The thesis is "a calendar/biography of your life," and those surfaces exist
(`wiki/*`, DayPage, etc.) — but install → setup → get-started → dashboard never points to them.
`/get-started` "Finish" calls `goto("/")`, landing on a chat view whose empty state is a collapsed
"Next wins" accordion. **The user finishes setup and is shown a to-do list, not their life.** The
flow dead-ends in a checklist.

### D2 · S1(product) · Three competing "what to do next" surfaces, from different endpoints
`BoxSetupNudge` (floating banner, polls `/api/box/health`), `NextWinsChecklist` (accordion, polls
`/api/setup/state`), and the `/get-started` stepper all coexist. The first two derive from different
endpoints and can disagree; "Add your phone" appears in two visual languages routing to two
different places. `BoxSetupNudge` dismissal persists in `sessionStorage` while `NextWins` uses
server-side prefs — the same "stop nagging me" gesture behaves differently per surface. And once
`NextWins` is dismissed there's no "resume setup" entry point at all.

### D3 · S1(product) · "Your phone" promises cross-platform, delivers iPhone-only
The richest-source step is titled "Add your iPhone" / "Pair iPhone" with `deviceType="ios"`
hardcoded and no Android path or "coming soon" — a hard wall for a large share of users, presented
as if universal.

### D4 · S1(product) · SaaS voice + alarming error states contradict the self-hosted thesis
`ServerProvisioning` is a full-screen blocking modal with cloud language ("Waking up your server
from cold storage," "Restoring your data") that on timeout shows a red icon and "Contact Support /
support@virtues.com" — wrong for a self-hosted appliance whose box is just slow to migrate.
Transient box-unreachability during setup is painted as a red error
([setup +page:211](apps/web/src/routes/(onboarding)/setup/+page.svelte#L211)) when it's the expected
case on a flaky LAN.

### D5 · S1(product) · Trust messaging peaks at the billing step, absent at the consent moments
The account step has the best sovereignty copy in the product ("Your data lives on this box — never
our cloud") — but it appears only at the $20/mo step and never returns. The Full Disk Access grant
(reading Messages — the highest-anxiety consent) gets one line; chat-history upload gets zero privacy
reassurance. Trust should peak where data is exposed, not where money is.

### D6 · S2(product) · Tone splits the journey into three visibly different products
Literary CLI hero ("the person you ought to become") → warm sentence-case web setup → generic
Title-Case SaaS in `ServerProvisioning`/`BoxSetupNudge`/`DevicePairModal`. The modal even exposes
raw infra mid-pairing ("Server endpoint: http://…/api" + Copy button) — a manual-config leftover that
clashes with the "labels, not addresses" doctrine. Seams land exactly at the handoffs.

### D7 · S2(product) · The "Living vs one-time" teaching arrives too late
The two-bucket Living/one-time explainer lives only inside the chat-import step — the last, most-
skippable one — after Calendar/Email (the "Living" sources) were already connected without it. Most
users skip the step where the mental model is taught.

### D8 · S3(product) · Copy/command drift
Two near-identical terminal states ("You're set up" vs "Your box is set up"); the `/pair` page tells
users to run `virtues pair` while doctrine renamed the verb; `$20/mo` hardcoded in UI copy with no
shared source against the locked economic model.

---

## E. Technical hygiene (correctness-adjacent)

### E1 · S2 · `bridge.ts` calls three Tauri commands that don't exist
`get_user_domain` / `set_user_domain` / `clear_user_domain` ([bridge.ts:42-73](apps/web/src/lib/tauri/bridge.ts#L42))
are not registered in `invoke_handler!` and have no `#[tauri::command]` anywhere — every call rejects
"command not found" and the errors are swallowed (returns `null`/`false`). Dead leftover or a missing
backend half; either way silently broken.

### E2 · S2 · Loopback-forwarder copy threads can block forever and leak
Each bridged connection spawns two `spawn_blocking` `std::io::copy` loops over a *blocking* inbound
socket with **no read timeout** and no observation of the shutdown channel
([tunnel.rs:153](apps/desktop/src/tunnel.rs#L153)). An idle/half-open connection blocks the `up`
thread indefinitely; under churn this exhausts tokio's bounded blocking pool.

### E3 · S2 · `TunnelStream::into_split()` is unsound for read-half-only consumers
Close-on-drop moved entirely to the write half; the read half has no `Drop`
([tunnel.rs:571](crates/virtues-tunnel/src/tunnel.rs#L571)). Dropping only the write half (legal for
split halves) fires `Close` while the reader is live → premature EOF. The desktop bridge happens to
join both halves so it's safe *there*, but the API is unsound generally and the safety comment
doesn't capture the constraint.

### E4 · S2 · `probe_box_session` parses HTTP by substring + ~6s of blocking work
Decides pairing validity via `buf.contains("\"user\":null")` vs `"device_id"`
([main.rs:64](apps/web/src-tauri/src/main.rs#L64)) — brittle to chunked/compressed/partial reads and
to nested data containing the substring; a misread bounces a valid device to `pair.html` or dead-ends
a revoked one on the box `/pair`. The 5× retry adds ~6s of blocking work before the window builds.

### E5 · S2 · Collector "install succeeded" on exit 0 even when `launchctl bootstrap` failed
`InstallCommand.swift` prints bootstrap failures as warnings and exits 0; `install_collector` keys
success purely on exit status. (The Svelte card now polls `running` for 12s, which compensates — but
the Rust/Swift contract "exit 0 ≠ installed" is still wrong for any other caller.)

### E6 · S2 · `name_handler` claims success when the avahi reload is refused
After `hostnamectl`, the `sudo -n systemctl reload-or-restart avahi-daemon` result is discarded
([setup.rs:257](virtues-core/src/api/setup.rs#L257)); if the sudoers rule only whitelists
`hostnamectl`, the reload silently fails yet the handler returns `200 {ok:true}` — `.local` doesn't
update but success is reported.

### E7 · S3 · Revoke targets the local proxy, which is usually down at revoke time
`revoke()` builds `http://localhost:7117/...` ([main.rs:391](apps/desktop/src/main.rs#L391)); if the
user revokes *because* pairing is broken, the proxy isn't running, the DELETE fails, and the code
clears local creds while the server row persists (compounds C1).

### E8 · S3 · Scattered swallowed errors / panics-on-poison on hot paths
`uninstall_helpers` discards the command result and returns `Ok`; `Tunnel::send_cmd`/`status` use
`.expect()` on mutex locks in the proxy hot path (poison → crash instead of degrade);
`DevicePairModal` polling errors are only `console.error`'d (silent until timeout).

---

## Cross-cutting root causes
1. **Multiple sources of truth with no ownership protocol** — Tauri disk `is_paired` vs box session
   vs `setup_complete` vs local component flags vs a global store singleton (drives A1-adjacent
   confusion, B1, B2, B4, D2).
2. **`setup_complete` overloaded** — couples a non-user-drivable network signal into a hard gate (B1).
3. **App-layer bearer carries all trust; SPKI is decorative on desktop** (A3, C1-C3).
4. **The step model is authored twice** (server vec + stepper) and never reconciled (B5, D2).
5. **The flow optimizes for "configure the box," never for "show me my life"** (D1).

## Confirmed-correct (do not re-flag)
Atomic 0600 daemon-bundle write; RFC-correct proxy hop-by-hop/upgrade handling; `net_check` FFI
null-safety; `Tunnel` Drop→Shutdown→join; iOS SPKI TOFU; the account-step sovereignty copy;
RemoteAccessExplainer isolating all BYO/overlay language to the moment of intent; no live
punch/coordinator/gotatun code remains.
