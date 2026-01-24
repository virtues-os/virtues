<script lang="ts">
	import { workspaceStore } from '$lib/stores/workspace.svelte';
	import TabContent from './TabContent.svelte';

	// Use $derived to ensure reactivity when store updates
	const tabs = $derived(workspaceStore.tabs);
	const activeTabId = $derived(workspaceStore.activeTabId);
</script>

<div class="tab-container">
	{#each tabs as tab (tab.id)}
		<TabContent {tab} active={tab.id === activeTabId} />
	{/each}
</div>

<style>
	.tab-container {
		flex: 1;
		position: relative;
		overflow: hidden;
		min-height: 0;
	}
</style>
