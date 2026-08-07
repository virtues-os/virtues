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

  Two states, no wizard: setting up (code + QR) and ambient (once claimed).
-->
<script lang="ts">
	import { onMount, onDestroy } from "svelte";

	type DisplayState = {
		pair_code: string | null;
		ap_ssid: string | null;
		ap_passphrase: string | null;
		claimed: boolean;
		online: boolean;
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

	onMount(() => {
		void refresh();
		// The standing code rotates every 15 min with a 5 min overlap, so a
		// 30s poll can never show an expired one.
		poll = setInterval(refresh, 30_000);
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
		<!-- SETUP. One screen, no advance: the box cannot detect that the app
		     has been installed, so there is no state to move between. -->
		<div class="split">
			<div class="lite">
				{#if state_.ap_ssid}
					<img class="qr" src="/api/display/qr" alt="" />
					<!-- The passphrase in readable text, not only inside the QR. A QR
					     needs a camera, and the device that needs this network is often
					     a laptop. Shipping it QR-only stranded the lab box. -->
					<div class="apcreds">
						<div class="apssid">{state_.ap_ssid}</div>
						{#if state_.ap_passphrase}
							<div class="appass">{state_.ap_passphrase}</div>
						{/if}
					</div>
				{:else}
					<div class="qr-missing">no setup network</div>
				{/if}
			</div>
			<div class="dark">
				<div class="brand">∴ &nbsp;Virtues</div>
				<div class="lead">Scan with your phone, then enter</div>
				{#if grouped}
					<div class="code">{grouped}</div>
				{:else}
					<div class="code fault">— — —</div>
				{/if}
				<div class="foot">
					No app yet? <b>virtues.com/downloads</b><br />
					Or plug in ethernet and this finishes itself.
				</div>
			</div>
		</div>
	{:else}
		<!-- AMBIENT. The screen someone sees ten thousand times, so it reports
		     the record rather than the machine — a ship's log, not htop. -->
		<div class="amb">
			<div class="top">
				<div class="brand">∴ &nbsp;virtues.local</div>
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
		--lite: #f2efe8;
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
	.apssid {
		font-size: 0.62rem;
		color: #8a8578;
		letter-spacing: 0.04em;
	}
	.appass {
		font-size: 0.78rem;
		color: #1a1a1a;
		letter-spacing: 0.06em;
	}
	.lite {
		width: 41%;
		flex: none;
		background: var(--lite);
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.qr {
		width: 150px;
		height: 150px;
	}
	.qr-missing {
		font-size: 0.7rem;
		color: #8a8578;
		font-family: ui-monospace, Menlo, monospace;
	}
	.dark {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		justify-content: center;
		padding: 0 34px;
		position: relative;
	}
	.brand {
		position: absolute;
		top: 19px;
		right: 28px;
		font-size: 0.72rem;
		color: #3f4b57;
	}
	.lead {
		font-size: 0.875rem;
		color: #8894a1;
		margin-bottom: 13px;
	}
	.code {
		font-size: 3.75rem;
		font-weight: 600;
		line-height: 1;
		letter-spacing: 0.055em;
		font-variant-numeric: tabular-nums;
		color: #f7f5f0;
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
	.foot b {
		color: #75828f;
		font-weight: 400;
	}

	/* ── ambient ── */
	.amb {
		flex-direction: column;
		padding: 17px 26px 13px;
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
