<!--
  /display — the box's face. The 7" panel on the front of an appliance, and
  any other glass that can show a URL.

  On the box it is rendered by a WebKit kiosk (installer's
  virtues-display.service: cage + WebKit on bare DRM, no X, no desktop) and
  is the only interface a Dragon owner has before any device is paired.

  THE FACE IS A URL. The appliance kiosk is one pre-wired consumer of this
  page, not its owner: a paired browser on any device — an old tablet on a
  stand, a spare monitor, a TV — opens the same route, full-screens it, and
  wears the same face. That is the whole DIY display story, and it costs no
  kiosk stack. The page has two data doors and tries them in order:

    1. /api/display/state — loopback-only, carries the setup phrase.
       Answers only on the box itself; proximity is the authority.
    2. /api/system/display — the authenticated, REDACTED mirror. Answers for
       any PAIRED device. It may say the panel is showing setup words,
       never what they are; the record census sits behind pairing, which is
       the right bar for it.

  A browser that is neither the box nor paired gets an honest sentence, not
  a forever-bootmark.

  It lives in (public) because on the box it draws before a session exists.
  The secrets are protected at the API, not here.

  DESIGN CONSTRAINTS, all measured on real glass:

  * The canvas is 585 x 329 CSS px, not 1920x1080. The panel's EDID claims
    53x30cm (~24") when it is physically 15.5x8.7cm, so the kiosk pins
    devicePixelRatio to 3.28 and the usable layout is roughly a phone in
    landscape. One idea per screen; there is no room for a second.
  * Display-only. The digitizer doesn't work through the cover glass, so
    nothing here is interactive, and the phrase in particular must carry no
    affordance that invites a tap.
  * It runs 24/7 in someone's home: dark, no spinners, no animation loops. The
    one exception is the breathing pip on a live setup session, which is the
    smallest thing that reads as "now" without becoming a spinner.

  STATES, in the order they outrank each other:

    0. button held        → a countdown, because something is about to happen
    0. updating           → the server is going away on purpose
    0. no data disk       → the box booted so that it could say exactly this
    1. unclaimed, factory → get the Mac app, and the four words that let it in
    2. unclaimed, frozen  → reset box: the words are the ones you saved
    3. session live       → the words are spent; who is setting up, instead
    4. claimed            → ambient

  The three at the top are interruptions, not steps. Each is a thing happening
  to the box right now that the ordinary screens would paper over — most
  sharply the last one, where an ambient "REACHABLE · 3 devices syncing" over a
  missing disk would spend the entire reason the OS lives on the eMMC.

  A fifth — the six-digit pair code — was added and then removed the same day
  (2026-08-13), and the round trip is the lesson. It went in because the app
  asked for digits this screen could not show. But the code never had to leave
  the box: pairing is authorized by the SESSION, so the only box that ever
  needed it printed here was one with no session at all — and a box with no
  session is a box waiting for someone to START, which needs the PHRASE.
  (0x85, the RPC that then handed the code to the app, was itself deleted
  2026-08-24; 0x83 is codeless now and the box redeems its own code.) The two secrets wanted the same
  slot at the same moment, and showing the wrong one stopped a live run dead:
  the app asked for four words while the glass showed six digits.

  There is no state where the pair code is the right thing to print. If a
  first device ever has to pair with no Bluetooth at all, `virtues pair` on the
  box is the documented door, and it does not cost this screen a fifth mode.

  There used to be three numbered setup screens (join a network, link an
  account, then a pair code) on a full-bleed light/dark split with a QR. They
  are gone. The app carries all three over one Bluetooth conversation, so the
  panel's only job is to start it — point at the app, and show the phrase that
  proves whoever is typing can SEE this box.

  What that replaced is worth remembering, because the failure was instructive:
  screens 1 and 2 were once a single screen carrying two different secrets at
  once, and the first person shown it read the big code on the right and typed
  it as the wifi password. Splitting them helped; deleting them helped more.
  Every code on this glass existed because two machines could not talk, and now
  they can.

  The QR went with them. It pointed a phone at the download page, and setup is
  a desktop job now — scanning it hands the page to the wrong device. Dropping
  it also let the phrase go to one line, which is what makes it readable from
  across a room while you type it on another machine.
-->
<script lang="ts">
	import { onMount, onDestroy } from "svelte";

	type DisplayState = {
		box_name: string;
		linked: boolean;
		setup_phrase: string | null | undefined;
		phrase_frozen: boolean;
		setup_session: string | null | undefined;
		data_disk_fault: string | null | undefined;
		button_held_secs: number | null | undefined;
		button_hold_target: number;
		claimed: boolean;
		online: boolean;
		connectivity: string;
		wifi_ssid: string | null | undefined;
		devices: number;
		record: Array<{ label: string; count: number }> | undefined;
		record_since: string | null | undefined;
		face:
			| { kind: string; builtin: string; applet_id?: string | null }
			| undefined;
	};

	let state_ = $state<DisplayState | null>(null);
	let unreachable = $state(false);
	let now = $state(new Date());
	let poll: ReturnType<typeof setInterval> | null = null;
	let clock: ReturnType<typeof setInterval> | null = null;

	// THE SERVER IS EXPECTED TO GO AWAY, and only this page can know it.
	//
	// An upgrade stops virtues.service for the length of a flip, a migration
	// and a start. This kiosk is a WebKit page served BY that server, so it
	// dies with it — but the process does not exit (cage and WebKit are fine;
	// it is the page underneath that went), so nothing restarts us and nothing
	// tells us. The panel just kept its last good render and flipped the badge
	// to NO SERVER: at the one moment the box is deliberately down, its own
	// screen said it was broken.
	//
	// The state has to be LATCHED BEFORE the restart, because after it there is
	// no server left to ask. So the update endpoint is polled while the box is
	// still answering, and when it reports an upgrade running we remember that
	// across the outage. The latch expires on its own — a box that never comes
	// back must not claim to be updating forever.
	let updating = $state(false);
	let updatingSince = 0;
	// Generous, and deliberately so: this bounds a LIE, not a wait. Under it we
	// say "back in a minute" while the box is down; over it we go back to
	// saying nothing, which is the honest thing once an update has plainly
	// failed. A staged activation is ~15s and a full download-and-install on a
	// Q6A can run several minutes.
	const UPDATE_LATCH_MS = 10 * 60 * 1000;

	// A live BLE setup session, if there is one. `""` is a real value from the
	// server — a session whose client sent no name — so test for null, not truth.
	const session = $derived(state_?.setup_session ?? null);

	// Which data door answered. `mirror` latches on the first loopback 403 —
	// a peer address does not change under a running page, so there is no
	// reason to knock on the shut door again. `mirrorDenied` means neither
	// door is open: this browser is not the box and not paired.
	let mirror = $state(false);
	let mirrorDenied = $state(false);

	async function refresh() {
		try {
			if (!mirror) {
				const res = await fetch("/api/display/state");
				if (res.status === 403) {
					// Off the box. Not an error — this glass just has to come
					// through the paired door instead.
					mirror = true;
				} else {
					if (!res.ok) throw new Error(String(res.status));
					const next = await res.json();
					// Advance the log line once per poll, only when the box is
					// claimed and there is more than one thing to say.
					if (state_?.claimed && (next.record?.length ?? 0) > 1) logIndex += 1;
					state_ = next;
					// Server came BACK: its in-memory face-token store died
					// with it, so a hung face's queries would fail for up to
					// 45min on the old token. Bump the epoch so the face
					// effect re-mints and reloads now.
					if (unreachable) faceEpoch++;
					unreachable = false;
					// The latch is pollUpdating's to clear, not ours: an
					// answered state poll proves the server is up, NOT that
					// the upgrade is over — clearing here raced the 5s
					// updating poll, and losing that race (server dies inside
					// the window after a clearing refresh) put NO SERVER on
					// the glass during the one outage that is deliberate.
					return;
				}
			}

			// The mirror. Same face, redacted feed: no phrase, no session
			// name, no button — a remote glass renders what the panel shows,
			// never what only the panel may say.
			const res = await fetch("/api/system/display", { cache: "no-store" });
			if (res.status === 401 || res.status === 403) {
				mirrorDenied = true;
				unreachable = false;
				return;
			}
			if (!res.ok) throw new Error(String(res.status));
			const d = await res.json();
			const next: DisplayState = {
				box_name: d.state.box_name,
				linked: true,
				setup_phrase: null,
				phrase_frozen: false,
				setup_session: null,
				data_disk_fault: d.state.data_disk_fault ?? null,
				button_held_secs: null,
				button_hold_target: 3,
				claimed: d.state.claimed,
				online: d.state.online,
				connectivity: d.state.connectivity,
				wifi_ssid: null,
				devices: d.state.devices,
				record: d.state.record,
				record_since: d.state.record_since ?? null,
				face: d.face,
			};
			if (state_?.claimed && (next.record?.length ?? 0) > 1) logIndex += 1;
			state_ = next;
			mirrorDenied = false;
			if (unreachable) faceEpoch++;
			unreachable = false;
			// The mirror carries the upgrade fact inline (the kiosk's separate
			// /api/display/updating door is loopback-only). Same latch, same
			// semantics: remember it now so the outage can be narrated.
			if (d.state.updating) {
				updating = true;
				updatingSince = Date.now();
			} else {
				updating = false;
			}
		} catch {
			// Keep the last good state on screen. A box whose server blipped
			// should not blank the panel — the phrase on it is still valid.
			unreachable = true;
		}
	}

	/// Arm the latch while the box can still be asked.
	///
	/// `/api/system/update` is authed, so this reads the box-local unit state
	/// instead: `virtues upgrade` and `virtues prepare` each run under a named
	/// transient unit (`api::updates::UPGRADE_UNIT`), and the box exposes
	/// whether one is active. Failures are silent by design — this is a
	/// nicety, and a panel that can't reach the update endpoint has bigger news
	/// to render.
	async function pollUpdating() {
		// The mirror carries this fact inline in refresh(); the dedicated
		// door is loopback-only and would 403 forever from here.
		if (mirror) return;
		try {
			const res = await fetch("/api/display/updating", { cache: "no-store" });
			if (!res.ok) return;
			const { active } = await res.json();
			// Sole owner of the latch, in BOTH directions: armed while the
			// unit runs, cleared only when the box itself says the upgrade is
			// over. (Clearing from the state poll raced this one.) During the
			// outage this fetch fails, which changes nothing — surviving the
			// outage armed is the latch's entire job.
			if (active) {
				updating = true;
				updatingSince = Date.now();
			} else {
				updating = false;
			}
		} catch {
			/* the outage itself is the signal; the latch is already set */
		}
	}

	// Seconds the case button has been held, or null between presses.
	//
	// Two sources: the state poll carries it, but at the 30s ambient cadence
	// a 3s hold was only ever SEEN one time in ten — so the dedicated 1s
	// button poll (below) is what actually makes the countdown reliable on a
	// claimed box. Loopback-only, like the updating poll; a mirror glass has
	// no button to narrate.
	let buttonHeld = $state<{ held: number; target: number } | null>(null);
	const held = $derived(buttonHeld?.held ?? state_?.button_held_secs ?? null);
	const remaining = $derived(
		Math.max(
			0,
			(buttonHeld?.target ?? state_?.button_hold_target ?? 3) - (held ?? 0),
		),
	);

	const BUTTON_POLL_MS = 1_000;
	async function pollButton() {
		if (mirror) return;
		try {
			const res = await fetch("/api/display/button", { cache: "no-store" });
			if (!res.ok) return;
			const { held_secs, target } = await res.json();
			buttonHeld =
				held_secs === null || held_secs === undefined
					? null
					: { held: held_secs, target };
		} catch {
			// Server gone — the interruption screens own that story.
			buttonHeld = null;
		}
	}

	// Has the latch gone stale? An update that never finished must stop
	// claiming it is about to.
	//
	// Reads `now` — the 20s clock — rather than calling `Date.now()` directly,
	// or the expiry would never be re-evaluated: nothing else changes while the
	// server is down, so a `$derived` over `Date.now()` would compute once and
	// hold "updating" on the glass forever.
	const updatingNow = $derived(
		updating && unreachable && now.getTime() - updatingSince < UPDATE_LATCH_MS,
	);

	// Two cadences, because the screen has two jobs.
	//
	// SETUP is a conversation: someone is standing in front of the box acting on
	// what it says, and every transition they cause — wifi joined, phrase
	// accepted, device paired — must land while they are still looking. At 30s
	// the panel spent up to half a minute showing words that had already been
	// used, which reads as a fault rather than a wait. Seen on hardware.
	//
	// AMBIENT is furniture. Nothing there changes on a human timescale, it runs
	// 24/7 in someone's home, and the phrase rotates every 15 min with a 5 min
	// overlap — so 30s can never show an expired one.
	const SETUP_POLL_MS = 2_000;
	const AMBIENT_POLL_MS = 30_000;

	function schedulePoll(ms: number) {
		if (poll) clearInterval(poll);
		pollMs = ms;
		poll = setInterval(refresh, ms);
	}

	let pollMs = $state(SETUP_POLL_MS);

	// Re-cadence when the box crosses between setup and ambient — in either
	// direction, since `virtues reset` puts a claimed box back into setup.
	$effect(() => {
		// A claimed box is on the 30s ambient cadence, which cannot see a 3s
		// hold at all — so the ONE screen the button matters on would never
		// draw. Once a hold is seen, drop to the setup cadence until it ends.
		// A denied mirror retries slowly: someone may pair this device and
		// come back, but 2s knocks on a shut door are just noise in the log.
		const want =
			mirrorDenied || (state_?.claimed && held === null)
				? AMBIENT_POLL_MS
				: SETUP_POLL_MS;
		if (want !== pollMs) schedulePoll(want);
	});

	// The update watch runs on its own slow timer rather than inside `refresh`:
	// it must keep firing while the state poll is failing (that is the whole
	// point), and it is the only thing here that needs to run on the ambient
	// screen, which polls state every 30s.
	let updateWatch: ReturnType<typeof setInterval> | null = null;
	let buttonWatch: ReturnType<typeof setInterval> | null = null;
	const UPDATE_POLL_MS = 5_000;

	onMount(() => {
		void refresh();
		void pollUpdating();
		schedulePoll(SETUP_POLL_MS);
		updateWatch = setInterval(pollUpdating, UPDATE_POLL_MS);
		buttonWatch = setInterval(pollButton, BUTTON_POLL_MS);
		// Re-evaluate the latch's expiry even when nothing else ticks.
		clock = setInterval(() => (now = new Date()), 20_000);
	});
	onDestroy(() => {
		if (poll) clearInterval(poll);
		if (clock) clearInterval(clock);
		if (updateWatch) clearInterval(updateWatch);
		if (buttonWatch) clearInterval(buttonWatch);
	});

	// ── the record ────────────────────────────────────────────────────────
	//
	// One line at a time, advancing once per ambient poll — every 30s, which is
	// the slowest thing on the screen and deliberately so. This panel runs 24/7
	// in someone's home and the design forbids animation loops; a line that
	// changes twice a minute is furniture, one that changes every second is a
	// slot machine.
	//
	// Server-supplied and already filtered: `record` omits anything with a zero
	// count, so this never prints "0 emails".
	let logIndex = $state(0);
	$effect(() => {
		// Advance on each new poll, not on a timer of its own — one clock.
		const n = state_?.record?.length ?? 0;
		if (n > 0) logIndex = logIndex % n;
	});

	const logLine = $derived.by(() => {
		const rec = state_?.record ?? [];
		if (!rec.length) return null;
		const l = rec[logIndex % rec.length];
		// Grouped thousands: "41,000 messages" is a number someone reads, and
		// "41000 messages" is one they have to count.
		return `${l.count.toLocaleString()} ${l.label}`;
	});

	// The oldest trace, as a month and year. Does more work than any count —
	// most people have no idea their Mac has kept messages for a decade.
	const sinceLabel = $derived.by(() => {
		const iso = state_?.record_since;
		if (!iso) return null;
		const d = new Date(iso);
		if (Number.isNaN(d.getTime())) return null;
		return `since ${d.toLocaleDateString([], { month: "long", year: "numeric" })}`;
	});

	const timeLabel = $derived(
		now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false }),
	);
	const dateLabel = $derived(
		now.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" }),
	);

	// ── the face ──────────────────────────────────────────────────────────
	//
	// The ambient slot's tenant (Settings → Display). The face NEVER replaces
	// the interruptions or the setup screens — those branches sit above this
	// one and outrank any choice the owner made, which is the whole contract:
	// the glass always tells the truth about the box's condition first.
	//
	// An applet face is the same sandboxed-iframe runtime the app uses
	// (server/faces.rs). This page runs on the box, so its loopback fetch
	// authenticates as local-console and can mint its own face token — no
	// pairing ceremony for the box's own screen.
	const faceAppletId = $derived(
		state_?.claimed && state_.face?.kind === "applet"
			? (state_.face.applet_id ?? null)
			: null,
	);
	const matte = $derived(
		(state_?.claimed &&
			state_.face?.kind === "builtin" &&
			state_.face.builtin === "matte") ??
			false,
	);

	let faceSrc = $state<string | null>(null);
	// Bumped when the server returns from an outage: its in-memory token
	// store restarted empty, so the hung face needs a fresh token + reload.
	let faceEpoch = $state(0);

	// Tokens live one hour (faces.rs TOKEN_TTL); a panel face lives for days.
	// Re-mint well inside the TTL — the src change reloads the iframe, which
	// at this cadence is furniture settling, not an animation.
	const FACE_REMINT_MS = 45 * 60 * 1000;

	$effect(() => {
		const id = faceAppletId;
		// Read so a bump re-runs this effect (fresh mint + iframe reload).
		void faceEpoch;
		if (!id) {
			faceSrc = null;
			return;
		}
		let cancelled = false;
		const mint = async () => {
			try {
				const res = await fetch(
					`/api/applets/${encodeURIComponent(id)}/face-token`,
				);
				if (!res.ok) throw new Error(String(res.status));
				const { token } = await res.json();
				if (cancelled) return;
				faceSrc = `/face/${encodeURIComponent(id)}/?vt=${encodeURIComponent(
					token,
				)}&theme=dark&surface=panel`;
			} catch {
				// A face that can't be hung (applet deleted, token refused) must
				// not blank the glass — fall through to the record screen, which
				// is every box's pre-face ambient.
				if (!cancelled) faceSrc = null;
			}
		};
		void mint();
		const t = setInterval(mint, FACE_REMINT_MS);
		return () => {
			cancelled = true;
			clearInterval(t);
		};
	});
