<script lang="ts">
	/**
	 * Data Source Detail Redirect
	 * 
	 * This route is kept for backward compatibility.
	 * It redirects to the new tab-based URL format.
	 */
	import { windowTabs } from '$lib/stores/windowTabs.svelte';
	import { onMount } from 'svelte';
	import { page } from '$app/stores';

	const sourceId = $derived($page.params.id ?? '');

	onMount(() => {
		if (sourceId) {
			// Open in tab and let the tab system handle URL update
			windowTabs.openTabFromRoute(`/data/sources/${sourceId}`);
		}
	});
</script>

<!-- This page redirects to tab-based navigation. Content is empty. -->
