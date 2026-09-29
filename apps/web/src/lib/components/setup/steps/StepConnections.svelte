<!--
	Connections. THIS DEVICE FIRST, then the other one.

	The step knows which device is in hand (`here`) and does that device's
	part right there, before offering the other:

	  - This Mac: the collector (a one-time token, then the app installs it),
	    then Full Disk Access (Messages and Safari history, required) and
	    Accessibility (optional). macOS has no prompt for either, so the
	    permissions panel (MacPermissions) opens the right Settings pane and
	    asks the collector to check again when the window comes back.
	  - This iPhone (or iPad): location, then Health, one sheet at a time,
	    each answered before the next; then the microphone, after the same
	    consent Settings asks for, because it records everyone in the room.
	    What was refused says so and opens the app's page in Settings.
	  - A computer that isn't a Mac (the Windows or Linux app): collecting is
	    Mac only, said plainly, and the iPhone is still offered.
	  - A phone's browser: the phone comes first, with where to get the app.

	THE OTHER DEVICE reads its state from the server's device list, the one
	record pairing makes: "Add…" until it is paired, then what it has sent,
	or the one thing still to do on it.

	Continuing is the step's acknowledgement: the server counts the step done
	once a device or account is sending AND the person has moved past it. On
	this Mac, moving on before Full Disk Access is a quiet link that says what
	is left behind, not the filled way forward.

	ACCOUNTS ARE NOT A STEP. Google, a bank and the rest live in Sources.
