<!--
  /display — the 7" panel on the front of an appliance.

  Rendered by a WebKit kiosk running ON the box (installer's
  virtues-display.service: cage + WebKit on bare DRM, no X, no desktop). It is
  the only interface a Dragon owner has before any device is paired.

  It lives in (public) because it draws before a session exists. The secret it
  shows — the setup phrase — is protected at the API, not here:
  /api/display/state refuses anything that isn't loopback, and this page only
  ever runs on the box itself.

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

  FIVE STATES:

    1. unclaimed, virgin  → get the Mac app, and the four words that let it in
    2. unclaimed, frozen  → reset box: the words are the ones you saved
    3. session live       → the words are spent; who is setting up, instead
    4. pairing            → the six digits the app is asking for
    5. claimed            → ambient

  State 4 was missing until 2026-08-13, and the omission was invisible from
  here: every state above is about the PHRASE, so a redesign that got the
  phrase right looked complete. The app meanwhile said "the 6 digits on its
  screen right now" about a screen with no way to show them. This panel's
  states are not its own — they are the app's steps, and the two have to be
  read together or a whole step can go missing.

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
		link_code: string | null | undefined;
		setup_phrase: string | null | undefined;
		phrase_frozen: boolean;
		setup_session: string | null | undefined;
		pair_code: string | null;
		ap_ssid: string | null;
		ap_passphrase: string | null;
		claimed: boolean;
		online: boolean;
		connectivity: string;
		wifi_ssid: string | null | undefined;
		devices: number;
	};

	let state_ = $state<DisplayState | null>(null);
	let unreachable = $state(false);
	let now = $state(new Date());
	let poll: ReturnType<typeof setInterval> | null = null;
	let clock: ReturnType<typeof setInterval> | null = null;

	// A live BLE setup session, if there is one. `""` is a real value from the
	// server — a session whose client sent no name — so test for null, not truth.
	const session = $derived(state_?.setup_session ?? null);

	async function refresh() {
		try {
			const res = await fetch("/api/display/state");
			if (!res.ok) throw new Error(String(res.status));
			state_ = await res.json();
			unreachable = false;
		} catch {
			// Keep the last good state on screen. A box whose server blipped
			// should not blank the panel — the phrase on it is still valid.
			unreachable = true;
		}
	}

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
		const want = state_?.claimed ? AMBIENT_POLL_MS : SETUP_POLL_MS;
		if (want !== pollMs) schedulePoll(want);
	});

	onMount(() => {
		void refresh();
		schedulePoll(SETUP_POLL_MS);
		clock = setInterval(() => (now = new Date()), 20_000);
	});
	onDestroy(() => {
		if (poll) clearInterval(poll);
		if (clock) clearInterval(clock);
	});

	const timeLabel = $derived(
		now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", hour12: false }),
	);
	const dateLabel = $derived(
		now.toLocaleDateString([], { weekday: "short", day: "numeric", month: "short" }),
	);
</script>

<div class="screen">
	{#if !state_}
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
		     See docs/onboarding-paradigm.md §1. -->
		<span class="lockup"><span class="mk">∴</span>{state_.box_name}</span>
		<div class="body">
			{#if state_.pair_code && state_.linked && session === null}
				<!-- PAIRING. The step this panel had no screen for at all until
				     2026-08-13: the redesign built every state around the setup
				     phrase and dropped the six digits, so the app said "the 6
				     digits on its screen right now" about a screen that showed
				     none. A live owner hit the wall mid-run.

				     It replaces the phrase rather than joining it. The phrase's
				     job — proving line of sight to open a session — is done by
				     the time either condition here is true, and ONE CODE AT A
				     TIME is the rule the whole redesign came from: two codes on
				     one screen and nobody knows which one is being asked for.

				     ONLY WHEN NO SESSION IS LIVE. Any box new enough to have
				     this state also has RPC 0x85, so a connected app can always
				     fetch the code itself — printing it during a live session
				     was showing an owner a number nobody was going to ask them
				     for, in the one place this redesign exists to keep clear.

				     What remains is the case that earns the state: pairing a
				     first device with no Bluetooth in play at all — box on
				     ethernet, or a Mac with the radio off — where the glass is
				     the only source of the code. Also covers a session that
				     lapsed mid-flow.

				     `linked` is the guard against the other direction: an
				     unclaimed box that has not linked yet is still waiting for
				     someone to START, and that owner needs the phrase, not a
				     pair code. -->
				<p class="doing">Pair your app</p>
				<p class="instruct">Type these six digits into the app.</p>
				<div class="phrase code6">{state_.pair_code}</div>
			{:else if session !== null}
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
				<p class="doing">Get Virtues for Mac</p>
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
				<div class="kicker">TODAY SO FAR</div>
				<div class="line">Your box is keeping the record.</div>
				<div class="meta">
					{state_.devices}
					{state_.devices === 1 ? "device" : "devices"} syncing
				</div>
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
	/* Six digits, unlike four words, are TRANSCRIBED character by character —
	   so they get the room the phrase cannot afford and the spacing that keeps
	   a glance from losing its place. Tabular figures so the width never
	   shifts as the code rotates. */
	.code6 {
		font-size: 2.6rem;
		letter-spacing: 0.16em;
		font-variant-numeric: tabular-nums;
		margin-top: 0.1rem;
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
