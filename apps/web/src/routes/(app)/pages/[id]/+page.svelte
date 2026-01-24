<script lang="ts">
	/**
	 * Page Detail Redirect
	 * 
	 * This route is kept for backward compatibility.
	 * It redirects to the new tab-based URL format.
	 */
	import { windowTabs } from '$lib/stores/windowTabs.svelte';
	import { onMount } from 'svelte';
	import { page } from '$app/stores';

	const pageId = $derived($page.params.id ?? '');

	onMount(() => {
		if (pageId) {
			// Open in tab and let the tab system handle URL update
			windowTabs.openTabFromRoute(`/pages/${pageId}`);
		}
	});
</script>

<!-- This page redirects to tab-based navigation. Content is empty. -->
