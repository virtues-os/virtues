<script lang="ts">
	/**
	 * History Redirect
	 * 
	 * This route is kept for backward compatibility.
	 * The tab system handles rendering via HistoryView.
	 */
	import { windowTabs } from '$lib/stores/windowTabs.svelte';
	import { onMount } from 'svelte';

	onMount(() => {
		windowTabs.openTabFromRoute('/history');
	});
</script>

<!-- Content is rendered by tab system -->
