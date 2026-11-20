<script lang="ts">
	/**
	 * The two sides of a site's page: the site, and its options (when it
	 * runs, what it may spend, its runs, its prompt). Both are the same tab.
	 * Switching replaces the tab's place in history, so Back leaves the site
	 * instead of flipping the switch.
	 */
	import Segmented from '$lib/components/Segmented.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { optionsRoute, siteRoute } from '$lib/applets/sites.svelte';

	let {
		tabId,
		appletId,
		name,
		hasView,
		showing
	}: {
		tabId: string;
		appletId: string;
		name: string;
		/** A site with no view shows what it made on its own page. */
		hasView: boolean;
		showing: 'site' | 'options';
	} = $props();

	const OPTIONS = [
		{ value: 'site', label: 'Site' },
		{ value: 'options', label: 'Options' }
	] as const;

	function show(next: 'site' | 'options') {
		if (next === showing) return;
		const route = next === 'options' ? optionsRoute(appletId) : hasView ? siteRoute(appletId) : `/applet/${appletId}`;
		windowShellStore.retarget(tabId, route, name);
	}
</script>

<Segmented value={showing} options={OPTIONS} label={`${name}: the site or its options`} onchange={show} />
