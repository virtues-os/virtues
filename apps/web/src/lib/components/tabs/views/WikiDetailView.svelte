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

	// Derive slug from route
	// Route format: /person/person_john-doe → slug = john-doe
	// Route format: /day/day_2026-01-25 → slug = 2026-01-25
	const slug = $derived.by(() => {
		// Extract entity ID from route, then get the slug portion after the prefix
		const match = tab.route.match(/^\/[a-z]+\/[a-z]+_(.+)$/);
		return match?.[1];
	});

	function handleLabelChange(label: string) {
		spaceStore.updateTab(tab.id, { label });
	}
</script>

<WikiContent
	{slug}
	{active}
	onLabelChange={handleLabelChange}
/>
