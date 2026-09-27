<!--
	Put the server on Wi-Fi, over an open Bluetooth link.

	THE CONTRACT ($lib/tauri/boxRadio.ts): this step takes a `BoxWifiLink`
	and nothing else, and never branches on its `purpose`. Setup opens it with
	the four words; a claimed server that moved opens the same step from its
	owner's device, with no dots around it.

	  scanning  the server's own scan; an empty list means "no networks
	            here", a failed scan throws
	  list      strongest first, plus joining a hidden network by name
	  password  PSK needs 8 or more characters; an enterprise network asks
	            for a username too; an open one joins straight away
	  joining   the stages as the server reports them

	`join-failed` (usually the password) keeps the link open, so it simply
	asks again. `not-found`, `timeout` and `failed` mean the link is gone
	(every call hangs up at its deadline): `onlost` sends the flow back to
	find the server.
-->
<script lang="ts">
	import { onMount, tick } from "svelte";
	import StepFrame from "../StepFrame.svelte";
	import { rise } from "../motion";
	import { BoxRadioError, type BoxWifiLink, type JoinStage, type WifiNetwork } from "$lib/tauri/boxRadio";

	let {
		link,
		onjoined,
		onlost,
	}: { link: BoxWifiLink; onjoined: (url: string) => void; onlost: () => void } = $props();

	type Phase = "scanning" | "list" | "password" | "joining";
	let phase = $state<Phase>("scanning");
	let networks = $state<WifiNetwork[]>([]);
	let net = $state<WifiNetwork | null>(null);
	let byName = $state(false);
	let ssid = $state("");
	let identity = $state("");
	let password = $state("");
	let stage = $state<JoinStage>("sent");
	let error = $state<string | null>(null);
	let gone = $state(false);
	let passEl = $state<HTMLInputElement | null>(null);

	const label = $derived(link.box.label);
	const sorted = $derived([...networks].sort((a, b) => b.signal - a.signal));
	const enterprise = $derived(!!net?.enterprise);
	// A listed WPA network needs 8 or more characters. One joined by name has
	// no known kind, so any password (or none) is sent as entered.
	const canJoin = $derived(
		byName
			? ssid.trim().length > 0
			: !!net &&
					(enterprise ? identity.trim().length > 0 && password.length > 0 : !net.secured || password.length >= 8),
	);

	function fail(e: unknown, back: Phase) {
		const code = e instanceof BoxRadioError ? e.code : "failed";
		error = e instanceof BoxRadioError ? e.message : "Something went wrong talking to your server. Try again.";
		if (code === "join-failed") {
			phase = "password";
			tick().then(() => passEl?.select());
		} else if (code === "not-found" || code === "timeout" || code === "failed" || code === "unavailable") {
			gone = true;
			phase = back;
		} else {
			phase = back;
		}
	}

	async function scan() {
		phase = "scanning";
		error = null;
		try {
			networks = await link.scan();
			phase = "list";
		} catch (e) {
			networks = [];
			fail(e, "list");
		}
	}

	async function pick(n: WifiNetwork | null) {
		net = n;
		byName = !n;
		ssid = n?.ssid ?? "";
		password = "";
		identity = "";
		error = null;
		if (n && !n.secured) {
			await join();
			return;
		}
		phase = "password";
		await tick();
		passEl?.focus();
	}

	async function join() {
		const name = byName ? ssid.trim() : (net?.ssid ?? "");
		if (!name) return;
		phase = "joining";
		stage = "sent";
		error = null;
		try {
			const { url } = await link.join(
				{ ssid: name, password, ...(enterprise ? { identity: identity.trim() } : {}) },
				(s) => (stage = s),
			);
			onjoined(url);
		} catch (e) {
			fail(e, "list");
		}
	}

	onMount(() => void scan());

	const bars = (signal: number) => (signal >= -55 ? 3 : signal >= -70 ? 2 : 1);
	const title = $derived(
		phase === "password"
			? byName
				? "Join a network by name"
				: `Join ${net?.ssid}`
			: phase === "joining"
				? `Connecting ${label}`
				: `Put ${label} on your Wi-Fi`,
	);
	const subtitle = $derived(
		phase === "scanning"
			? `${label} is looking for networks.`
			: phase === "joining"
				? stage === "sent"
					? "Sending the network to your server."
					: stage === "joining"
						? "Your server is joining. This can take a minute."
						: "Connected. Finishing up."
				: phase === "password"
					? enterprise
						? "This network asks for a username and a password."
						: byName
							? "Enter its name exactly, and its password if it has one."
							: "Enter the Wi-Fi password."
					: networks.length === 0
						? "Your server can't see any networks. Move it closer to your router, or plug in ethernet and look again."
						: "Choose the network your server should use.",
	);
