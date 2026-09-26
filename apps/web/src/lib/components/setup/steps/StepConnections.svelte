<!--
	Step 5 - Connections. THIS DEVICE FIRST, then the other one.

	Setup runs inside the app, on a device that is already paired: pairing is
	how the person got here. So the step never asks the device in their hand
	to be "added". Its card comes first, marked paired, and its one action
	turns on what it collects:

	  - the Mac: the collector (a one-time token, then the app installs it),
	    then Full Disk Access (Messages, required) and Accessibility (what is on
	    screen, optional). macOS has no prompt for either, so the button opens
	    the right Settings pane and the badge lights when the daemon itself
	    reports the grant. The logic is CollectorPermissionCard's, which was
	    kept for exactly this step.
	  - the iPhone: location, health and audio, one system prompt each, in
	    order. Calendar, contacts and the rest stay in This device.

	The other device follows, as "Add your iPhone" or "Add your computer". In a
	plain browser there is no device in hand, so both cards are adds.

	BADGES ONLY ON A PAIRED DEVICE. Before pairing they read as tags or
	filters; after, they are a live checklist that lights as each thing
	arrives, which is the moment worth showing.

	Continuing is the step's acknowledgement (the server counts the step done
	once something is flowing AND the person has moved past it), so the way
	forward writes that, and "Skip for now" writes the same thing with nothing
	flowing, which the server records as set aside, not done.

	ACCOUNTS ARE NOT A STEP. Google, a bank and the rest live in Sources; this
	step names where, once.
