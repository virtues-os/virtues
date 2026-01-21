<script lang="ts">
	import { windowTabs } from '$lib/stores/windowTabs.svelte';
	import TabContent from './TabContent.svelte';

	// Use $derived to ensure reactivity when store updates
	const tabs = $derived(windowTabs.tabs);
	const activeTabId = $derived(windowTabs.activeTabId);
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
