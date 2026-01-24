<script lang="ts">
	/**
	 * PageDetailView - Tab wrapper for PageContent
	 * 
	 * This is a thin wrapper that passes tab data to the platform-agnostic
	 * PageContent component and handles tab-specific concerns like label updates
	 * and navigation.
	 */
	import type { Tab } from '$lib/tabs/types';
	import { workspaceStore } from '$lib/stores/workspace.svelte';
	import { PageContent } from '$lib/components/views';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	function handleLabelChange(label: string) {
		workspaceStore.updateTab(tab.id, { label });
	}

	function handleNavigate(route: string) {
		// Close this tab and navigate to the new route
		workspaceStore.closeTab(tab.id);
		workspaceStore.openTabFromRoute(route);
	}
</script>

<PageContent 
	pageId={tab.pageId} 
	{active} 
	onLabelChange={handleLabelChange}
	onNavigate={handleNavigate}
/>