-->
<script lang="ts">
	import { onDestroy, onMount } from "svelte";
	import { fade } from "svelte/transition";
	import { invoke } from "@tauri-apps/api/core";
	import Icon from "$lib/components/Icon.svelte";
	import DevicePairModal from "$lib/components/sources/DevicePairModal.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { isIOS, isMacOS } from "$lib/utils/platform";
	import * as api from "$lib/api/client";
	import {
		getCollectorStatus,
		installCollector,
		openAccessibilitySettings,
		openFullDiskAccess,
		type CollectorStatus,
	} from "$lib/tauri/bridge";
	import { setup, type StepPart } from "../setup.svelte";
	import StepFrame from "../StepFrame.svelte";

	let { eyebrow, onnext }: { eyebrow?: string; onnext: () => void } = $props();

	/** The device in hand. iOS first: an iPad reports a Mac platform. */
	const here: "mac" | "iphone" | null = isIOS ? "iphone" : isMacOS ? "mac" : null;

	let pairing = $state<{ deviceType: "ios" | "mac"; displayName: string } | null>(null);
	let busy = $state(false);
	let error = $state<string | null>(null);

	// ── this Mac ──────────────────────────────────────────────────────────
	let mac = $state<CollectorStatus | null>(null);
	let turningOn = $state(false);
	let poll: ReturnType<typeof setInterval> | null = null;
	let live: ReturnType<typeof setInterval> | null = null;

	async function readMac() {
		const s = await getCollectorStatus();
		if (s) mac = s;
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
				error = "The collector installed but didn't start. Try again, or check ~/.virtues/logs/collector.error.log.";
			}
			void setup.refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : "Couldn't turn on this Mac. Try again.";
		} finally {
			turningOn = false;
		}
	}

	const macParts = $derived<StepPart[]>([
		{ label: "Collector", done: !!mac?.running },
		{ label: "Full Disk Access", done: !!mac?.hasFullDiskAccess },
		{ label: "Accessibility", done: !!mac?.hasAccessibility },
	]);

	// ── this iPhone ───────────────────────────────────────────────────────
	type Stream = { key: string; label: string; enable: string; status?: string; on: boolean };
	let streams = $state<Stream[]>([
		{ key: "location", label: "Location", enable: "plugin:location-probe|start_probe", on: false },
		{ key: "health", label: "Health", enable: "plugin:health|enable", status: "plugin:health|status", on: false },
		{ key: "audio", label: "Audio", enable: "plugin:audio|enable", status: "plugin:audio|status", on: false },
	]);
	let allowing = $state(false);

	async function readPhone() {
		for (const s of streams) {
			if (s.status) {
				try {
					const st = await invoke<{ authorized?: boolean }>(s.status);
					if (st?.authorized) s.on = true;
				} catch {
					/* a missing plugin reads as off */
				}
			}
		}
		// Location has no status call: arriving points are the proof.
		const loc = setup.phone.find((p) => p.label === "Location");
		if (loc?.done) streams[0].on = true;
	}

	/** One system prompt per stream, in order. */
	async function allowPhone() {
		allowing = true;
		error = null;
		for (const s of streams) {
			if (s.on) continue;
			try {
				const res = await invoke<{ authorized?: boolean } | null>(s.enable);
				s.on = !(res && res.authorized === false);
			} catch {
				s.on = false;
			}
		}
		allowing = false;
		void setup.refresh();
	}

	const phoneParts = $derived<StepPart[]>(streams.map((s) => ({ label: s.label, done: s.on })));

	onMount(() => {
		// The badges light, and the counts rise, while the person watches.
		live = setInterval(() => void setup.refresh(), 5000);
		if (here === "mac") {
			void readMac();
			poll = setInterval(readMac, 2000);
		} else if (here === "iphone") {
			void readPhone();
		}
	});
	onDestroy(() => {
		if (poll) clearInterval(poll);
		if (live) clearInterval(live);
	});

	// ── what counts as connected ──────────────────────────────────────────
	const macOn = $derived(here === "mac" ? !!mac?.running : setup.computer[0].done);
	const phoneOn = $derived(here === "iphone" ? streams.some((s) => s.on) : setup.phonePaired);
	const anything = $derived(macOn || phoneOn);

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
	<article class="card" class:paired={macOn} class:here={here === "mac"}>
		<div class="card-head">
			<span class="glyph" aria-hidden="true"><Icon icon="ri:macbook-line" width="22" /></span>
			<div class="card-title">
				<h2>{here === "mac" ? "This Mac" : "Computer"}</h2>
				<p>Messages, the apps you use, and the pages you read.</p>
			</div>
		</div>
		{#if here === "mac"}
			{@render badges(macParts)}
			{@render payoff(setup.arrived("computer"))}
			<div class="card-act">
				{#if !mac?.running}
					<button type="button" class="setup-go" disabled={turningOn} onclick={turnOnMac}>
						{turningOn ? "Turning on…" : "Turn on this Mac"}
					</button>
				{:else if !mac.hasFullDiskAccess}
					<button type="button" class="setup-go" onclick={() => openFullDiskAccess()}>Open Full Disk Access</button>
					<p class="hint">Turn on Virtues Collector there. Your Mac reads Messages and keeps them on your server.</p>
				{:else if !mac.hasAccessibility}
					<button type="button" class="setup-go quiet" onclick={() => openAccessibilitySettings()}>
						Open Accessibility
					</button>
					<p class="hint">Optional. Turn on Virtues Collector there to add what's on your screen.</p>
				{:else}
					<p class="done-line"><Icon icon="ri:check-line" width="15" /> Collecting</p>
				{/if}
			</div>
		{:else}
			{#if macOn}{@render badges(setup.computer)}{@render payoff(setup.arrived("computer"))}{/if}
			<div class="card-act">
				{#if macOn}
					<p class="done-line"><Icon icon="ri:check-line" width="15" /> Paired</p>
				{:else}
					<button
						type="button"
						class="setup-go"
						class:quiet={here === "iphone"}
						onclick={() => (pairing = { deviceType: "mac", displayName: "computer" })}
					>
						Add your computer
					</button>
				{/if}
			</div>
		{/if}
	</article>
{/snippet}

{#snippet phoneCard()}
	<article class="card" class:paired={phoneOn} class:here={here === "iphone"}>
		<div class="card-head">
			<span class="glyph" aria-hidden="true"><Icon icon="ri:smartphone-line" width="22" /></span>
			<div class="card-title">
				<h2>{here === "iphone" ? "This iPhone" : "iPhone"}</h2>
				<p>Where you go, what you say out loud, and how you sleep.</p>
			</div>
		</div>
		{#if here === "iphone"}
			{@render badges(phoneParts)}
			{@render payoff(setup.arrived("phone"))}
			<div class="card-act">
				{#if streams.some((s) => !s.on)}
					<button type="button" class="setup-go" disabled={allowing} onclick={allowPhone}>
						{allowing ? "Asking…" : "Turn on this iPhone"}
					</button>
					<p class="hint">Your iPhone asks about each one. Calendar, contacts and more are in This device.</p>
				{:else}
					<p class="done-line"><Icon icon="ri:check-line" width="15" /> Collecting</p>
				{/if}
			</div>
		{:else}
			{#if phoneOn}{@render badges(setup.phone)}{@render payoff(setup.arrived("phone"))}{/if}
			<div class="card-act">
				{#if phoneOn}
					<p class="done-line"><Icon icon="ri:check-line" width="15" /> Paired</p>
				{:else}
					<button
						type="button"
						class="setup-go quiet"
						onclick={() => (pairing = { deviceType: "ios", displayName: "iPhone" })}
					>
						Add your iPhone
					</button>
				{/if}
			</div>
		{/if}
	</article>
{/snippet}

<StepFrame {eyebrow} title="Add your devices" subtitle="Each device sends its part of your day to your server.">
	<div class="cards">
		{#if here === "iphone"}
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
		     greyed "Build your timeline" beside them only said "not yet". -->
		{#if anything}
			<button type="button" class="setup-go" disabled={busy} onclick={moveOn}>
				Build your timeline
				<Icon icon="ri:arrow-right-line" width="16" />
			</button>
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
		transition: box-shadow 0.4s ease;
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
		padding: 4px 8px 4px 8px;
		border-radius: 999px;
		font-size: 13px;
		color: var(--color-foreground-muted);
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
		transition:
			color 0.5s ease,
			background 0.5s ease,
			box-shadow 0.5s ease;
	}
	.light {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--color-foreground) 18%, transparent);
		transition:
			background 0.5s ease,
			box-shadow 0.5s ease;
	}
	/* Lit: the stream has arrived. The light comes on the way a lamp does —
	   a glow first, then the dot. */
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
	   thing that reads first: the device in hand, or in a browser the
	   computer, which holds the most. */
	.card-act :global(.setup-go.quiet) {
		background: transparent;
		color: var(--color-foreground);
		outline: 1px solid var(--color-border);
		outline-offset: -1px;
	}
	.done-line {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		margin: 0;
		font-size: 14px;
		color: var(--color-success);
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
