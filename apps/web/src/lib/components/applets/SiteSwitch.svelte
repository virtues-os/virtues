<script lang="ts">
	/**
	 * The two sides of a site's or a job's page: the thing itself, and its
	 * options (when it runs, what it may spend, its runs, its prompt). Both
	 * are the same tab.
	 * Switching replaces the tab's place in history, so Back leaves the site
	 * instead of flipping the switch.
	 */
	import Segmented from '$lib/components/Segmented.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { homeRoute, isSiteId, optionsRoute } from '$lib/applets/routes';

	let {
		tabId,
		appletId,
		name,
		showing
	}: {
		tabId: string;
		appletId: string;
		name: string;
		showing: 'site' | 'options';
	} = $props();

	// A job is your server's own, so its first side is not called a site.
	const options = $derived([
		{ value: 'site' as const, label: isSiteId(appletId) ? 'Site' : 'Job' },
		{ value: 'options' as const, label: 'Options' }
	]);

	function show(next: 'site' | 'options') {
		if (next === showing) return;
		windowShellStore.retarget(tabId, next === 'options' ? optionsRoute(appletId) : homeRoute(appletId), name);
	}
</script>

<Segmented value={showing} {options} label={`${name}, or its options`} onchange={show} />
