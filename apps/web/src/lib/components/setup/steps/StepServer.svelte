<!--
	Setup, before the server: find it, and open Virtues hardware with its four
	words.

	Two searches at once: Bluetooth for Virtues hardware, over the app's radio
	bridge ($lib/tauri/boxRadio.ts), and the network for a server someone
	installed themselves (`prePair.serversOnNetwork`). A server on the network
	pairs on the connect page with the code `virtues pair` prints, and one
	found alone goes there straight away. The beats:

	  looking   both searches run
	  choose    "Which server?": hardware by the number its screen shows
	            (`label`, "Virtues 4812"), then servers on the network;
	            hardware that already has an owner is listed but not offered,
	            because opening a claimed server is its owner's recovery
	            path, not first setup.
	            Nothing found: the main action is the address and code,
	            because some networks block the search while the server is
	            right there. The search keeps running underneath.
	  waking    the old firmware probe, once, before any words are asked
	            for (`tryOpenWithoutWords`): a server that predates the
	            four words opens here, and there is nothing to save
	  words     the four words its screen shows
	  keep      the words ARE the recovery phrase (one name everywhere):
	            shown back, copied or written down, and ticked before going on

	Errors branch on `BoxRadioError.code`; their messages are already
	written for people and are shown as they are.
-->
<script lang="ts">
	import { onMount, tick } from "svelte";
	import StepFrame from "../StepFrame.svelte";
	import { rise } from "../motion";
	import { prePair } from "../prepair.svelte";
	import { boxRadio, BoxRadioError, boxLabel, type NearbyBox } from "$lib/tauri/boxRadio";
	import type { NetworkServer } from "$lib/tauri/bridge";
	import { isIOS, isMacOS } from "$lib/utils/platform";

	let { onnext }: { onnext: () => void } = $props();

	const here = isIOS ? "this iPhone" : isMacOS ? "this Mac" : "this device";

	type Phase = "looking" | "choose" | "waking" | "words" | "checking" | "keep";
	let phase = $state<Phase>(prePair.link ? "keep" : "looking");
	let found = $state<NearbyBox[]>([]);
	let network = $state<NetworkServer[]>([]);
	let chosen = $state<NearbyBox | null>(prePair.box);
	let words = $state("");
	let error = $state<string | null>(null);
	let saved = $state(prePair.wordsKept);
	let copied = $state(false);
	let wordsEl = $state<HTMLInputElement | null>(null);

	const open = $derived(found.filter((b) => b.state !== "needs-owner"));
	/** Network servers that aren't hardware already listed over Bluetooth. */
	const elsewhere = $derived(network.filter((s) => !found.some((b) => b.label === boxLabel(s.name))));
	const nothing = $derived(open.length === 0 && elsewhere.length === 0);
	/** "mango burly skull dough", "Mango-Burly-Skull-Dough": one shape. */
	const canonical = $derived(
		words
			.trim()
			.toLowerCase()
			.split(/[^a-z]+/)
			.filter(Boolean)
			.join("-"),
	);
	const fourWords = $derived(canonical.split("-").filter(Boolean).length === 4);
	const said = (e: unknown) =>
		e instanceof BoxRadioError ? e.message : "Something went wrong talking to your server. Try again.";

	/** One round of both searches. A failed Bluetooth scan is said only when
	 *  the network search found nothing either. */
	async function search(): Promise<void> {
		const [ble, lan] = await Promise.all([
			boxRadio.discover().then(
				(boxes) => ({ boxes, error: null }),
				(e: unknown) => ({ boxes: [] as NearbyBox[], error: said(e) }),
			),
			prePair.serversOnNetwork(),
		]);
		found = ble.boxes;
		network = lan;
		error = ble.error && lan.length === 0 ? ble.error : null;
	}

	/** Bumped by anything that ends a search, so a stale round can't redraw. */
	let gen = 0;

	async function look() {
		const my = ++gen;
		phase = "looking";
		await search();
		if (my !== gen) return;
		phase = "choose";
		// One server on the network and no hardware: straight to its code,
		// once. Coming back from the code screen means they want to choose.
		if (found.length === 0 && network.length === 1 && prePair.forwardOnce(network[0].origin)) {
			prePair.toCodeEntry(network[0]);
			return;
		}
		void keepLooking(my);
	}

	/** While nothing is found, search again every few seconds, so a server
	 *  that comes up appears here without anyone asking. */
	async function keepLooking(my: number) {
		while (my === gen && phase === "choose" && nothing) {
			await new Promise((r) => setTimeout(r, 4000));
			if (my !== gen || phase !== "choose" || !nothing) return;
			await search();
		}
	}

	async function choose(box: NearbyBox) {
		gen++;
		chosen = box;
		error = null;
		phase = "waking";
		const link = await boxRadio.tryOpenWithoutWords(box).catch(() => null);
		if (link) {
			prePair.opened(box, link, "");
			onnext();
			return;
		}
		phase = "words";
		await tick();
		wordsEl?.focus();
	}

	async function openWithWords() {
		if (!chosen || !fourWords || phase === "checking") return;
		phase = "checking";
		error = null;
		try {
			const link = await boxRadio.openSetup(chosen, canonical);
			prePair.opened(chosen, link, canonical);
			if (link.gated) phase = "keep";
			else onnext();
		} catch (e) {
			error = said(e);
			// Out of range or gone quiet: look again. Wrong words: ask again.
			if (e instanceof BoxRadioError && (e.code === "not-found" || e.code === "timeout")) {
				phase = "choose";
				void look();
			} else {
				phase = "words";
				await tick();
				wordsEl?.select();
			}
		}
	}

	async function copy() {
		try {
			await navigator.clipboard.writeText(prePair.words.replace(/-/g, " "));
			copied = true;
			saved = true;
		} catch {
			/* nothing to copy to; the tick box is the promise */
		}
	}

	function keepGoing() {
		prePair.wordsKept = true;
		onnext();
	}

	async function another() {
		await prePair.forget();
		chosen = null;
		words = "";
		saved = false;
		void look();
	}

	onMount(() => {
		if (!prePair.link) void look();
		return () => gen++;
	});

	const title = $derived(
		phase === "words" || phase === "checking"
			? "Enter the four words on its screen"
			: phase === "keep"
				? "Save your recovery phrase"
				: phase === "waking"
					? `Opening ${chosen?.label ?? "your server"}`
					: open.length + elsewhere.length > 1
						? "Which server is yours?"
						: "Find your server",
	);
	const subtitle = $derived(
		phase === "looking"
			? `Looking over Bluetooth and on this network. If ${here} asks, allow both so Virtues can find your server.`
			: phase === "choose"
				? nothing
					? "Ran virtues pair on your server? Enter the address and code it printed."
					: open.length === 0
						? "Found on this network. Choose it, then enter the code virtues pair printed."
						: open.length === 1 && elsewhere.length === 0
							? "Is this the number on your server's screen?"
							: "Choose the one whose number is on its screen."
				: phase === "waking"
					? "One moment."
					: phase === "keep"
						? `These four words are your recovery phrase. You need them if you ever reset your server, so keep them somewhere safe, away from ${here}.`
						: `${chosen?.label ?? "Your server"} shows them when it's new. After a reset, enter the recovery phrase you saved.`,
	);
