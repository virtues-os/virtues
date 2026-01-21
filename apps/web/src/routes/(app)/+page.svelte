<script lang="ts">
	/**
	 * This page now acts as a bootstrap component for the tab system.
	 * When navigated to with a conversationId, it opens that conversation in a tab.
	 * The actual chat UI is rendered by ChatView.svelte within the TabContainer.
	 */
	import { windowTabs } from '$lib/stores/windowTabs.svelte';
	import { onMount } from 'svelte';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	onMount(() => {
		// Check if we have a specific conversationId from the URL
		if (data.conversationId) {
			// Look for an existing tab with this conversation
			const existingTab = windowTabs.tabs.find(
				(t) => t.conversationId === data.conversationId
			);

			if (existingTab) {
				// Tab exists - just activate it
				windowTabs.setActiveTab(existingTab.id);
			} else {
				// Create a new tab for this conversation
				windowTabs.openTab({
					type: 'chat',
					label: data.isNew ? 'New Chat' : 'Chat',
					route: data.isNew ? '/' : `/?conversationId=${data.conversationId}`,
					conversationId: data.conversationId,
					icon: 'ri:chat-1-line'
				});
			}
		}
	});
</script>

<!--
	This component doesn't render anything visible.
	The TabContainer in +layout.svelte handles all chat rendering.
-->
