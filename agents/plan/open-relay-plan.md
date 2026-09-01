# The open relay — reachability stops being a tier

> **STATUS 2026-08-31: direction agreed, not yet built.** Ideation folded in
> same day after inspecting the live relay (see §What actually runs). Items
> 6–7 below are independent of the relay decision and can ship first.

## The incident (failure class)

A beta owner on ordinary IPv4 home wifi set up her box and **skipped the
subscription** at the airlock's escape hatch. The skip copy promises "LAN-only
by choice." What she actually got: a Mac that could *discover* the box (mDNS
got through) but never connect to it, an airlock code screen asking for six
digits that exist nowhere (§Work 7), and — after a 3-second reset and a change
of heart about subscribing — a checkout screen that sat on "waiting for the
payment to land" with no timeout until she gave up. Nothing on any screen
could tell her what was wrong, because nothing was wrong *by the product's own
story*: every screen behaved as written. The story is what's broken.

## The mechanism

In the iroh model the relay is not a fallback data path — it is the
**rendezvous**. Two peers behind IPv4 NAT can only hole-punch after
exchanging address candidates, and that exchange happens through the relay.
So relay access gates three things, not one:

1. remote reachability (the obvious one),
2. NAT hole-punching coordination, and
3. on any home network where local discovery fails — AP/client isolation,
   mesh nodes on different segments, filtered multicast — effectively **any
   connection at all**.

The box only homes on the relay when atlas hands it a `relay_url` **at
claim/link**, authenticated by the box's api_key
(`virtues-core/src/virtues_api/relay.rs`); no account → no api_key → no relay
→ `RelayMode::Disabled` (`crates/virtues-iroh/src/endpoint.rs`). And the
airlock writes the account grant only when `state.entitled` is true
(`renderGrantStep`, `apps/web/src-tauri/ui/connect.html`), so
skipped-or-unpaid → no link → no relay.

The net effect: the free path does not degrade to "LAN-only." It degrades to
"works only if your router happens to be friendly" — a property of the
owner's AP settings that neither she nor the app controls, inspects, or can
name in an error message. **A paywall on the connectivity substrate is
indistinguishable from a bug**, and the owner experiences it as one.

## What actually runs (verified on the relay host, 2026-08-31)

The deployed relay is **stock n0 `iroh-relay`** on one hostname
(`relay.virtues.ch`, one LetsEncrypt cert) with admission enforced by an
`[access.http]` hook: **every client connection triggers a real-time callout
to `atlas.virtues.com/relay/authorize` carrying the client's EndpointId**,
answered from account data.

Two consequences:

- **The `<boxhash>.virtues.ch` per-box SNI + HMAC control plane is legacy.**
  It does not exist in the running system; the box-side `RelayConfig` is
  literally `{ relay_url }` — no token, no per-box anything. The comment in
  `server/api.rs` ("atlas mints this box's per-SNI token") is stale prose from
  the pre-iroh design and must be corrected. Per-box SNI would also have been
  an anti-blind leak (box identity in plaintext SNI to every on-path
  observer); one hostname for everyone is blinder and less infra.
- **The privacy claim is currently false in a specific way.** The relay is
  "blind" to payload (e2e-encrypted QUIC it cannot read) but its front door
  reports every connection to atlas, linked to accounts, in real time. Atlas
  observes which endpoints connect and when. Deleting the access hook deletes
  that linkage entirely — a concrete, statable privacy win.

So the relay-side work is **mostly deletion**, the best kind.

## The decision

The relay admits **any endpoint**. Specifically:

- **Rendezvous is free and unlimited, forever.** Candidate exchange and
  hole-punch assist are a few packets; they are what make pairing and reach
  *work*, and they are never metered.
- **Relayed payload gets one flat per-connection throughput cap, the same
  for everyone.** No tiers. Tiered bandwidth would require the relay to know
  who subscribes, reinstating the exact account↔connection linkage being
  deleted. The heaviest legitimate stream today is iOS audio (~1–2 MB/min);
  a cap set an order of magnitude above real workloads is invisible to every
  customer while making freeload-as-a-pipe boring. The cap's job is to bound
  abuse, not to meter customers. Revisit only if measured traffic (§Work 5)
  argues; the fallback design if tiering ever becomes necessary is an
  atlas-signed offline voucher (`{EndpointId, tier, expiry}`, verified at the
  relay with no callout) — never the authorize hook.
- **Abuse posture, stated plainly:** an iroh relay is not a proxy. It can
  only carry encrypted traffic between two endpoints that both chose to talk
  to each other; it cannot browse the web, send email, or reach any
  third-party server. The worst possible abuse is two strangers using it as
  a free pipe for their own traffic — a bandwidth problem the cap bounds,
  never a content or liability problem. Abuse handling therefore never
  requires knowing who anyone is. (Per-EndpointId limits alone are weak —
  keys are free to mint — so limits are per-IP *and* per-connection, sized
  to forgive CGNAT.)