</script>

<StepFrame {title} {subtitle}>
	{#if phase === "looking" || phase === "waking"}
		<div class="searching" in:rise aria-hidden="true">
			<svg viewBox="0 0 24 24" width="40" height="40">
				<circle cx="4.5" cy="18" r="2.85" />
				<circle cx="19.5" cy="18" r="2.85" />
				<circle cx="12" cy="5" r="2.85" />
			</svg>
		</div>
	{:else if phase === "choose" && nothing}
		<p class="hardware" in:rise>
			Setting up Virtues hardware? Make sure it has power and sits close by. It shows up here on its own.
		</p>
	{:else if phase === "choose"}
		<ul class="servers" in:rise>
			{#each found as b (b.id)}
				<li>
					<button
						type="button"
						class="server"
						disabled={b.state === "needs-owner"}
						onclick={() => choose(b)}
					>
						<span class="name">{b.label}</span>
						<span class="state">
							{b.state === "needs-owner"
								? "Already has an owner"
								: b.state === "online"
									? "Online, ready to set up"
									: "Ready to set up"}
						</span>
					</button>
				</li>
			{/each}
			{#each elsewhere as s (s.origin)}
				<li>
					<button type="button" class="server" onclick={() => prePair.toCodeEntry(s)}>
						<span class="name">{boxLabel(s.name)}</span>
						<span class="state">
							{s.claimed === true ? "Already set up, " : ""}{s.origin.replace(/^https?:\/\//, "")}
						</span>
					</button>
				</li>
			{/each}
		</ul>
	{:else if phase === "words" || phase === "checking"}
		<form class="one" onsubmit={(e) => (e.preventDefault(), openWithWords())} in:rise>
			<input
				class="setup-field"
				type="text"
				autocomplete="off"
				autocapitalize="off"
				spellcheck="false"
				placeholder="four words, in order"
				aria-label="Four words"
				bind:this={wordsEl}
				bind:value={words}
				disabled={phase === "checking"}
				oninput={() => (error = null)}
			/>
		</form>
	{:else if phase === "keep"}
		<div class="keep" in:rise>
			<ol class="words" aria-label="Your recovery phrase">
				{#each prePair.words.split("-") as w, i (i)}
					<li><span class="n">{i + 1}</span>{w}</li>
				{/each}
			</ol>
			<label class="saved">
				<input type="checkbox" bind:checked={saved} />
				I've saved my recovery phrase somewhere safe
			</label>
		</div>
	{/if}
	<p class="note" class:error role={error ? "alert" : undefined}>{error ?? " "}</p>
	{#if phase === "choose" && isIOS && prePair.canJoinExisting}
		<!-- The phone's other way in: the code a paired computer shows, on the
		     connect page (prepair.joinExisting). A computer joins by address
		     and code, which the main action already offers. -->
		<p class="join">
			Adding {here} to a server you already use?
			<button type="button" class="setup-past" onclick={() => prePair.joinExisting()}>Scan its code</button>
		</p>
	{/if}

	{#snippet actions()}
		{#if phase === "choose"}
			{#if open.length === 1 && elsewhere.length === 0}
				<button type="button" class="setup-go" onclick={() => choose(open[0])}>Set up {open[0].label}</button>
			{/if}
			<button
				type="button"
				class={nothing ? "setup-go" : "setup-past"}
				onclick={() => {
					gen++;
					prePair.toAddressEntry();
				}}>Enter address and code</button
			>
		{:else if phase === "words" || phase === "checking"}
			<button type="button" class="setup-go" onclick={openWithWords} disabled={!fourWords || phase === "checking"}>
				{phase === "checking" ? "Checking…" : "Continue"}
			</button>
			<button type="button" class="setup-past" onclick={another} disabled={phase === "checking"}>A different server</button>
		{:else if phase === "keep"}
			<button type="button" class="setup-go" onclick={keepGoing} disabled={!saved}>Continue</button>
			<button type="button" class="setup-past" onclick={copy}>{copied ? "Copied" : "Copy the phrase"}</button>
		{/if}
	{/snippet}
</StepFrame>

<style>
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

	.one {
		display: flex;
		justify-content: center;
	}

	.keep {
		display: grid;
		justify-items: center;
		gap: 24px;
	}
	.words {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 7em));
		gap: 12px 24px;
		margin: 0;
		padding: 0;
		list-style: none;
		font-family: var(--font-serif-ui, var(--font-serif));
		font-size: 24px;
	}
	.words .n {
		display: inline-block;
		width: 1.4em;
		font-family: var(--font-sans);
		font-size: 13px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
	}
	.saved {
		display: flex;
		max-width: 100%;
		text-align: left;
		align-items: center;
		gap: 8px;
		min-height: 44px;
		font-size: 14px;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.saved input {
		width: 16px;
		height: 16px;
		accent-color: var(--color-primary);
	}

	.hardware {
		max-width: 24rem;
		margin: 0 auto;
		text-align: center;
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
	}
</style>