-->
<script lang="ts">
	import { useShowing } from "../showing";
	import { onDestroy, onMount } from "svelte";
	import { fade } from "svelte/transition";
	import Icon from "$lib/components/Icon.svelte";
	import DevicePairModal from "$lib/components/sources/DevicePairModal.svelte";
	import MacPermissions from "$lib/components/devices/MacPermissions.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { isIOS, isIPad, isMacOS, isPhoneBrowser, isTauri, thisComputerLabel } from "$lib/utils/platform";
	import * as api from "$lib/api/client";
	import { getCollectorStatus, installCollector, recheckCollector, type CollectorStatus } from "$lib/tauri/bridge";
	import { macReady } from "$lib/devices/shared";
	import {
		enableAudio,
		enableHealth,
		healthPermissionOf,
		locationStatus,
		micPermission,
		audioStatus,
		healthStatus,
		openAppSettings,
		requestLocation,
		type HealthPermission,
		type LocationAuth,
		type MicPermission,
	} from "$lib/tauri/devicePermissions";
	import { setup, intoLabel, type StepPart } from "../setup.svelte";
	import StepFrame from "../StepFrame.svelte";

	let { eyebrow, onnext }: { eyebrow?: string; onnext: () => void } = $props();

	/**
	 * The device in hand. iOS first: an iPad reports a Mac platform. A
	 * computer app that isn't a Mac can't collect; a phone's browser is on the
	 * phone it would otherwise be asked to add.
	 */
	const here: "mac" | "iphone" | "computer" | "phone" | null = isIOS
		? "iphone"
		: isMacOS
			? "mac"
			: isTauri
				? "computer"
				: isPhoneBrowser
					? "phone"
					: null;
	const phoneName = isIPad ? "iPad" : "iPhone";
	const phoneFirst = here === "iphone" || here === "phone";

	let pairing = $state<{ deviceType: "ios" | "mac"; displayName: string } | null>(null);
	let busy = $state(false);
	let error = $state<string | null>(null);
	const showing = useShowing();
	let poll: ReturnType<typeof setInterval> | null = null;
	let live: ReturnType<typeof setInterval> | null = null;

	// ── this Mac ──────────────────────────────────────────────────────────
	let mac = $state<CollectorStatus | null>(null);
	let turningOn = $state(false);

	async function readMac() {
		const s = await getCollectorStatus();
		if (s) mac = s;
	}
	async function recheckMac() {
		const s = await recheckCollector();
		if (s) mac = s;
		else await readMac();
	}

	async function turnOnMac() {
		turningOn = true;
		error = null;
		try {
			const { token } = await api.mintCollectorToken();
			await installCollector(token);
			// Install can return OK while launchd fails to start it, so wait
			// for the daemon to say it is running.
			const deadline = Date.now() + 12_000;
			while (Date.now() < deadline) {
				await readMac();
				if (mac?.running) break;
				await new Promise((r) => setTimeout(r, 1000));
			}
			if (!mac?.running) {
				error = "This Mac didn't start collecting. Try again, and if it still won't, restart this Mac.";
			}
			void setup.refresh();
		} catch (e) {
			// The raw reason is for whoever debugs it, not for the page.
			console.error("[setup] turning on this Mac failed", e);
			error = "This Mac couldn't turn on collecting. Try again, and if it still won't, restart this Mac.";
		} finally {
			turningOn = false;
		}
	}

	// ── this iPhone ───────────────────────────────────────────────────────
	let location = $state<LocationAuth>("unknown");
	let health = $state<HealthPermission>("unknown");
	let mic = $state<MicPermission>("unavailable");
	let recording = $state(false);
	/** Location and Health asked in turn, then the microphone's consent. */
	let phase = $state<"idle" | "asking" | "consent" | "starting">("idle");

	async function readPhone() {
		const [loc, h, m, a] = await Promise.all([locationStatus(), healthStatus(), micPermission(), audioStatus()]);
		location = loc;
		health = healthPermissionOf(h);
		mic = m;
		recording = !!(a?.enabled ?? a?.recording);
	}

	const locationOn = $derived(location === "when_in_use" || location === "always");
	const locationRefused = $derived(location === "denied" || location === "restricted");
	const micOn = $derived(mic === "granted" && recording);
	const micRefused = $derived(mic === "denied");
	/** Health can't say what was refused (iOS hides it), only whether it was
	 *  asked; its data arriving is the proof. */
	const healthArriving = $derived(!!setup.phone.find((p) => p.label === "Health")?.done);

	/** Location, then Health: each sheet answered before the next shows. */
	async function allowPhone() {
		phase = "asking";
		error = null;
		if (!locationOn && !locationRefused) location = await requestLocation();
		if (health !== "requested") health = healthPermissionOf(await enableHealth());
		await readPhone();
		phase = micOn || micRefused ? "idle" : "consent";
		void setup.refresh();
	}

	async function allowMic() {
		phase = "starting";
		await enableAudio();
		await readPhone();
		phase = "idle";
		void setup.refresh();
	}

	async function openSettings() {
		const opened = await openAppSettings();
		if (!opened) error = `Open the Settings app, then Virtues, to turn them on.`;
	}

	const phoneParts = $derived<StepPart[]>([
		{ label: "Location", done: locationOn },
		{ label: "Health", done: healthArriving },
		{ label: "Microphone", done: micOn },
	]);
	const refused = $derived([locationRefused && "Location", micRefused && "the microphone"].filter(Boolean) as string[]);
	const phoneAsked = $derived((locationOn || locationRefused) && health === "requested");

	// Back from Settings (or anywhere): read again, so a permission turned on
	// there lights here without a reload.
	function onVisible() {
		if (document.visibilityState !== "visible" || !showing()) return;
		if (here === "iphone") void readPhone();
		if (here === "mac") void recheckMac();
	}

	onMount(() => {
		// The badges light, and the counts rise, while the person watches.
		// Only while someone can see them (setup/showing.ts).
		live = setInterval(() => showing() && void setup.refresh(), 5000);
		if (here === "mac") {
			void readMac();
			poll = setInterval(() => showing() && void readMac(), 2000);
		} else if (here === "iphone") {
			void readPhone();
		}
		document.addEventListener("visibilitychange", onVisible);
	});
	onDestroy(() => {
		if (poll) clearInterval(poll);
		if (live) clearInterval(live);
		if (typeof document !== "undefined") document.removeEventListener("visibilitychange", onVisible);
	});

	// ── the other device, from the server's device list ────────────────────
	const phoneSilent = $derived(!!setup.iphone && !setup.arrived("phone"));

	// ── what counts as connected ──────────────────────────────────────────
	const macOn = $derived(here === "mac" ? macReady(mac) : !!setup.mac);
	const phoneOn = $derived(here === "iphone" ? locationOn || micOn || healthArriving : !!setup.iphone);
	const anything = $derived(macOn || phoneOn);
	/** On this Mac, collecting but without Full Disk Access: going on loses
	 *  Messages and Safari history, so the way on says so and stays quiet. */
	const macHalfway = $derived(here === "mac" && !!mac?.running && !macReady(mac) && !phoneOn);

	async function moveOn() {
		if (busy) return;
		busy = true;
		try {
			await gettingStarted.skip("connect_world", true);
		} catch {
			/* moving on never waits on the server */
		}
		busy = false;
		onnext();
	}
