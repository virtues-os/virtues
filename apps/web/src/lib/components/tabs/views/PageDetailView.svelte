<script lang="ts">
	/**
	 * PageDetailView - Tab wrapper for PageContent
	 *
	 * This is a thin wrapper that passes tab data to the platform-agnostic
	 * PageContent component and handles tab-specific concerns like label updates
	 * and navigation.
	 */
	import type { Tab } from '$lib/tabs/types';
	import { routeToEntityId } from '$lib/tabs/types';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { PageContent } from '$lib/components/views';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Extract pageId from route (e.g., '/page/page_xyz' → 'page_xyz')
	const pageId = $derived(routeToEntityId(tab.route));

	function handleLabelChange(label: string) {
		spaceStore.updateTab(tab.id, { label });
	}

	function handleIconChange(icon: string | null) {
		spaceStore.updateTab(tab.id, { icon: icon || undefined });
	}

	function handleNavigate(route: string) {
		// Close this tab and navigate to the new route
		spaceStore.closeTab(tab.id);
		spaceStore.openTabFromRoute(route);
	}
</script>

<PageContent
	{pageId}
	{active}
	onLabelChange={handleLabelChange}
	onIconChange={handleIconChange}
	onNavigate={handleNavigate}
/>