- **The subscription moves to where it can honestly live: hosted AI and
  services.** Reachability — like the record itself — is part of what the
  owner *owns*, not what they rent. Sign-in's honest pitch becomes: hosted
  AI, the wallet, and your box attached to your account. Connectivity is
  simply not for sale.
- **The default relay URL ships in the box binary, said out loud, trivially
  off.** Baked default for appliance *and* DIY (resolve order: stored → env →
  shipped default): everything works out of the box, and being reachable is
  the product. The costs are accepted consciously: a small bandwidth subsidy
  for DIY boxes, and the "self-hosted box connects to vendor infra by
  default" optics — answered by naming the relay in Settings, one honest
  sentence in the manual, and a real off switch. Rejected alternatives:
  DIY-defaults-off (breaks out-of-the-box reach for the primary paradigm),
  and defaulting DIY to n0's public relays (moves connection metadata to a
  third party the code deliberately excluded).

## The work

1. **Relay: delete admission, add limits.** ✔ **APPLIED 2026-08-31**: backed
   up config, removed `[access.http]` (serde-defaults to `everyone`), added
   `[limits]` (20 conns/sec accept, burst 100) and `[limits.client.rx]`
   (1 MB/s steady, 16 MB burst — stock iroh-relay 1.0.x has no per-IP knob,
   so the per-client bucket is the whole abuse story), pinned metrics to
   `127.0.0.1:9090` (ufw already blocked it; now unexposable). Verified:
   service active, HTTPS 200, six fleet endpoints reconnected within
   seconds. Still to do: retire `atlas.virtues.com/relay/authorize` (dead —
   nothing calls it), fix the stale per-SNI comment in `server/api.rs`.
1c. ✔ **BUILT 2026-09-01 — the registry the gate fed is gone too.** An audit
   found `/iroh/register`, `iroh_endpoints`, and the box's `report_endpoints`
   all still live and still running every reconcile tick, a day after the
   callout they existed for was deleted: a reconcile-refreshed inventory of
   every box and every paired device, keyed to a billing account, read by
   nothing. Deleted on both sides (migration `0018_drop_iroh_endpoints.sql`);
   older boxes that still POST get a 404 and carry on, because the call was
   always best-effort. Stale references fixed with it: the box comment
   justifying register-before-bind by a race that no longer has a racer,
   `reconcile()`'s four-leg contract (now three), `entitlement.md` §7 listing
   both deleted endpoints as live behind a link into `agents/archive/`, and
   the relay unit file still naming the retired bearer.
   **The rule this earns: when admission logic is deleted, its registry goes
   in the same change.** Deleting a gate and keeping its data is strictly
   worse than keeping both or dropping both — the liability stays and the
   justification leaves.
1b. ✔ **BUILT 2026-08-31 (not yet deployed)** — decoupling chosen over the
   free-Stripe-customer trick (Stripe must learn only about people who pay
   it). Migration `0017_accounts_decouple.sql`: `accounts` table (account_id
   PK, email UNIQUE, stripe nullable-UNIQUE), backfill prefers the
   active-sub customer among duplicate emails, `box_key.account_id` NOT NULL
   with rotation scope moved to the account, orphan keys retired.
   `ensure_account(email)` mints identity at sign-in; grant/approve/
   redemption lose their 402s; `/relay/config` + `/iroh/register` resolve
   the account with no sub check; `/relay/authorize` deleted with its
   bearer-secret config. Entitlement still guards checkout, top-ups, and the
   billing portal — the money. Compile clean, unit tests green, migration
   chain + seeded rehearsal pass (dup-email collapse, wallet identity
   preserved, ghost keys dropped). Original analysis follows:
   **Atlas: ungate linking server-side — the three remaining locks**
   (discovered auditing `services/virtues-atlas`): `/relay/config` requires
   an *active subscription* before a box may learn the relay URL
   (`routes/relay.rs` → `resolve_active_customer`); `/init/grant` and
   `/init/approve` return 402 for unpaid accounts; and redemption re-checks
   entitlement (`redeem_granted_link`). Beneath all three: **atlas's account
   identity is keyed on Stripe** — `customers.stripe_customer_id` is the
   primary key, `account_id` and `box_keys` hang off it — so a
   never-subscribed sign-in has no account row to attach a box to.
   Proposed fix: at sign-in (or lazily at first grant), create a **Stripe
   Customer with no subscription** (free objects) → `customers` row →
   `account_id` exists; drop the entitlement checks from grant/approve/
   redemption/relay-config (keep api_key/session auth); entitlement checks
   remain only on spend paths (hosted-AI wallet in virtues-api, where the
   money actually is). Server-side deploy — benefits every existing app and
   box instantly, no client release needed for the server half.
