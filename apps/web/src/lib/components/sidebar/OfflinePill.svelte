<!--
	OfflinePill.svelte

	The sidebar's note that the server isn't answering, as a small pill above
	Settings. It goes away by itself when the server answers (reachability
	retries on its own).
-->
<script lang="ts">
	import { goto } from "$app/navigation";
	import Icon from "$lib/components/Icon.svelte";
	import { reachability } from "$lib/stores/reachability.svelte";

	let { collapsed = false }: { collapsed?: boolean } = $props();

	// The app's own copy stays up when the server goes away, and it carries
	// /reconnect, which tells this computer being offline from the server being
	// off, refused or moved. A browser on the LAN has no radio and no pairing
	// to repair, so it gets the words without the click.
	const canReconnect =
		typeof window !== "undefined" &&
		!!(window as unknown as { __VIRTUES_BACKEND_ORIGIN__?: string }).__VIRTUES_BACKEND_ORIGIN__;
</script>

{#if reachability.unreachable && !collapsed}
	{#if canReconnect}
		<button
			type="button"
			class="offline-pill"
			onclick={() => void goto("/reconnect")}
			title="Find out why and fix it"
		>
			<Icon icon="ri:wifi-off-line" width="13" />
			<span>Can't reach server</span>
		</button>
	{:else}
		<div class="offline-pill" role="status">
			<Icon icon="ri:wifi-off-line" width="13" />
			<span>Can't reach server</span>
		</div>
	{/if}
{/if}

<style>
	/* Neutral tokens: the server being asleep is ordinary, so it gets no
	   warning color. */
	.offline-pill {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		margin: 0 0 6px var(--sidebar-interactive-padding);
		padding: 3px 10px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-full);
		background: color-mix(in srgb, var(--wash-ink) 4%, transparent);
		font-size: 12px;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}

	button.offline-pill {
		cursor: pointer;
		transition: border-color var(--motion-hover-out) var(--ease-premium);
	}

	button.offline-pill:hover {
		border-color: var(--color-border-strong);
		color: var(--color-foreground);
	}

	button.offline-pill:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}
</style>