</script>

<div class="screen">
	{#if held !== null}
		<!-- THE CASE BUTTON, being held right now. Outranks everything, including
		     the update and disk states: something is happening to this box in
		     the next second or two and the person doing it — or the cable
		     resting on it — needs to see that before it lands, not after.

		     Counts DOWN, not up: "2" is a deadline, "1s held" is trivia. And it
		     names what will happen in the owner's words, because the one thing
		     they must not conclude is that they are wiping their record. -->
		<div class="fault">
			<span class="lockup"><span class="mk">∴</span>{state_?.box_name ?? ""}</span>
			<p class="doing">Keep holding to forget your devices</p>
			<div class="phrase">{remaining}</div>
			<div class="foot">Your record and your words are kept. You'll set your
				devices up again with the four words you saved.</div>
		</div>
	{:else if updatingNow}
		<!-- UPDATING. Outranks every other screen, including the boot mark: the
		     server is gone on purpose, and this is the one state where the
		     panel knows something the box cannot currently tell it. Says what
		     is happening, that it is deliberate, and not to intervene — the
		     owner's instinct at a dark appliance is to pull the power, which
		     during a migration is the one thing that could actually hurt. -->
		<div class="fault">
			<span class="lockup"><span class="mk">∴</span>{state_?.box_name ?? ""}</span>
			<p class="doing">Updating</p>
			<div class="recall">Back in a minute. Don't unplug me.</div>
		</div>
	{:else if state_?.data_disk_fault}
		<!-- NO DATA DISK. Also outranks everything: this box booted precisely
		     so it could say this (fstab carries `nofail`), and an ambient screen
		     reporting "REACHABLE · 3 devices syncing" over a missing disk would
		     spend the entire value of having booted. -->
		<div class="fault">
			<span class="lockup"><span class="mk">∴</span>{state_.box_name}</span>
			<p class="doing">Storage disconnected</p>
			<div class="recall">{state_.data_disk_fault}</div>
		</div>
	{:else if mirrorDenied}
		<!-- Neither door answered: this browser is not the box and not
		     paired. The one state that must NOT be a silent bootmark —
		     someone deliberately opened this page on a spare screen, and an
		     honest sentence is what turns that into a working setup. -->
		<div class="fault">
			<span class="lockup"><span class="mk">∴</span>Virtues</span>
			<p class="doing">This screen isn't paired</p>
			<div class="recall">
				Any paired device can wear the box's face. Pair this one — open the
				app here, Settings → Devices → Add device — then come back to this
				page and go full screen.
			</div>
		</div>
	{:else if !state_}
		<!-- Pre-first-response. Deliberately bare: the box is seconds from
		     answering and a spinner would be the first thing it ever said. -->
		<div class="boot"><span class="bootmark">∴</span></div>
	{:else if !state_.claimed}
		<!-- ONE SETUP SCREEN. Not step 1 of 3 — there is no sequence any more.
		     The app is the wizard, so this screen has exactly two jobs: point at
		     the app, and show the four words that let the app configure this box.

		     THE PHRASE. An unclaimed box advertises over Bluetooth to anything in
		     radio range, and radio range passes through walls. Four words printed
		     here prove LINE OF SIGHT, which is the bar we actually want — and they
		     are the same words that get the owner back in after a reset. They
		     rotate every 15 minutes while the box is empty (so a photograph taken
		     last week is worthless) and freeze forever the moment it is claimed.
		     See docs/onboarding-paradigm.md §1.

		     BRAND TOP-LEFT, CODENAME BOTTOM-LEFT (2026-08-19, bench feedback):
		     an unboxed unit's first screen led with "Honest Kestrel", which reads
		     as a mystery, not a product — the person at the shelf needs to know
		     WHAT this is before which one it is. The codename keeps a corner
		     because it is still the identity check against the app's chooser
		     when two boxes share a house. -->
		<span class="lockup"><span class="mk">∴</span>Virtues</span>
		<span class="devname">{state_.box_name}</span>
		<div class="body">
			{#if session !== null}
				<!-- The words have been accepted, so they leave the glass — two
				     jobs at once. They are spent, so nobody who wanders past can
				     read them; and the owner gets confirmation ON THE BOX that
				     what they typed landed here. It also makes a race they did
				     not start visible while it is happening: a name they do not
				     recognise, on their own hardware. -->
				<p class="doing">Setting up</p>
				<div class="live">
					<span class="pip"></span>{session ? `with ${session}` : "with a nearby device"}
				</div>
			{:else}
				<p class="doing">Get Virtues for your computer</p>
				{#if state_.phrase_frozen}
					<!-- RESET, NOT VIRGIN. This box still holds a life, so its
					     phrase stays frozen and off the screen. Rendering the
					     virgin layout here would leave a blank where the words go
					     and read as a fault at the worst possible moment — the
					     second line is what stops someone assuming the reset wiped
					     them. -->
					<p class="instruct">
						virtues.com/downloads — then type the words you saved when you first set
						this box up.
					</p>
					<div class="recall">I can't show them again — your record is still here.</div>
				{:else if state_.setup_phrase}
					<p class="instruct">virtues.com/downloads — then type these words.</p>
					<div class="phrase">{state_.setup_phrase}</div>
				{:else if mirror}
					<!-- A remote glass during setup. The redaction is the
					     feature: this screen may say the words exist, and only
					     the box's own glass may say what they are. -->
					<p class="instruct">
						virtues.com/downloads — the setup words show on the box's own
						screen.
					</p>
				{:else}
					<!-- Neither frozen nor minted: the box could not produce a
					     phrase at all. Say so, rather than show a gap. -->
					<p class="instruct">virtues.com/downloads</p>
					<div class="recall fault">I can't show setup words right now.</div>
				{/if}
			{/if}
			{#if state_.connectivity === "portal"}
				<!-- Captive network. With honest online-detection a portal join
				     reads as still-offline, and without this the screen looks like
				     the join silently failed. Seen live at WeWork 2026-08-11. -->
				<div class="foot warn">
					Joined {state_.wifi_ssid ?? "a network"}, but it wants a browser sign-in I
					can't do — pick another network in the app.
				</div>
			{:else if state_.connectivity === "limited"}
				<div class="foot warn">
					Joined {state_.wifi_ssid ?? "a network"}, but no internet is getting through —
					pick another network in the app.
				</div>
			{/if}
		</div>
	{:else if matte}
		<!-- MATTE. Black glass on purpose — reserve, not off. Chosen in
		     Settings → Display; the interruptions above still take the screen
		     when they must, which is what makes choosing nothing safe. -->
		<div class="matte"></div>
	{:else if faceAppletId && faceSrc}
		<!-- AN APPLET FACE, hanging in the ambient slot. Full-bleed and in the
		     same jail it lives in everywhere else: opaque origin, strict CSP,
		     read-only face_reader bridge. If it cannot be hung, `faceSrc`
		     stays null and the record screen below renders instead. -->
		<iframe class="panelface" src={faceSrc} sandbox="allow-scripts" title="Face"
		></iframe>
	{:else}
		<!-- AMBIENT. The screen someone sees ten thousand times, so it reports
		     the record rather than the machine — a ship's log, not htop. -->
		<div class="amb">
			<div class="top">
				<div class="ambname"><span class="ambmark">∴</span> {state_.box_name}</div>
				<div class="status" class:offline={!state_.online || unreachable}>
					{unreachable ? "NO SERVER" : state_.online ? "REACHABLE" : "OFFLINE"}
				</div>
			</div>
			<div class="log">
				{#if logLine}
					<!-- THE RECORD, counted. One line, rotating on the ambient poll.
					     What was here before was a kicker reading "TODAY SO FAR" over
					     the string literal "Your box is keeping the record." — a
					     promise of live content over something that could never
					     change. The only true line on the screen was the device
					     count. -->
					<div class="kicker">THE RECORD</div>
					<div class="line">{logLine}</div>
					<div class="meta">
						{sinceLabel ? `${sinceLabel} · ` : ""}{state_.devices}
						{state_.devices === 1 ? "device" : "devices"} syncing
					</div>
				{:else}
					<!-- A box holding nothing says so plainly. A census of absences
					     is a list of reproaches, and someone who has just set up a
					     box has not failed at anything yet. -->
					<div class="kicker">THE RECORD</div>
					<div class="line">Nothing has arrived yet.</div>
					<div class="meta">
						{state_.devices}
						{state_.devices === 1 ? "device" : "devices"} syncing
					</div>
				{/if}
			</div>
			<div class="ambfoot">{dateLabel} &middot; {timeLabel}</div>
		</div>
	{/if}
</div>

<style>
	/* Self-contained: this page never shares a shell with the app, and the
	   kiosk has no user to change theme, so the tokens are literal. */
	.screen {
		--ink: #f5f2ec;
		--dim: #7d8b99;
		--faint: #4a5663;
		--bg: #0b0f14;
		--ok: #5fb07e;
		--warn: #c9a227;
		position: fixed;
		inset: 0;
		background: var(--bg);
		color: var(--ink);
		font-family: system-ui, -apple-system, sans-serif;
		overflow: hidden;
		/* ONE GROUND. The old light/dark split filled the strip but cost the
		   phrase most of its width, and a screen that is half white at 3am in
		   a bedroom is its own argument. */
		display: flex;
		align-items: center;
		padding: 0 44px;
		box-sizing: border-box;
	}

	.boot,
	.amb {
		display: flex;
		width: 100%;
		height: 100%;
	}
	.boot {
		align-items: center;
		justify-content: center;
	}
	/* Updating / storage-disconnected. Borrows the setup screen's column so a
	   box that drops into one of these does not also change shape — the
	   sentence changes, the furniture does not. */
	.fault {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	.bootmark {
		font-size: 2rem;
		color: var(--faint);
	}

	/* ── setup ── */
	.body {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
	}
	/* Mark and name as one quiet lockup, top-left. The name is NOT the heading:
	   a box announcing itself is odd, and the heading's job is what the person
	   should do. This is identity — the thing you check against the app before
	   typing a secret into it, which matters when two boxes share a house. */
	.lockup {
		position: absolute;
		top: 20px;
		left: 40px;
		display: flex;
		align-items: baseline;
		gap: 10px;
		font-size: 0.72rem;
		color: #55636f;
		letter-spacing: 0.04em;
	}
	.lockup .mk {
		font-size: 0.82rem;
		color: #46545f;
	}
	/* The codename, bottom-left on the setup screen — identity demoted to a
	   corner, not deleted: it is what you check against the app's box chooser
	   when two boxes share a house. */
	.devname {
		position: absolute;
		bottom: 20px;
		left: 40px;
		font-size: 0.72rem;
		color: #55636f;
		letter-spacing: 0.04em;
	}
	/* Serif, because it is the one voiced line on an otherwise instrumental
	   screen. Georgia is not on Ubuntu — the fallbacks are what actually ship. */
	.doing {
		font-family: Georgia, "Liberation Serif", "DejaVu Serif", serif;
		font-size: 1.3rem;
		font-weight: 400;
		letter-spacing: -0.005em;
		color: var(--ink);
		margin: 0 0 4px;
	}
	.instruct {
		font-size: 0.76rem;
		color: var(--dim);
		line-height: 1.45;
		margin: 0 0 22px;
		max-width: 420px;
	}
	/* ONE LINE. It is read across a room while typing on another machine, and a
	   phrase that wraps loses its shape — you lose your place mid-word. The
	   wordlist is capped at 7-letter words for exactly this reason
	   (`setup_phrase::MAX_WORD_LEN`), so `nowrap` here is a tripwire: if a
	   longer word ever gets in, it clips visibly instead of wrapping quietly. */
	.phrase {
		font-family: ui-monospace, "SF Mono", Menlo, "DejaVu Sans Mono", monospace;
		font-size: 1.45rem;
		line-height: 1.25;
		letter-spacing: 0.01em;
		color: var(--ink);
		white-space: nowrap;
	}
	/* The reset state's stand-in for the phrase: same slot, quieter, and it
	   points at the owner's password manager instead of at this screen.
	   Deliberately NOT mono — mono on this screen means "these are the
	   characters, type them", and this is a sentence. Setting it in the phrase's
	   typeface would tell an owner to type "I can't show them again". */
	.recall {
		font-size: 0.95rem;
		line-height: 1.4;
		color: var(--ink);
		max-width: 460px;
	}
	.recall.fault {
		color: var(--warn);
	}
	/* Session narration. One live line, no spinner — the panel is furniture and
	   runs 24/7; a breathing dot is the smallest thing that reads as "now". */
	.live {
		display: flex;
		align-items: center;
		gap: 11px;
		font-size: 1.02rem;
		color: var(--ink);
	}
	.live .pip {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--ok);
		flex: none;
		animation: breathe 2.4s ease-in-out infinite;
	}
	@keyframes breathe {
		0%,
		100% {
			opacity: 0.3;
		}
		50% {
			opacity: 1;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.live .pip {
			animation: none;
			opacity: 0.85;
		}
	}
	.foot {
		margin-top: 20px;
		font-size: 0.68rem;
		line-height: 1.5;
		color: var(--faint);
		max-width: 460px;
	}
	.foot.warn {
		color: var(--warn);
	}

	/* ── the face ── */
	/* True black, not the page's near-black: matte means the least light the
	   backlight allows, and the difference is visible in a dark room. */
	.matte {
		position: absolute;
		inset: 0;
		background: #000;
	}
	.panelface {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		border: 0;
		background: var(--bg);
	}

	/* ── ambient ── */
	.amb {
		flex-direction: column;
		padding: 17px 34px 13px;
		/* The setup layout centres its child; ambient owns the whole strip. */
		position: absolute;
		inset: 0;
		box-sizing: border-box;
	}
	.top {
		display: flex;
		justify-content: space-between;
		align-items: baseline;
		flex: none;
	}
	/* The name carries through from setup, in the same serif. */
	.ambname {
		font-family: Georgia, "Liberation Serif", "DejaVu Serif", serif;
		font-size: 0.95rem;
		color: var(--dim);
		display: flex;
		align-items: baseline;
		gap: 9px;
	}
	.ambmark {
		font-family: system-ui, sans-serif;
		font-size: 0.8rem;
		color: #46545f;
	}
	.status {
		font-family: ui-monospace, Menlo, monospace;
		font-size: 0.6rem;
		letter-spacing: 0.1em;
		color: var(--ok);
		display: flex;
		align-items: center;
		gap: 5px;
	}
	.status::before {
		content: "";
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: currentColor;
	}
	.status.offline {
		color: var(--warn);
	}
	.log {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 13px;
		border-top: 1px solid #1b242e;
		margin-top: 11px;
	}
	.kicker {
		font-family: ui-monospace, Menlo, monospace;
		font-size: 0.6rem;
		letter-spacing: 0.11em;
		color: var(--faint);
	}
	.line {
		font-size: 1.3rem;
		line-height: 1.42;
		max-width: 470px;
	}
	.meta {
		font-size: 0.7rem;
		color: var(--faint);
	}
	.ambfoot {
		flex: none;
		height: 24px;
		display: flex;
		align-items: center;
		font-family: ui-monospace, Menlo, monospace;
		font-size: 0.6rem;
		letter-spacing: 0.06em;
		color: var(--faint);
	}
</style>