2. ✔ **BUILT 2026-08-31 (`6c56b6ce`, on wave; ships in the next box
   release)** — baked `DEFAULT_RELAY_URL` as the last resolve step, gated on
   the box-install marker (dev checkouts stay relay-less); explicit off word
   (`VIRTUES_RELAY_URL=off` or the stored config) so empty-env ≠ disabled;
   Settings → Network gains the Reach reading + toggle (`/api/network/relay`,
   rebinds in-process); the manual's reach page drops the now-false
   subscription-gate claim. Original: **bake the default relay URL** — `resolve_relay_url` gains a shipped
   default after the stored-config and env steps, so a never-linked box homes
   on the relay from first boot instead of `RelayMode::Disabled`. Surface the
   active relay in Settings with the off switch.
3. **Migration (no breakage window).** Linked 0.1.4 boxes already hold
   `relay_url` and keep working untouched — step 1 is server-side and
   invisible to them, and benefits their devices immediately. Never-linked
   boxes are rescued by step 2 at their next upgrade. Atlas keeps
   `/relay/config` alive (it returns only a URL) for old builds.
4. **Airlock: ungate the grant, delete checkout.** `renderGrantStep` drops
   the `state.entitled` condition — a signed-in owner gets the account link
   regardless of payment. **`renderCheckout` leaves the airlock entirely:**
   setup never mentions money. The subscription is offered where it's honest
   — the first time hosted AI is invoked ("no AI configured — subscribe or
   bring your own") — and lives in Settings. This deletes the screen that
   hung on the beta call (a 3s poll of `/account/session` with no timeout and
   unclear cancel semantics) rather than patching it, and removes setup's
   last dependency on Stripe webhooks being alive. While in there: check
   whether the beta owner's abandoned checkout actually charged.
5. **Telemetry: aggregate-only relay dashboard.** `iroh-relay` exposes
   Prometheus metrics; dashboard at admin.virtues.com or the existing
   Grafana. Blindness rules: aggregates only, unique-endpoints-per-day via a
   sketch (HLL) not stored IDs, no who-talked-to-whom, rate-limit state in
   memory only. The four numbers: concurrent connections, relayed bytes/day,
   unique endpoints/day, and share of sessions relayed vs. hole-punched —
   the last is the product-health metric and the cost forecast in one.
6. **Pairing collapses to one doctrine.** Today four proof narratives coexist
   — BLE codeless session, LAN 6-digit code, `virtues pair` URL, phone
   handoff QR — and they are all the same thing: *a token minted where you
   already have access, carried to the new device*. The typed 6-digit form
   exists for exactly one case (claimed box + zero paired devices + LAN),
   which the incident proved is unreachable without a shell anyway — it is UI
   for a code that cannot exist (`renderCodeEntry` promises "the 6 digits on
   the display right now," but the standing code dies at claim,
   `maintenance/pair_rotator.rs`, and the display deliberately never prints
   one). `virtues pair` mints a token rendered as QR / URL / short code —
   encodings, not mechanisms — the airlock keeps one "I have a code / scan"
   surface, and vouch-from-a-paired-device covers the rest.
7. **The claimed-box code screen stops lying** (interim, shippable now,
   subsumed by 6): for a claimed box, lead with the one true sentence — run
   `virtues pair` on the server — and demote the code field to after the
   owner has a code in hand.
8. **One data transport on clients.** With universal reachability, the app
   data path becomes iroh-only; plain LAN HTTP (`:8000`) remains solely for
   the airlock's discovery probe. One transport to debug — "works at the
   coffee shop" and "works at home" stop being different code paths, which
   is the class of bug the incident was.
9. **Audit the phrase-save moment.** After a reset, re-claiming needs the
   frozen setup phrase (`api/setup_phrase.rs`) and it never returns to the
   screen; the incident nearly hit an owner who hadn't saved it. Make saving
   genuinely unskippable at first claim — and consider offering to write the
   phrase into the owner's password manager (standard credential API), so
   "did you save it" becomes "your Mac saved it."

## What this makes atlas

Accounts, billing, hosted-AI keys, update manifests — nothing else. With
admission gone and checkout out of setup, **setup works fully offline from
the cloud when sign-in is skipped**: an atlas outage can no longer brick
onboarding. That is the self-host promise made structural.

## Verification

- A box that has never seen an account, on a client-isolating AP, is
  reachable from a Mac on the same AP (rendezvous through the open relay).
- The same box is reachable from off-network with zero atlas state.
- A linked 0.1.4-era box's devices still connect after the access hook is
  removed, with no box-side change.
- Limits demonstrably cap a hostile client without affecting a normal one
  (bench: one greedy pipe + one normal session, measure both), and a CGNAT-
  shaped burst (many endpoints, one IP) degrades gracefully.
- The airlock completes setup start-to-finish with atlas unreachable.
- The claimed-box pair path reads correctly with no display attached.
- The dashboard shows the four aggregates and stores no per-endpoint rows.

## Death condition

Delete when the open relay ships and the airlock matches it. What survives:
a record of the decision (open admission + flat cap + what the subscription
gates + the deleted authorize linkage) and manual pages for pairing, remote
access, and the relay setting.
