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
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { PageContent } from '$lib/components/views';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Extract pageId from route (e.g., '/page/page_xyz' → 'page_xyz')
	const pageId = $derived(routeToEntityId(tab.route) ?? undefined);

	function handleLabelChange(label: string) {
		windowShellStore.updateTab(tab.id, { label });
	}

	function handleIconChange(icon: string | null) {
		windowShellStore.updateTab(tab.id, { icon: icon || undefined });
	}

	// A link inside the page opens in this tab, as any link does, so Back
	// returns to the page. Only the tab in front of its pane navigates: a
	// request from anywhere else must not rewrite whichever tab is in front.
	function handleNavigate(route: string) {
		const paneId = windowShellStore.findTabPane(tab.id);
		const pane = windowShellStore.panes.find((p) => p.id === paneId);
		if (!paneId || pane?.activeTabId !== tab.id) return;
		windowShellStore.openHere(route, { paneId });
	}
</script>

<PageContent
	{pageId}
	{active}
	onLabelChange={handleLabelChange}
	onIconChange={handleIconChange}
	onNavigate={handleNavigate}
/>
