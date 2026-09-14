<!--
	Step 3 — Connect your world: sources, this Mac, the phone. ConnectWorld
	ported whole (it carries the pair modal's QR hand-off for the phone). On
	the phone itself, the native permissions flow opens from here instead of
	as a screen mounted before the room. A collector running with a denied
	permission is said above the sources, never hidden behind the check.
-->
<script lang="ts">
	import ConnectWorld from "$lib/components/onboarding/document/ConnectWorld.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";
	import { GETTING_STARTED_CHAT_ID } from "./getting-started";
	import StepCard from "./StepCard.svelte";

	interface Props {
		forceOpen?: boolean;
	}
	let { forceOpen = false }: Props = $props();
	const detail = $derived(gettingStarted.step("connect_world")?.detail ?? null);
</script>

<StepCard
	step="connect_world"
	title="Connect your world"
	what="Your accounts, this computer, your phone. Your server reads them from here on."
	skippable
	{forceOpen}
>
	{#if detail}
		<p class="degraded">{detail}</p>
	{/if}
	{#if mobileLayout.isMobile}
		<div class="phone">
			<p>This phone can keep your location, health, calendar and contacts on your server.</p>
			<button type="button" class="btn" onclick={() => mobileLayout.openOnboarding()}>Set up this phone</button>
		</div>
	{/if}
	<ConnectWorld
		onConnected={() => void gettingStarted.refresh()}
		onDeviceReady={() => void gettingStarted.refresh()}
		next={`/chat/${GETTING_STARTED_CHAT_ID}`}
	/>
</StepCard>

<style>
	.degraded {
		margin: 0 0 0.75rem;
		font-size: 0.875rem;
		color: var(--color-error, #9a2b2e);
	}
	.phone {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		margin: 0 0 0.875rem;
		padding: 0.625rem 0.75rem;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}
	.phone p {
		margin: 0;
	}
	.btn {
		font: inherit;
		font-size: 0.875rem;
		padding: 0.35rem 0.8rem;
		border-radius: 6px;
		border: 1px solid var(--color-foreground);
		background: var(--color-foreground);
		color: var(--color-background);
		cursor: pointer;
		flex: none;
	}
</style>
