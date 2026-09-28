<!--
	The end of the first half: the account's grant, then pairing, over the
	same Bluetooth link the Wi-Fi went over.

	THE ORDER IS THE CONTRACT: the grant has to cross before `pair()`, which
	is why sign-in comes first in Setup. Pairing can take up to eighty seconds
	(the server may spend a minute linking before it answers), so nothing here
	times it out sooner; the link's own deadline does.

	A grant that fails is not a dead end: the server pairs without it, and the
	Subscription step links the account afterwards through the server.
-->
<script lang="ts">
	import { onMount } from "svelte";
	import StepFrame from "../StepFrame.svelte";
	import { rise } from "../motion";
	import { prePair, AccountError } from "../prepair.svelte";
	import { BoxRadioError } from "$lib/tauri/boxRadio";
	import { isIOS, isMacOS } from "$lib/utils/platform";

	let { onpaired, onlost }: { onpaired: () => void; onlost: () => void } = $props();

	let phase = $state<"linking" | "pairing" | "opening" | "grant-failed" | "pair-failed">("linking");
	let error = $state<string | null>(null);
	let gone = $state(false);

	const label = $derived(prePair.box?.label ?? "your server");
	const here = isIOS ? "this iPhone" : isMacOS ? "this Mac" : "this device";

	async function linkAccount() {
		phase = "linking";
		error = null;
		try {
			await prePair.grant();
		} catch (e) {
			error = e instanceof AccountError || e instanceof BoxRadioError ? e.message : null;
			phase = "grant-failed";
			return;
		}
		await pair();
	}

	async function pair() {
		phase = "pairing";
		error = null;
		try {
			await prePair.pair();
			phase = "opening";
			onpaired();
		} catch (e) {
			error =
				e instanceof BoxRadioError ? e.message : "Your server didn't finish pairing. Try again.";
			gone = e instanceof BoxRadioError && (e.code === "not-found" || e.code === "timeout");
			phase = "pair-failed";
		}
	}

	onMount(() => {
		if (prePair.account) void linkAccount();
		else void pair();
	});

	const title = $derived(
		phase === "opening"
			? `${here[0].toUpperCase()}${here.slice(1)} is paired`
			: phase === "grant-failed"
			? "Your account didn't reach your server"
			: phase === "pair-failed"
				? `${label} didn't finish pairing`
				: `Pairing ${here} with ${label}`,
	);
	const subtitle = $derived(
		phase === "linking"
			? "Handing your account to your server."
			: phase === "pairing"
				? "Exchanging keys. This can take up to a minute."
				: phase === "opening"
					? "Opening your server."
				: phase === "grant-failed"
					? "You can pair now and link your account in the next step."
					: gone
						? "It stopped answering. Make sure it's still on and close by, then find it again."
						: "Nothing on your server changed. Try again.",
	);
</script>

<StepFrame {title} {subtitle}>
	{#if phase === "linking" || phase === "pairing" || phase === "opening"}
		<div class="waiting" in:rise aria-hidden="true">
			<svg viewBox="0 0 24 24" width="40" height="40">
				<circle cx="4.5" cy="18" r="2.85" />
				<circle cx="19.5" cy="18" r="2.85" />
				<circle cx="12" cy="5" r="2.85" />
			</svg>
		</div>
	{/if}
	<p class="note" class:error role={error ? "alert" : undefined}>{error ?? " "}</p>

	{#snippet actions()}
		{#if phase === "grant-failed"}
			<button type="button" class="setup-go" onclick={pair}>Pair without it</button>
			<button type="button" class="setup-past" onclick={linkAccount}>Try again</button>
		{:else if phase === "pair-failed"}
			{#if gone}
				<button type="button" class="setup-go" onclick={onlost}>Find your server again</button>
			{:else}
				<button type="button" class="setup-go" onclick={pair}>Try again</button>
				<button type="button" class="setup-past" onclick={onlost}>Start over</button>
			{/if}
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
