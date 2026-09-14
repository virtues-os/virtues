<!--
	One door, two labels. Before AI is connected it is hidden-ish and skips
	connecting AI — the box's only exit while it has no model, and it is
	meant to look like one. After, the app is open anyway, so the same door
	reads "come back to this later" and simply goes Home.
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import Icon from "$lib/components/Icon.svelte";
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	const connected = $derived(gettingStarted.aiConnected);
	let busy = $state(false);

	async function leave() {
		if (busy) return;
		if (!connected) {
			const ok = window.confirm(
				"Skip connecting AI? Your server will show its record but cannot answer, write up a day, or run this room until AI is connected in Settings. You can come back to this conversation any time.",
			);
			if (!ok) return;
			busy = true;
			try {
				await gettingStarted.skip("connect_ai", true);
			} finally {
				busy = false;
			}
		}
		void goto("/home");
	}
</script>

<button
	type="button"
	class="door"
	class:quiet={!connected}
	onclick={leave}
	disabled={busy}
	title={connected ? "Come back to this later" : "Dangerously skip onboarding"}
	aria-label={connected ? "Come back to this later" : "Dangerously skip onboarding"}
>
	<Icon icon="ri:door-open-line" width="16" />
	{#if connected}
		<span class="label">Come back to this later</span>
	{/if}
</button>

<style>
	.door {
		display: inline-flex;
		align-items: center;
		gap: 0.4rem;
		background: none;
		border: 1px solid transparent;
		border-radius: 6px;
		padding: 0.25rem 0.5rem;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
		flex: none;
	}
	.door:hover {
		color: var(--color-foreground);
		border-color: var(--color-border);
	}
	.door.quiet {
		opacity: 0.35;
	}
	.door.quiet:hover {
		opacity: 1;
	}
	.door:disabled {
		opacity: 0.4;
		cursor: default;
	}
</style>
