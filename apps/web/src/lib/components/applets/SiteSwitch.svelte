<script lang="ts">
	/**
	 * The two sides of a site's, a skill's or a task's page: the thing
	 * itself, and its options (when it runs, what it may spend, its runs, its
	 * prompt). Both are the same tab. Switching replaces the tab's place in
	 * history, so Back leaves the page instead of flipping the switch.
	 */
	import Segmented from '$lib/components/Segmented.svelte';
	import type { Applet } from '$lib/api/client';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { KIND_WORD, kindOf, optionsOf, pageOf } from '$lib/applets/kind';

	let {
		tabId,
		applet,
		showing
	}: {
		tabId: string;
		applet: Applet;
		showing: 'site' | 'options';
	} = $props();

	// The first side is named for what the thing is: Site, Task or Skill.
	const options = $derived([
		{ value: 'site' as const, label: KIND_WORD[kindOf(applet)] },
		{ value: 'options' as const, label: 'Options' }
	]);

	function show(next: 'site' | 'options') {
		if (next === showing) return;
		windowShellStore.retarget(tabId, next === 'options' ? optionsOf(applet) : pageOf(applet), applet.name);
	}
</script>

<Segmented value={showing} {options} label={`${applet.name}, or its options`} onchange={show} />