</script>

<StepFrame {title} {subtitle}>
	{#if phase === "scanning" || phase === "joining"}
		<div class="waiting" in:rise aria-hidden="true">
			<svg viewBox="0 0 24 24" width="40" height="40">
				<circle cx="4.5" cy="18" r="2.85" />
				<circle cx="19.5" cy="18" r="2.85" />
				<circle cx="12" cy="5" r="2.85" />
			</svg>
		</div>
	{:else if phase === "list" && !gone}
		<ul class="nets" in:rise>
			{#each sorted as n (n.ssid)}
				<li>
					<button type="button" class="net" onclick={() => pick(n)}>
						<span class="ssid">{n.ssid}</span>
						<span class="meta">
							{#if n.secured}<svg class="lock" viewBox="0 0 12 12" width="12" height="12" aria-label="Needs a password"
									><rect x="2" y="5.5" width="8" height="5.5" rx="1" /><path d="M4 5.5V4a2 2 0 0 1 4 0v1.5" /></svg
								>{/if}
							<svg class="bars" viewBox="0 0 14 12" width="14" height="12" aria-label="Signal {bars(n.signal)} of 3">
								{#each [1, 2, 3] as b}
									<rect x={(b - 1) * 5} y={12 - b * 4} width="3.5" height={b * 4} rx="1" class:on={b <= bars(n.signal)} />
								{/each}
							</svg>
						</span>
					</button>
				</li>
			{/each}
		</ul>
	{:else if phase === "password"}
		<form class="fields" onsubmit={(e) => (e.preventDefault(), canJoin && join())} in:rise>
			{#if byName}
				<input class="setup-field" type="text" autocapitalize="off" autocomplete="off" spellcheck="false" placeholder="Network name" aria-label="Network name" bind:value={ssid} />
			{/if}
			{#if enterprise}
				<input class="setup-field" type="text" autocapitalize="off" autocomplete="username" placeholder="Username" aria-label="Username" bind:value={identity} />
			{/if}
			<input
				class="setup-field"
				type="password"
				autocomplete="off"
				placeholder={byName ? "Password, if it has one" : "Password"}
				aria-label="Password"
				bind:this={passEl}
				bind:value={password}
				oninput={() => (error = null)}
			/>
		</form>
	{/if}
	<p class="note" class:error role={error ? "alert" : undefined}>{error ?? " "}</p>

	{#snippet actions()}
		{#if gone}
			<button type="button" class="setup-go" onclick={onlost}>Find your server again</button>
		{:else if phase === "list"}
			<button type="button" class={networks.length ? "setup-past" : "setup-go"} onclick={scan}>Look again</button>
			<button type="button" class="setup-past" onclick={() => pick(null)}>Join a network by name</button>
		{:else if phase === "password"}
			<button type="button" class="setup-go" onclick={join} disabled={!canJoin}>Join</button>
			<button type="button" class="setup-past" onclick={() => ((phase = "list"), (error = null))}>Choose another network</button>
		{/if}
	{/snippet}
</StepFrame>

<style>
	.waiting {
		display: flex;
		justify-content: center;
		padding: 24px 0;
	}
	.waiting circle {
		fill: var(--color-foreground);
		animation: seek 1.4s var(--m-ease) infinite;
	}
	.waiting circle:nth-child(2) {
		animation-delay: 0.18s;
	}
	.waiting circle:nth-child(3) {
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

	.nets {
		display: grid;
		max-width: 24rem;
		margin: 0 auto;
		padding: 0;
		list-style: none;
		border-top: 1px solid var(--color-border);
	}
	.net {
		display: flex;
		width: 100%;
		min-height: 52px;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 0 4px;
		border: 0;
		border-bottom: 1px solid var(--color-border);
		background: transparent;
		font: inherit;
		font-size: 16px;
		color: var(--color-foreground);
		text-align: left;
		cursor: pointer;
		transition: background var(--m-quick) ease;
	}
	.net:hover {
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}
	.net:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
	}
	.ssid {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.meta {
		display: flex;
		align-items: center;
		gap: 8px;
		flex: none;
	}
	.lock {
		fill: none;
		stroke: var(--color-foreground-muted);
		stroke-width: 1.2;
	}
	.bars rect {
		fill: color-mix(in srgb, var(--color-foreground) 18%, transparent);
	}
	.bars rect.on {
		fill: var(--color-foreground-muted);
	}

	.fields {
		display: grid;
		justify-items: center;
		gap: 20px;
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
