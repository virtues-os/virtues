<script lang="ts">
	/**
	 * Wiki Detail Redirect
	 * 
	 * This route is kept for backward compatibility.
	 * It redirects to the new tab-based URL format.
	 */
	import { windowTabs } from '$lib/stores/windowTabs.svelte';
	import { onMount } from 'svelte';
	import { page } from '$app/stores';

	const slug = $derived($page.params.slug ?? '');

	onMount(() => {
		if (slug) {
			// Open in tab and let the tab system handle URL update
			windowTabs.openTabFromRoute(`/wiki/${slug}`);
		}
	});
</script>

<!-- This page redirects to tab-based navigation. Content is empty. -->
