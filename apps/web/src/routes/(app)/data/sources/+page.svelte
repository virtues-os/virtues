<script lang="ts">
	/**
	 * Data Sources Redirect
	 * 
	 * This route is kept for backward compatibility.
	 * The tab system handles rendering via DataSourcesView.
	 */
	import { windowTabs } from '$lib/stores/windowTabs.svelte';
	import { onMount } from 'svelte';

	onMount(() => {
		windowTabs.openTabFromRoute('/data/sources');
	});
</script>

<!-- Content is rendered by tab system -->
