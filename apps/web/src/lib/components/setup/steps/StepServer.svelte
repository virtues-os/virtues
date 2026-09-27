<!--
	Setup, before the server: find it, and open it with its four words.

	Over the app's radio bridge only ($lib/tauri/boxRadio.ts), never the reach
	plugin directly. The beats:

	  looking   the radio looks for servers nearby
	  choose    "Which server?", by the name its screen shows (`label`);
	            one that already has an owner is listed but not offered,
	            because opening a claimed server is its owner's recovery
	            path, not first setup
	  waking    the old firmware probe, once, before any words are asked
	            for (`tryOpenWithoutWords`): a server that predates the
	            four words opens here, and there is nothing to save
	  words     the four words its screen shows
	  keep      the words ARE the recovery phrase (one name, copy.md):
	            shown back, copied or written down, and ticked before going on

	Errors branch on `BoxRadioError.code`; their messages are already
	written for people and are shown as they are.
-->
<script lang="ts">
	import { onMount, tick } from "svelte";
	import StepFrame from "../StepFrame.svelte";
	import { rise } from "../motion";
	import { prePair } from "../prepair.svelte";
	import { boxRadio, BoxRadioError, type NearbyBox } from "$lib/tauri/boxRadio";

	let { onnext }: { onnext: () => void } = $props();

	type Phase = "looking" | "choose" | "waking" | "words" | "checking" | "keep";
	let phase = $state<Phase>(prePair.link ? "keep" : "looking");
	let found = $state<NearbyBox[]>([]);
	let chosen = $state<NearbyBox | null>(prePair.box);
	let words = $state("");
	let error = $state<string | null>(null);
	let saved = $state(prePair.wordsKept);
	let copied = $state(false);
	let wordsEl = $state<HTMLInputElement | null>(null);

	const open = $derived(found.filter((b) => b.state !== "needs-owner"));
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

	async function look() {
		phase = "looking";
		error = null;
		try {
			found = await boxRadio.discover();
		} catch (e) {
			found = [];
			error = said(e);
		}
		phase = "choose";
	}

	async function choose(box: NearbyBox) {
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
	});

	const title = $derived(
		phase === "words" || phase === "checking"
			? "Enter the four words on its screen"
			: phase === "keep"
				? "Save your recovery phrase"
				: phase === "waking"
					? `Opening ${chosen?.label ?? "your server"}`
					: open.length > 1
						? "Which server is yours?"
						: "Find your server",
	);
	const subtitle = $derived(
		phase === "looking"
			? "Looking nearby. Keep this phone within a few steps of it."
			: phase === "choose"
				? open.length === 0
					? "This phone can't find a new server nearby. Make sure yours has power and sits close by, then look again."
					: open.length === 1
						? "Is this the name on your server's screen?"
						: "Choose the one whose name is on its screen."
				: phase === "waking"
					? "One moment."
					: phase === "keep"
						? "These four words are your recovery phrase. You need them if you ever reset your server, so keep them somewhere safe, away from this phone."
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

	{#snippet actions()}
		{#if phase === "choose"}
			{#if open.length === 1}
				<button type="button" class="setup-go" onclick={() => choose(open[0])}>Set up {open[0].label}</button>
			{/if}
			<button type="button" class={open.length === 0 ? "setup-go" : "setup-past"} onclick={look}>Look again</button>
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
		border-radius: 12px;
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