</script>

{#snippet badges(items: StepPart[])}
	<div class="badges">
		{#each items as item (item.label)}
			<span class="badge" class:on={item.done}>
				<span class="light" aria-hidden="true"></span>
				{item.label}
				<span class="sr-only">{item.done ? "on" : "not yet"}</span>
			</span>
		{/each}
	</div>
{/snippet}

<!-- The payoff: what the device has already sent, the moment it has. -->
{#snippet payoff(line: string | null)}
	{#if line}
		<p class="payoff" in:fade={{ duration: 400 }}>{line}</p>
	{/if}
{/snippet}

{#snippet computerCard()}
	<article class="card" class:paired={macOn} class:here={here === "mac" || here === "computer"}>
		<div class="card-head">
			<span class="glyph" aria-hidden="true"><Icon icon="ri:macbook-line" width="22" /></span>
			<div class="card-title">
				<h2>{here === "mac" ? "This Mac" : here === "computer" ? thisComputerLabel[0].toUpperCase() + thisComputerLabel.slice(1) : "Mac"}</h2>
				{#if here !== "computer"}<p>Messages, the apps you use, and the pages you read.</p>{/if}
			</div>
		</div>
		{#if here === "mac"}
			{#if mac?.running}
				<MacPermissions status={mac} onRecheck={recheckMac} />
			{/if}
			{@render payoff(setup.arrived("computer"))}
			<div class="card-act">
				{#if !mac?.running}
					<button type="button" class="setup-go" disabled={turningOn} onclick={turnOnMac}>
						{turningOn ? "Turning on…" : "Collect from this Mac"}
					</button>
					<p class="hint">Then Full Disk Access, which Messages and Safari history need, and Accessibility, which is optional.</p>
				{:else if macReady(mac)}
					<p class="done-line"><Icon icon="ri:check-line" width="15" /> On</p>
				{/if}
			</div>
		{:else if here === "computer"}
			<p class="hint">Collecting from a computer works on a Mac for now. You can still add your iPhone.</p>
		{:else}
			{#if setup.mac}
				<MacPermissions device={setup.mac} deniedOnly canOpen={false} />
				{@render payoff(setup.arrived("computer"))}
			{/if}
			<div class="card-act">
				{#if setup.mac}
					<p class="done-line"><Icon icon="ri:check-line" width="15" /> Paired</p>
				{:else}
					<button
						type="button"
						class="setup-go"
						class:quiet={phoneFirst}
						onclick={() => (pairing = { deviceType: "mac", displayName: "Mac" })}
					>
						Add your Mac
					</button>
				{/if}
			</div>
		{/if}
	</article>
{/snippet}

{#snippet phoneCard()}
	<article class="card" class:paired={phoneOn} class:here={phoneFirst}>
		<div class="card-head">
			<span class="glyph" aria-hidden="true"><Icon icon="ri:smartphone-line" width="22" /></span>
			<div class="card-title">
				<h2>{here === "iphone" ? `This ${phoneName}` : here === "phone" ? "This phone" : "iPhone"}</h2>
				<p>Where you go, how you sleep, and what the microphone hears.</p>
			</div>
		</div>
		{#if here === "iphone"}
			{@render badges(phoneParts)}
			{@render payoff(setup.arrived("phone"))}
			<div class="card-act">
				{#if phase === "consent" || phase === "starting"}
					<!-- The consent this one stream earns, as Settings asks it. -->
					<div class="consent" in:fade={{ duration: 200 }}>
						<p>
							The microphone stays on while your phone is with you. It records the sound of your day, and everyone in
							the room. Recordings go to your server, which sends them to an AI model to write them down.
						</p>
						<p>
							In some places, recording a conversation needs everyone's consent. That part is yours to honor.
						</p>
						<div class="consent-actions">
							<button type="button" class="setup-go" disabled={phase === "starting"} onclick={allowMic}>
								{phase === "starting" ? "Turning on…" : "Turn the microphone on"}
							</button>
							<button type="button" class="setup-past" onclick={() => (phase = "idle")}>Not now</button>
						</div>
					</div>
				{:else if !phoneAsked}
					<button type="button" class="setup-go" disabled={phase === "asking"} onclick={allowPhone}>
						{phase === "asking" ? "Asking…" : "Turn on Location and Health"}
					</button>
					<p class="hint">Your {phoneName} asks about each one in turn. Calendar, contacts and more can come later, in Settings.</p>
				{:else}
					{#if refused.length}
						<button type="button" class="setup-go quiet" onclick={openSettings}>Open Settings</button>
						<p class="hint">
							You turned off {refused.join(" and ")}. Turn {refused.length === 1 ? "it" : "them"} on in Settings, then
							come back here.
						</p>
					{/if}
					{#if !micOn && !micRefused}
						<button type="button" class="setup-go quiet" class:after={refused.length} onclick={() => (phase = "consent")}>
							Turn on the microphone
						</button>
					{:else if !refused.length}
						<p class="done-line"><Icon icon="ri:check-line" width="15" /> On</p>
					{/if}
					{#if health === "requested" && !healthArriving}
						<p class="hint">Health readings arrive within a few minutes. If none do, allow them in the Health app, under Apps.</p>
					{/if}
				{/if}
			</div>
		{:else if here === "phone"}
			<p class="hint">Collecting from a phone needs the Virtues app for iPhone. Once it's installed, pair it with your server here.</p>
			<div class="card-act">
				<button type="button" class="setup-go" onclick={() => (pairing = { deviceType: "ios", displayName: "iPhone" })}>
					Pair the app
				</button>
			</div>
		{:else}
			{#if setup.iphone}{@render badges(setup.phone)}{@render payoff(setup.arrived("phone"))}{/if}
			<div class="card-act">
				{#if setup.iphone}
					<p class="done-line"><Icon icon="ri:check-line" width="15" /> Paired</p>
					{#if phoneSilent}
						<p class="hint">Nothing has arrived yet. Open Virtues on your iPhone and turn on Location and Health there.</p>
					{/if}
				{:else}
					<button
						type="button"
						class="setup-go"
						class:quiet={here === "mac"}
						onclick={() => (pairing = { deviceType: "ios", displayName: "iPhone" })}
					>
						Add your iPhone
					</button>
				{/if}
			</div>
		{/if}
	</article>
{/snippet}

<StepFrame {eyebrow} title="Add your devices" subtitle="Your iPhone brings where you went, and a Mac brings your messages. Each sends its part of your day to your server.">
	<div class="cards">
		{#if phoneFirst}
			{@render phoneCard()}
			{@render computerCard()}
		{:else}
			{@render computerCard()}
			{@render phoneCard()}
		{/if}
	</div>

	{#if error}
		<p class="error" role="alert">{error}</p>
	{/if}

	<p class="more">You can add accounts like Google and your bank after setup, in Sources.</p>

	{#snippet actions()}
		<!-- Until something is on, the card buttons ARE the way forward; a
		     greyed way forward beside them only said "not yet". The label
		     names the next step still to do, like every step's does. -->
		{#if anything}
			<button type="button" class="setup-go" disabled={busy} onclick={moveOn}>
				{intoLabel(setup.upNext("connections"))}
				<Icon icon="ri:arrow-right-line" width="16" />
			</button>
		{:else if macHalfway}
			<button type="button" class="setup-past" disabled={busy} onclick={moveOn}>Go on without Messages and Safari history</button>
		{:else}
			<button type="button" class="setup-past" disabled={busy} onclick={moveOn}>Skip for now</button>
		{/if}
	{/snippet}
</StepFrame>

{#if pairing}
	<DevicePairModal
		deviceType={pairing.deviceType}
		displayName={pairing.displayName}
		open={true}
		onClose={() => (pairing = null)}
		onSuccess={() => {
			pairing = null;
			void setup.refresh();
		}}
	/>
{/if}

<style>
	.cards {
		display: grid;
		grid-template-columns: 1fr 1fr;
		/* Each card as tall as it is: stretched to its neighbor's height, a
		   short card's button sat far below its text. */
		align-items: start;
		gap: 1rem;
	}
	@media (max-width: 640px) {
		.cards {
			grid-template-columns: 1fr;
		}
	}

	.card {
		display: flex;
		flex-direction: column;
		gap: 1.1rem;
		padding: 1.25rem 1.25rem 1.1rem;
		border-radius: 12px;
		outline: 1px solid color-mix(in srgb, var(--color-foreground) 9%, transparent);
		outline-offset: -1px;
		background: var(--color-surface-overlay, var(--color-surface));
	}
	.card.paired {
		outline: 1px solid color-mix(in srgb, var(--color-success) 35%, var(--color-border));
		outline-offset: -1px;
	}

	.card-head {
		display: flex;
		gap: 0.85rem;
		align-items: flex-start;
	}
	.glyph {
		flex: none;
		display: grid;
		place-items: center;
		width: 40px;
		height: 40px;
		border-radius: 12px;
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}
	.card-title h2 {
		margin: 0;
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		font-size: 1.35rem;
		line-height: 1.2;
		color: var(--color-foreground);
	}
	.card-title p {
		margin: 0.3rem 0 0;
		font-size: 14px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.badges {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}
	.badge {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 4px 8px;
		border-radius: 999px;
		font-size: 13px;
		color: var(--color-foreground-muted);
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
		transition:
			color 0.5s ease,
			background 0.5s ease;
	}
	.light {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--color-foreground) 18%, transparent);
		transition: background 0.5s ease;
	}
	/* Lit: the stream is on. The light comes on the way a lamp does: a glow
	   first, then the dot. */
	.badge.on {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-success) 8%, transparent);
		outline: 1px solid color-mix(in srgb, var(--color-success) 30%, transparent);
		outline-offset: -1px;
	}
	.badge.on .light {
		background: var(--color-success);
		outline: 3px solid color-mix(in srgb, var(--color-success) 18%, transparent);
		animation: lamp 900ms ease-out 1;
	}
	@keyframes lamp {
		0% {
		}
		100% {
			outline: 3px solid color-mix(in srgb, var(--color-success) 18%, transparent);
		}
	}

	.card-act {
		margin-top: auto;
	}
	.card-act .setup-go {
		font-size: 14px;
		padding: 0.6rem 1.15rem;
	}
	/* The second device's button is outlined, so the page keeps one filled
	   thing that reads first: the device in hand, or in a browser the Mac,
	   which holds the most. */
	.card-act :global(.setup-go.quiet) {
		background: transparent;
		color: var(--color-foreground);
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
	}
	.card-act .after {
		margin-top: 0.75rem;
	}
	.done-line {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		margin: 0;
		font-size: 14px;
		color: var(--color-success);
	}

	.consent p {
		margin: 0 0 0.75rem;
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground);
	}
	.consent-actions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 12px 16px;
	}

	.payoff {
		margin: -0.4rem 0 0;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-success);
	}
	.hint {
		margin: 0.6rem 0 0;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}
	.error {
		margin: 1rem 0 0;
		font-size: 14px;
		color: var(--color-error);
	}

	.more {
		margin: 1.5rem 0 0;
		text-align: center;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}

	@media (prefers-reduced-motion: reduce) {
		.badge.on .light {
			animation: none;
		}
	}
</style>
