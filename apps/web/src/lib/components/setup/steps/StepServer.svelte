<!--
	Setup, before the server: find it on the network, or take its address,
	and pair with the code `virtues pair` printed.

	The beats:

	  looking   the network search runs (`prePair.serversOnNetwork`)
	  choose    "Which server is yours?" when it finds more than one.
	            Nothing found: the main action is the address and code,
	            because some networks block the search while the server is
	            right there. The search keeps running underneath.
	  address   the address and code, typed or pasted from `virtues pair`
	            (one pasted line fills both), filled in for a server found
	            alone or picked from the list
	  pairing   pairing by address (`prePair.pairByAddress`)
-->
<script lang="ts">
	import { onMount, tick } from "svelte";
	import StepFrame from "../StepFrame.svelte";
	import { rise } from "../motion";
	import { prePair } from "../prepair.svelte";
	import type { NetworkServer } from "$lib/tauri/bridge";
	import { isIOS, isMacOS } from "$lib/utils/platform";
	import { pairError, serverLabel, splitPasted } from "../pairing";

	let { onpaired }: { onpaired: () => Promise<void> | void } = $props();

	const here = isIOS ? "this iPhone" : isMacOS ? "this Mac" : "this device";

	type Phase = "looking" | "choose" | "address" | "pairing";
	let phase = $state<Phase>("looking");
	let network = $state<NetworkServer[]>([]);
	let error = $state<string | null>(null);
	let address = $state("");
	let code = $state("");
	let addressEl = $state<HTMLInputElement | null>(null);
	let codeEl = $state<HTMLInputElement | null>(null);
	const codeOk = $derived(code.replace(/\s+/g, "").length === 6);
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
		(origin ? codeEl : addressEl)?.focus();
	}

	/** A whole line pasted from `virtues pair` fills both fields. */
	function onAddressInput() {
		error = null;
		const line = splitPasted(address);
		if (line) {
			address = line.address;
			code = line.code;
		}
	}

	async function pairHere() {
		if (!address.trim() || !codeOk || phase === "pairing") return;
		phase = "pairing";
		error = null;
		try {
			await prePair.pairByAddress(address.trim(), code);
		} catch (e) {
			console.error("[setup] pairing by address failed", e);
			error = pairError(e, { address, device: here }).text;
			phase = "address";
			return;
		}
		await onpaired();
	}

	onMount(() => {
		void look();
		return () => gen++;
	});

	const title = $derived(
		phase === "address" || phase === "pairing"
			? "Connect to your server"
			: network.length > 1
				? "Which server is yours?"
				: "Find your server",
	);
	const subtitle = $derived(
		phase === "address" || phase === "pairing"
			? "Enter the address and code your server printed. Pasting the whole line fills both. Lost the code? Run virtues pair on your server."
			: phase === "looking"
				? `Looking on this network. If ${here} asks, allow it so Virtues can find your server.`
				: nothing
					? "Ran virtues pair on your server? Enter the address and code it printed."
					: "Choose it, then enter the code virtues pair printed.",
	);
</script>

<StepFrame {title} {subtitle}>
	{#if phase === "looking"}
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
					class="setup-field"
					type="text"
					autocomplete="off"
					autocapitalize="off"
					spellcheck="false"
					placeholder="http://192.168.1.20:8000"
					bind:this={addressEl}
					bind:value={address}
					disabled={phase === "pairing"}
					oninput={onAddressInput}
				/>
			</label>
			<label class="field">
				<span>Code</span>
				<input
					class="setup-field"
					type="text"
					inputmode="numeric"
					autocomplete="one-time-code"
					placeholder="123 456"
					bind:this={codeEl}
					bind:value={code}
					disabled={phase === "pairing"}
					oninput={() => (error = null)}
				/>
			</label>
			<button type="submit" hidden aria-hidden="true" tabindex="-1"></button>
		</form>
	{/if}
	<p class="note" class:error role={error ? "alert" : undefined}>{error ?? " "}</p>
	{#if phase === "choose" && isIOS && prePair.canJoinExisting}
		<!-- The phone's other way in: the code a paired computer shows, on the
		     connect page (prepair.joinExisting). -->
		<p class="join">
			Adding {here} to a server you already use?
			<button type="button" class="setup-past" onclick={() => prePair.joinExisting()}>Scan its code</button>
		</p>
	{/if}

	{#snippet actions()}
		{#if phase === "choose"}
			<button type="button" class={nothing ? "setup-go" : "setup-past"} onclick={() => toAddress()}
				>Enter address and code</button
			>
		{:else if phase === "address" || phase === "pairing"}
			<button
				type="button"
				class="setup-go"
				onclick={pairHere}
				disabled={!address.trim() || !codeOk || phase === "pairing"}
			>
				{phase === "pairing" ? "Connecting…" : "Connect"}
			</button>
			<button type="button" class="setup-past" onclick={look} disabled={phase === "pairing"}>Search again</button>
		{/if}
	{/snippet}
</StepFrame>

<style>
	.pairform {
		display: flex;
		flex-direction: column;
		gap: 14px;
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
	}
</style>
