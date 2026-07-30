<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import FaceFrame from '$lib/components/applets/FaceFrame.svelte';

	// Full-page applet view: nothing but the applet's face, filling the pane.
	// The id comes from the route `/applet/<id>/view`.
	let { tab }: { tab: Tab; active: boolean } = $props();

	const actionId = $derived(tab.route.match(/\/(applet_[^/]+)\/view/)?.[1] ?? null);
</script>

<div class="applet-view">
	{#if actionId}
		<FaceFrame {actionId} height="100%" />
	{:else}
		<div class="state"><p class="muted">Applet not found.</p></div>
	{/if}
</div>

<style>
	.applet-view {
		height: 100%;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}
	/* Let the face fill the whole pane — the frame itself carries the border. */
	.applet-view :global(iframe.face-frame) {
		flex: 1;
		border: none;
		border-radius: 0;
	}
	.state {
		display: grid;
		place-items: center;
		height: 100%;
	}
	.muted {
		color: var(--color-foreground-subtle);
		font-size: 0.875rem;
	}
</style>
