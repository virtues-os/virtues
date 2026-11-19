<script lang="ts">
	/**
	 * WikiDetailView - Tab wrapper for WikiContent
	 *
	 * This is a thin wrapper that passes tab data to the platform-agnostic
	 * WikiContent component and handles tab-specific concerns like label updates.
	 */
	import type { Tab } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { WikiContent } from '$lib/components/views';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Derive entityId from route
	// Route format: /person/person_abc123 → entityId = person_abc123
	// Route format: /day/day_2026-01-25 → entityId = day_2026-01-25
	const routeMatch = $derived(tab.route.match(/^\/([a-z]+)\/(.+)$/));
	const kind = $derived(routeMatch?.[1]);
	const entityId = $derived(routeMatch?.[2]);

	function handleLabelChange(label: string) {
		windowShellStore.updateTab(tab.id, { label });
	}
</script>

<WikiContent
	{kind}
	{entityId}
	{active}
	onLabelChange={handleLabelChange}
/>
