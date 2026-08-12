<!--
  /display — the 7" panel on the front of an appliance.

  Rendered by a WebKit kiosk running ON the box (installer's
  virtues-display.service: cage + WebKit on bare DRM, no X, no desktop). It is
  the only interface a Dragon owner has before any device is paired.

  It lives in (public) because it draws before a session exists. The secret it
  shows — the standing pair code — is protected at the API, not here:
  /api/display/state refuses anything that isn't loopback, and this page only
  ever runs on the box itself.

  DESIGN CONSTRAINTS, all measured on real glass:

  * The canvas is 585 x 329 CSS px, not 1920x1080. The panel's EDID claims
    53x30cm (~24") when it is physically 15.5x8.7cm, so the kiosk pins
    devicePixelRatio to 3.28 and the usable layout is roughly a phone in
    landscape. One idea per screen; there is no room for a second.
  * It is a LANDSCAPE STRIP. Vertically stacked, centred content leaves most of
    the frame dead — the full-bleed split is what fills it.
  * Display-only. The digitizer doesn't work through the cover glass, so
    nothing here is interactive, and the pair code in particular must carry no
    affordance that invites a tap.
  * It runs 24/7 in someone's home: dark, no spinners, no animation loops.

  TWO STATES, and that is the whole design:

    1. unclaimed → GET THE APP, and the four words that let it in.
    2. claimed   → ambient.

  There used to be three numbered setup screens (join a network, link an
  account, then a pair code). They are gone. The app carries all three over one
  Bluetooth conversation, so the panel's only job is to start it — point at the
  app, and show the phrase that proves whoever is typing can SEE this box.

  What that replaced is worth remembering, because the failure was instructive:
  screens 1 and 2 were once a single screen carrying two different secrets at
  once, and the first person shown it read the big code on the right and typed
  it as the wifi password. Splitting them helped; deleting them helped more.
  Every code on this glass existed because two machines could not talk, and now
  they can.

-->
<script lang="ts">
	import { onMount, onDestroy } from "svelte";

	type DisplayState = {
		box_name: string;
		linked: boolean;
		link_code: string | null | undefined;
		setup_phrase: string | null | undefined;
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

	// Grouped "123 456" — the code is read aloud off a screen and typed on a
	// numeric pad, and the gap is what stops people losing their place.
	const grouped = $derived(
		state_?.pair_code ? `${state_.pair_code.slice(0, 3)} ${state_.pair_code.slice(3)}` : null,
	);

	async function refresh() {
		try {
			const res = await fetch("/api/display/state");
			if (!res.ok) throw new Error(String(res.status));
			state_ = await res.json();
			unreachable = false;
		} catch {
			// Keep the last good state on screen. A box whose server blipped
			// should not blank the panel — the code on it is still valid.
			unreachable = true;
		}
	}

	// Two cadences, because the screen has two jobs.
	//
	// SETUP is a conversation: someone is standing in front of the box acting on
	// what it says, and every transition they cause — AP coming up, wifi joined,
	// device paired — must land while they are still looking. At 30s the panel
	// spent up to half a minute telling someone to scan a QR that was not on it
	// yet, which reads as a fault rather than a wait. Seen on hardware.
	//
	// AMBIENT is furniture. Nothing there changes on a human timescale, it runs
	// 24/7 in someone's home, and the standing code rotates every 15 min with a
	// 5 min overlap — so 30s can never show an expired one.
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
		<div class="boot"><span class="mark">∴</span></div>
	{:else if !state_.claimed}
		<!-- ONE SETUP SCREEN. Not step 1 of 3 — there is no sequence any more.
		     The app is the wizard, so this screen has exactly two jobs: point at
		     the app, and show the four words that let the app configure this box.

		     THE PHRASE. An unclaimed box advertises over Bluetooth to anything in
		     radio range, and radio range passes through walls. Four words printed
		     here prove LINE OF SIGHT, which is the bar we actually want — and they
		     are the same words that get the owner back in after a reset. They
		     rotate every 15 minutes while the box is empty (so a photograph taken
		     last week is worthless) and vanish forever the moment it is claimed.
		     See docs/onboarding-paradigm.md §1.

		     The link QR and the pair code that used to live on screens 2 and 3 are
		     GONE from the panel: the app carries the account grant and the pairing
		     over the same Bluetooth session that starts here. -->
		<div class="split">
			<div class="lite">
				<img class="qr" src="/api/display/app-qr" alt="" />
				<div class="apcreds">
					<div class="aplabel">Get the app</div>
					<div class="apssid">virtues.com/downloads</div>
				</div>
			</div>
			<div class="dark">
				<div class="brand">∴ &nbsp;{state_.box_name}</div>
				<div class="head">Get the Virtues app</div>
				<div class="lead">Open it and type these words — it finds me over Bluetooth
					and does the rest.</div>
				{#if state_.setup_phrase}
					<div class="phrase">{state_.setup_phrase}</div>
				{:else}
					<div class="phrase fault">— — —</div>
				{/if}
				{#if state_.connectivity === "portal"}
					<!-- Captive network. With honest online-detection a portal join
					     reads as still-offline, and without this the screen looks like
					     the join silently failed. Seen live at WeWork 2026-08-11. -->
					<div class="foot warn">
						Joined {state_.wifi_ssid ?? "a network"}, but it wants a browser
						sign-in, which I can't do — pick a different network in the app.
					</div>
				{:else if state_.connectivity === "limited"}
					<div class="foot warn">
						Joined {state_.wifi_ssid ?? "a network"}, but no internet is getting
						through — pick a different network in the app.
					</div>
				{/if}
			</div>
		</div>
	{:else}
		<!-- AMBIENT. The screen someone sees ten thousand times, so it reports
		     the record rather than the machine — a ship's log, not htop. -->
		<div class="amb">
			<div class="top">
				<div class="brand">∴ &nbsp;{state_.box_name}</div>
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
		--ink: #f4f1ea;
		--dim: #93a0ad;
		--faint: #54606c;
		--bg: #0b0f14;
		--lite: #ffffff;
		--ok: #5fb07e;
		--warn: #c9a227;
		position: fixed;
		inset: 0;
		background: var(--bg);
		color: var(--ink);
		font-family: system-ui, -apple-system, sans-serif;
		overflow: hidden;
	}

	.boot,
	.split,
	.amb {
		display: flex;
		width: 100%;
		height: 100%;
	}
	.boot {
		align-items: center;
		justify-content: center;
	}
	.mark {
		font-size: 2rem;
		color: var(--faint);
	}

	/* ── setup: full-bleed split ── */
	.apcreds {
		margin-top: 11px;
		text-align: center;
		font-family: ui-monospace, Menlo, monospace;
		line-height: 1.45;
	}
	.aplabel {
		font-family: system-ui, sans-serif;
		font-size: 0.5rem;
		text-transform: uppercase;
		letter-spacing: 0.13em;
		color: #8f8f8f;
		margin-top: 6px;
	}
	.apssid {
		font-size: 0.7rem;
		color: #1c1c1c;
		letter-spacing: 0.04em;
	}
	.lite {
		width: 41%;
		flex: none;
		background: var(--lite);
		display: flex;
		/* COLUMN, and it has to be. Without it the QR (150px) and the credentials
		   (~99px) lay out side by side in a 240px panel: the QR hangs 5px off the
		   left edge of the screen and the passphrase runs 4px into the dark half.
		   Measured, not guessed — and invisible in review, because both children
		   overflow their parent without the parent itself overflowing, so nothing
		   scrolls and nothing reports a wrong size. */
		flex-direction: column;
		align-items: center;
		justify-content: center;
		padding: 0 20px;
	}
	.qr {
		width: 150px;
		height: 150px;
		/* A flex item with an intrinsic size shrinks by default. This QR is
		   pointed at by a phone camera; a silently squashed one still scans
		   badly rather than not at all, which is the worse failure. */
		flex: none;
	}
	.dark {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		justify-content: center;
		padding: 0 42px;
		position: relative;
	}
	.brand {
		position: absolute;
		top: 19px;
		right: 36px;
		font-size: 0.72rem;
		color: #3f4b57;
	}
	.step {
		font-family: ui-monospace, Menlo, monospace;
		font-size: 0.6rem;
		letter-spacing: 0.13em;
		text-transform: uppercase;
		color: #46525f;
		margin-bottom: 9px;
	}
	.head {
		font-size: 1.6rem;
		line-height: 1.15;
		letter-spacing: -0.01em;
		color: #f7f5f0;
		margin-bottom: 11px;
	}
	.lead {
		font-size: 0.875rem;
		color: #8894a1;
		margin-bottom: 13px;
		line-height: 1.45;
		max-width: 290px;
	}
	.code {
		font-size: 3.75rem;
		font-weight: 600;
		line-height: 1;
		letter-spacing: 0.055em;
		font-variant-numeric: tabular-nums;
		color: #f7f5f0;
	}
	.code.linkcode {
		font-size: 2.6rem;
		letter-spacing: 0.09em;
	}
	/* Four words, read off glass and typed on another machine. Sized to be
	   legible across a room, wrapping rather than shrinking — a phrase that
	   truncates is worse than one that takes two lines. */
	.phrase {
		font-family: ui-monospace, Menlo, monospace;
		font-size: 1.55rem;
		line-height: 1.3;
		letter-spacing: 0.01em;
		color: #f7f5f0;
		word-break: break-word;
		max-width: 300px;
	}
	.phrase.fault {
		color: var(--warn);
	}
	.qr-pending-mark {
		width: 150px;
		height: 150px;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 1.5rem;
		color: #bdbdbd;
		flex: none;
	}
	.code.fault {
		color: var(--warn);
		font-size: 2.5rem;
	}
	.foot {
		margin-top: 19px;
		font-size: 0.72rem;
		line-height: 1.5;
		color: #4c5764;
	}
	.foot.warn {
		color: var(--warn);
	}

	/* ── ambient ── */
	.amb {
		flex-direction: column;
		padding: 17px 34px 13px;
	}
	.top {
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex: none;
	}
	.amb .brand {
		position: static;
		font-size: 0.75rem;
		color: var(--dim);
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
