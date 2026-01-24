<script lang="ts">
	/**
	 * WikiDetailView - Tab wrapper for WikiContent
	 * 
	 * This is a thin wrapper that passes tab data to the platform-agnostic
	 * WikiContent component and handles tab-specific concerns like label updates.
	 */
	import type { Tab } from '$lib/tabs/types';
	import { workspaceStore } from '$lib/stores/workspace.svelte';
	import { WikiContent } from '$lib/components/views';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	function handleLabelChange(label: string) {
		workspaceStore.updateTab(tab.id, { label });
	}
</script>

<WikiContent 
	slug={tab.slug} 
	{active} 
	onLabelChange={handleLabelChange}
/>
