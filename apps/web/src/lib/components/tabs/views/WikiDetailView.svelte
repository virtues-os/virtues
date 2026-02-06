<script lang="ts">
	/**
	 * WikiDetailView - Tab wrapper for WikiContent
	 *
	 * This is a thin wrapper that passes tab data to the platform-agnostic
	 * WikiContent component and handles tab-specific concerns like label updates.
	 */
	import type { Tab } from '$lib/tabs/types';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { WikiContent } from '$lib/components/views';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Derive entityId from route
	// Route format: /person/person_abc123 → entityId = person_abc123
	// Route format: /day/day_2026-01-25 → entityId = day_2026-01-25
	const entityId = $derived.by(() => {
		// Extract the entityId (last path segment) from the route
		const match = tab.route.match(/^\/[a-z]+\/(.+)$/);
		return match?.[1];
	});

	function handleLabelChange(label: string) {
		spaceStore.updateTab(tab.id, { label });
	}
</script>

<WikiContent
	{entityId}
	{active}
	onLabelChange={handleLabelChange}
/>
