<!--
	Setup, before the server: find it on the network, or take its address,
	and pair with the code `virtues pair` printed.

	The beats:

	  scan      on an iPhone, first: scan the QR code the installer printed.
	            The camera is the connect page's (`prePair.joinExisting`),
	            which reads a server's code and a paired computer's alike.
	            The network search and the typed address are one tap away.
	  looking   the network search runs (`prePair.serversOnNetwork`)
	  choose    "Which server is yours?" when it finds more than one.
	            Nothing found: the main action is the address and code,
	            because some networks block the search while the server is
	            right there. The search keeps running underneath.
	  address   the address and code, typed or pasted from `virtues pair`
	            (one pasted line fills both), filled in for a server found
	            alone or picked from the list
	  pairing   pairing by address (`prePair.pairByAddress`)
	  paired    the server appears: its name, set large, and the dot that
	            fills, held for a moment before Setup moves on by itself.
	            Connecting is the peak of Setup, so it gets a beat of its
	            own instead of a page change.

	The code is six boxes in the serif, its figures lined up. Typing moves to the next box, Backspace on an empty box goes back,
	and pasting anywhere takes the whole line `virtues pair` printed
	(address and code) or just the six digits.
-->
<script lang="ts">
	import { onMount, tick } from "svelte";
	import { reducedMotion } from "$lib/stores/motion.svelte";
	import StepFrame from "../StepFrame.svelte";
	import { rise } from "../motion";
	import { prePair } from "../prepair.svelte";
	import type { NetworkServer } from "$lib/tauri/bridge";
	import { isIOS, isMacOS } from "$lib/utils/platform";
	import { pairError, serverLabel, splitPasted } from "../pairing";

	let {
		onpairing,
		onpaired,
	}: {
		/** Pairing has started: the route keeps this step on screen through
		 *  the paired moment, though the device is no longer unpaired. */
		onpairing?: () => void;
		onpaired: () => Promise<void> | void;
	} = $props();

	const here = isIOS ? "this iPhone" : isMacOS ? "this Mac" : "this device";

	type Phase = "scan" | "looking" | "choose" | "address" | "pairing" | "paired";
	/** An iPhone opens on the code the installer printed: scanning it is
	 *  what the person expects to do, and it needs nothing typed. */
	const scanFirst = isIOS && prePair.canJoinExisting;
	let phase = $state<Phase>(scanFirst ? "scan" : "looking");
	let network = $state<NetworkServer[]>([]);
	let error = $state<string | null>(null);
	let address = $state("");
	let digits = $state<string[]>(["", "", "", "", "", ""]);
	const code = $derived(digits.join(""));
	let addressEl = $state<HTMLInputElement | null>(null);
	let boxes = $state<(HTMLInputElement | null)[]>([]);
	const codeOk = $derived(/^\d{6}$/.test(code));
	/** Connect was pressed with something missing. */
	let tried = $state(false);
	/** What is missing, said once the code is whole or Connect was pressed:
	 *  a greyed button alone said nothing, and a hint that fires after the
	 *  first digit nags someone still typing. */
	const missing = $derived(
		!address.trim() && (codeOk || tried)
			? "Add the address your server printed, above the code."
			: tried && !codeOk
				? "The code has six digits."
				: null,
	);
	/** The name the server appears under once paired. */
	let pairedName = $state("Your server");
	let pairedAt = $state("");

	const still =
		reducedMotion();
	const nothing = $derived(network.length === 0);

	/** Bumped by anything that ends a search, so a stale round can't redraw. */
	let gen = 0;

	async function look() {
		const my = ++gen;
		phase = "looking";
		error = null;
		network = await prePair.serversOnNetwork();
		if (my !== gen) return;
		// One server on the network: straight to its code.
		if (network.length === 1) {
			void toAddress(network[0].origin);
			return;
		}
		phase = "choose";
		void keepLooking(my);
	}

	/** While nothing is found, search again every few seconds, so a server
	 *  that comes up appears here without anyone asking. */
	async function keepLooking(my: number) {
		while (my === gen && phase === "choose" && nothing) {
			await new Promise((r) => setTimeout(r, 4000));
			if (my !== gen || phase !== "choose" || !nothing) return;
			network = await prePair.serversOnNetwork();
		}
	}

	/** The address and code form, filled in with `origin` when one was picked. */
	async function toAddress(origin = "") {
		gen++;
		error = null;
		address = origin;
		phase = "address";
		await tick();
		(origin ? boxes[0] : addressEl)?.focus();
	}

	/** Fill the boxes from a run of digits, starting at `from`, and put the
	 *  cursor after the last one filled. */
	function fillFrom(from: number, run: string) {
		const ds = run.replace(/\D/g, "").slice(0, 6 - from).split("");
		if (!ds.length) return;
		const next = [...digits];
		ds.forEach((d, i) => (next[from + i] = d));
		digits = next;
		boxes[Math.min(5, from + ds.length)]?.focus();
	}

	/** A whole line from `virtues pair` fills both fields, wherever it lands. */
	function takeLine(text: string): boolean {
		const line = splitPasted(text);
		if (!line) return false;
		address = line.address;
		digits = ["", "", "", "", "", ""];
		fillFrom(0, line.code);
		return true;
	}

	function onAddressInput() {
		error = null;
		takeLine(address);
	}

	function onBoxInput(i: number, e: Event) {
		error = null;
		const el = e.currentTarget as HTMLInputElement;
		let value = el.value;
		// Typing over a filled box: iOS often keeps the old digit (it ignores
		// the select on focus), so the box holds two. The new one is the one
		// that isn't the old one.
		if (value.length === 2 && digits[i] && value.includes(digits[i])) {
			value = value[0] === digits[i] ? value[1] : value[0];
		}
		if (value.length > 1) {
			// Autofill of a one-time code, or a fast typist: spread it out.
			fillFrom(i, value);
			el.value = digits[i];
			return;
		}
		const d = value.replace(/\D/g, "");
		const next = [...digits];
		next[i] = d;
		digits = next;
		el.value = d;
		if (d && i < 5) boxes[i + 1]?.focus();
	}

	function onBoxKey(i: number, e: KeyboardEvent) {
		if (e.key === "Backspace" && !digits[i] && i > 0) {
			e.preventDefault();
			const next = [...digits];
			next[i - 1] = "";
			digits = next;
			boxes[i - 1]?.focus();
		} else if (e.key === "ArrowLeft" && i > 0) {
			e.preventDefault();
			boxes[i - 1]?.focus();
		} else if (e.key === "ArrowRight" && i < 5) {
			e.preventDefault();
			boxes[i + 1]?.focus();
		} else if (e.key === "Enter") {
			e.preventDefault();
			void pairHere();
		}
	}

	function onBoxPaste(i: number, e: ClipboardEvent) {
		const text = e.clipboardData?.getData("text") ?? "";
		e.preventDefault();
		error = null;
		// A whole code fills from the first box wherever it lands.
		if (!takeLine(text)) fillFrom(text.replace(/\D/g, "").length >= 6 ? 0 : i, text);
	}

	/** What to call the server when it appears: the name the network search
	 *  reported, else a host name from the address, else "Your server". */
	function nameFor(origin: string): string {
		const found = network.find((s) => s.origin === origin);
		if (found) return serverLabel(found.name);
		try {
			const host = new URL(origin.includes("://") ? origin : `http://${origin}`).hostname;
			if (/^[\d.]+$/.test(host) || host.includes(":")) return "Your server";
			return serverLabel(host.replace(/\.local$/, ""));
		} catch {
			return "Your server";
		}
	}

	async function pairHere() {
		if (phase === "pairing") return;
		if (!address.trim() || !codeOk) {
			tried = true;
			(address.trim() ? boxes[digits.findIndex((d) => !d)] : addressEl)?.focus();
			return;
		}
		phase = "pairing";
		error = null;
		onpairing?.();
		try {
			await prePair.pairByAddress(address.trim(), code);
		} catch (e) {
			console.error("[setup] pairing by address failed", e);
			error = pairError(e, { address, device: here }).text;
			phase = "address";
			// The fields were disabled while pairing, so focus fell to the
			// page; put it back on the code.
			await tick();
			boxes[0]?.focus();
			return;
		}
		pairedName = nameFor(address.trim());
		pairedAt = address.trim().replace(/^https?:\/\//, "").replace(/\/+$/, "");
		phase = "paired";
		// Long enough to read the name; reduced motion keeps the time to read.
		await new Promise((r) => setTimeout(r, still ? 1400 : 1900));
		await onpaired();
	}

	onMount(() => {
		if (!scanFirst) void look();
		return () => gen++;
	});

	const title = $derived(
		phase === "paired"
			? undefined
			: phase === "scan"
				? "Scan your server's code"
			: phase === "address" || phase === "pairing"
			? "Connect to your server"
			: network.length > 1
				? "Which server is yours?"
				: "Find your server",
	);
	const subtitle = $derived(
		phase === "paired"
			? undefined
			: phase === "scan"
				? "The QR code it printed when it finished installing."
			: phase === "address" || phase === "pairing"
			? "Enter the address and six-digit code your server printed. Paste the whole line and both fill in."
			: phase === "looking"
				? `Looking on this network. If ${here} asks, allow it so Virtues can find your server.`
				: nothing
					? "Ran virtues pair on your server? Enter the address and code it printed."
					: "Choose it, then enter the code virtues pair printed.",
	);
</script>

<StepFrame {title} {subtitle}>
	{#if phase === "scan"}
		<button type="button" class="finder" onclick={() => prePair.joinExisting()} in:rise>
			<span class="corner tl" aria-hidden="true"></span>
			<span class="corner tr" aria-hidden="true"></span>
			<span class="corner bl" aria-hidden="true"></span>
			<span class="corner br" aria-hidden="true"></span>
			<span class="sweep" aria-hidden="true"></span>
			<span class="finder-label">Open the camera</span>
		</button>
	{:else if phase === "looking"}
		<div class="searching" in:rise aria-hidden="true">
			<svg viewBox="0 0 24 24" width="40" height="40">
				<circle cx="4.5" cy="18" r="2.85" />
				<circle cx="19.5" cy="18" r="2.85" />
				<circle cx="12" cy="5" r="2.85" />
			</svg>
		</div>
	{:else if phase === "choose" && !nothing}
		<ul class="servers" in:rise>
			{#each network as s (s.origin)}
				<li>
					<button type="button" class="server" onclick={() => toAddress(s.origin)}>
						<span class="name">{serverLabel(s.name)}</span>
						<span class="state">
							{s.claimed === true ? "Already set up, " : ""}{s.origin.replace(/^https?:\/\//, "")}
						</span>
					</button>
				</li>
			{/each}
		</ul>
	{:else if phase === "address" || phase === "pairing"}
		<form class="pairform" onsubmit={(e) => (e.preventDefault(), pairHere())} in:rise>
			<label class="field">
				<span>Address</span>
				<input
					class="addr"
					type="text"
					autocomplete="off"
					autocapitalize="off"
					spellcheck="false"
					placeholder="For example, http://192.168.1.20:8000"
					bind:this={addressEl}
					bind:value={address}
					disabled={phase === "pairing"}
					oninput={onAddressInput}
				/>
			</label>
			<div class="field">
				<span id="code-label">Code</span>
				<div class="boxes" role="group" aria-labelledby="code-label">
					{#each digits as d, i (i)}
						{#if i === 3}<span class="gap" aria-hidden="true"></span>{/if}
						<input
							class="box"
							class:filled={!!d}
							type="text"
							inputmode="numeric"
							autocomplete={i === 0 ? "one-time-code" : "off"}
							aria-label="Digit {i + 1} of 6"
							value={d}
							bind:this={boxes[i]}
							disabled={phase === "pairing"}
							oninput={(e) => onBoxInput(i, e)}
							onkeydown={(e) => onBoxKey(i, e)}
							onpaste={(e) => onBoxPaste(i, e)}
							onfocus={(e) => (e.currentTarget as HTMLInputElement).select()}
						/>
					{/each}
				</div>
				<span class="hint">A code works once, for a short while. For a new one, run <code>virtues pair</code> on your server.</span>
			</div>
			<button type="submit" hidden aria-hidden="true" tabindex="-1"></button>
		</form>
	{:else if phase === "paired"}
		<div class="appears" role="status">
			<span class="dot" aria-hidden="true"><span class="halo"></span></span>
			<p class="with" in:rise={{ delay: still ? 0 : 120 }}>{here[0].toUpperCase() + here.slice(1)} is now paired with</p>
			<h1 class="appeared" in:rise={{ delay: still ? 0 : 260, y: 12 }}>{pairedName}</h1>
			<p class="where" in:rise={{ delay: still ? 0 : 420 }}>
				Only the devices you pair can reach it.{#if pairedAt}<br /><span class="mono">{pairedAt}</span>{/if}
			</p>
		</div>
	{/if}
	{#if phase !== "paired"}
		<p class="note" class:error role={error ? "alert" : undefined} aria-live="polite">
			{error ?? (phase === "address" ? missing : null) ?? " "}
		</p>
	{/if}
	{#if phase === "choose" && isIOS && prePair.canJoinExisting}
		<!-- The phone's other way in: the code a paired computer shows, on the
		     connect page (prepair.joinExisting). -->
		<p class="join">
			Adding {here} to a server you already use?
			<button type="button" class="setup-past" onclick={() => prePair.joinExisting()}>Scan its code</button>
		</p>
	{/if}

	{#snippet actions()}
		{#if phase === "scan"}
			<button type="button" class="setup-past" onclick={look}>Find it on this network</button>
			<button type="button" class="setup-past" onclick={() => toAddress()}>Type the address and code</button>
		{:else if phase === "choose"}
			<button type="button" class={nothing ? "setup-go" : "setup-past"} onclick={() => toAddress()}
				>Enter address and code</button
			>
		{:else if phase === "address" || phase === "pairing"}
			<button
				type="button"
				class="setup-go"
				onclick={pairHere}
				disabled={phase === "pairing"}
				aria-disabled={!address.trim() || !codeOk}
				class:incomplete={!address.trim() || !codeOk}
			>
				{phase === "pairing" ? "Connecting…" : "Connect"}
			</button>
			<button type="button" class="setup-past" onclick={look} disabled={phase === "pairing"}>Find it on this network</button>
		{/if}
	{/snippet}
</StepFrame>

<style>
	.pairform {
		display: flex;
		flex-direction: column;
		gap: 22px;
		width: min(100%, 25rem);
		margin: 0 auto;
		text-align: left;
	}
	/* The address in the interface's face; the code in the serif, its
	   figures lined up the way the terminal prints them. */
	.addr {
		height: 48px;
		box-sizing: border-box;
		padding: 0 14px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		font: inherit;
		font-size: 15px;
		letter-spacing: 0.01em;
		color: var(--color-foreground);
		outline: 0 solid transparent;
		outline-offset: 2px;
		transition:
			border-color var(--m-quick) ease,
			outline var(--m-quick) ease;
	}
	/* Focus is a full-strength ring the width of two hairlines, so it reads
	   on every theme, not a tint that vanishes on a dark one. */
	.addr:focus,
	.box:focus {
		border-color: var(--color-primary);
		outline: 2px solid var(--color-primary);
		outline-offset: -1px;
	}
	.addr::placeholder {
		color: var(--color-foreground-subtle, var(--color-foreground-muted));
		opacity: 0.75;
	}
	.boxes {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.box {
		flex: 1;
		min-width: 0;
		height: 60px;
		box-sizing: border-box;
		padding: 0;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		font-size: 30px;
		font-variant-numeric: lining-nums tabular-nums;
		text-align: center;
		color: var(--color-foreground);
		caret-color: var(--color-primary);
		outline: 0 solid transparent;
		transition:
			border-color var(--m-quick) ease,
			outline var(--m-quick) ease,
			transform var(--m-base) var(--m-spring);
	}
	/* A digit lands: a touch of lift, then home. */
	.box.filled {
		animation: land var(--m-base) var(--m-spring);
	}
	@keyframes land {
		40% {
			transform: translateY(-2px) scale(1.04);
		}
	}
	.gap {
		flex: none;
		width: 10px;
		height: 1.5px;
		background: var(--color-border);
	}
	.hint {
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.hint code,
	.mono {
		font: inherit;
		font-weight: 500;
		color: var(--color-foreground);
	}

	/* Connect stays pressable while something is missing, so pressing it
	   can say what; it only looks quieter. */
	.incomplete {
		opacity: 0.55;
	}

	/* ── the camera, before it opens ── */
	/* A viewfinder on the sheet: the corners a camera draws, and a line
	   that sweeps, so the one tap reads as "point it at the code". */
	.finder {
		position: relative;
		display: grid;
		place-items: end center;
		width: min(100%, 260px);
		aspect-ratio: 1;
		margin: 0 auto;
		padding: 0 0 24px;
		border: 0;
		border-radius: var(--radius-xl);
		background: var(--color-foreground);
		color: var(--color-background);
		font: inherit;
		cursor: pointer;
		overflow: hidden;
		transition: transform var(--m-base) var(--m-spring);
	}
	.finder:active {
		transform: scale(0.98);
		transition-duration: 90ms;
	}
	.finder:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 3px;
	}
	.corner {
		position: absolute;
		width: 28px;
		height: 28px;
		border: 0 solid currentColor;
	}
	.corner.tl {
		left: 30px;
		top: 30px;
		border-left-width: 3px;
		border-top-width: 3px;
		border-top-left-radius: var(--radius-md);
	}
	.corner.tr {
		right: 30px;
		top: 30px;
		border-right-width: 3px;
		border-top-width: 3px;
		border-top-right-radius: var(--radius-md);
	}
	.corner.bl {
		left: 30px;
		bottom: 30px;
		border-left-width: 3px;
		border-bottom-width: 3px;
		border-bottom-left-radius: var(--radius-md);
	}
	.corner.br {
		right: 30px;
		bottom: 30px;
		border-right-width: 3px;
		border-bottom-width: 3px;
		border-bottom-right-radius: var(--radius-md);
	}
	.sweep {
		position: absolute;
		left: 40px;
		right: 40px;
		top: 20%;
		height: 2px;
		background: currentColor;
		opacity: 0.7;
		animation: sweep 2.6s ease-in-out infinite;
	}
	@keyframes sweep {
		50% {
			top: 74%;
		}
	}
	.finder-label {
		position: relative;
		font-size: 14px;
		font-weight: 500;
		letter-spacing: 0.01em;
	}

	/* ── the server appears ── */
	.appears {
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		padding: 8px 0 4px;
	}
	.dot {
		position: relative;
		width: 18px;
		height: 18px;
		margin-bottom: 20px;
		border-radius: 50%;
		background: var(--color-primary);
		outline: 6px solid color-mix(in srgb, var(--color-primary) 12%, transparent);
		animation: dot-in var(--m-slow) var(--m-spring) both;
	}
	.halo {
		position: absolute;
		inset: 0;
		border-radius: 50%;
		background: var(--color-primary);
		animation: halo 1.6s var(--m-ease) 0.2s both;
	}
	@keyframes dot-in {
		from {
			transform: scale(0.2);
			opacity: 0;
		}
	}
	@keyframes halo {
		from {
			transform: scale(1);
			opacity: 0.45;
		}
		to {
			transform: scale(3);
			opacity: 0;
		}
	}
	.with {
		margin: 0;
		font-size: 13px;
		letter-spacing: 0.02em;
		color: var(--color-foreground-muted);
	}
	.appeared {
		margin: 6px 0 0;
		font-family: var(--font-serif, Georgia, serif);
		font-weight: 400;
		font-size: clamp(2.75rem, 7vw, 4rem);
		line-height: 1.02;
		letter-spacing: -0.018em;
		color: var(--color-foreground);
	}
	.where {
		margin: 16px 0 0;
		font-size: 15px;
		line-height: 1.6;
		color: var(--color-foreground-muted);
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.field span {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.searching {
		display: flex;
		justify-content: center;
		padding: 24px 0;
	}
	.searching circle {
		fill: var(--color-foreground);
		animation: seek 1.4s var(--m-ease) infinite;
	}
	.searching circle:nth-child(2) {
		animation-delay: 0.18s;
	}
	.searching circle:nth-child(3) {
		animation-delay: 0.36s;
	}
	@keyframes seek {
		0%,
		100% {
			opacity: 0.25;
		}
		40% {
			opacity: 1;
		}
	}

	.servers {
		display: grid;
		gap: 8px;
		max-width: 24rem;
		margin: 0 auto;
		padding: 0;
		list-style: none;
	}
	.server {
		display: flex;
		width: 100%;
		min-height: 56px;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		padding: 12px 16px;
		border: 0;
		border-radius: var(--radius-xl);
		outline: 1px solid color-mix(in srgb, var(--color-foreground) 12%, transparent);
		outline-offset: -1px;
		background: transparent;
		font: inherit;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
		transition: background var(--m-quick) ease;
	}
	.server:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}
	.server:disabled {
		cursor: default;
		color: var(--color-foreground-muted);
	}
	.server:focus-visible {
		outline: 2px solid var(--color-primary);
	}
	.name {
		font-family: var(--font-serif-ui, var(--font-serif));
		font-size: 20px;
	}
	.state {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}

	.join {
		margin: 4px 0 0;
		text-align: center;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.note {
		margin: 12px 0 0;
		min-height: 1.5em;
		text-align: center;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
	.note.error {
		color: var(--color-error, var(--color-foreground));
	}
	@media (prefers-reduced-motion: reduce) {
		circle {
			animation: none !important;
			opacity: 0.6;
		}
		.box.filled,
		.dot,
		.halo,
		.sweep {
			animation: none;
		}
		.halo {
			opacity: 0;
		}
	}
</style>
