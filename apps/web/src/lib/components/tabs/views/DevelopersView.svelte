<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import SubNav, { type SubNavItem } from '$lib/components/SubNav.svelte';
	import DeveloperSqlView from './DeveloperSqlView.svelte';
	import DeveloperTerminalView from './DeveloperTerminalView.svelte';
	import DeveloperLakeView from './DeveloperLakeView.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type SubTab = 'sql' | 'terminal' | 'lake';

	// Active sub-tab is derived from the route (SubNav owns the writing side).
	const subTab = $derived<SubTab>(
		(tab.route.match(/^\/developers\/(sql|terminal|lake)$/)?.[1] as SubTab) ?? 'sql'
	);

	// Legacy /virtues/{sql|terminal|lake} routes self-heal to /developers/*.
	$effect(() => {
		const legacy = tab.route.match(/^\/virtues\/(sql|terminal|lake)$/)?.[1];
		if (legacy) windowShellStore.updateTab(tab.id, { route: `/developers/${legacy}` });
	});

	const tabs: SubNavItem[] = [
		{ id: 'sql', label: 'SQL' },
		{ id: 'terminal', label: 'Terminal' },
		{ id: 'lake', label: 'Lake' }
	];
</script>

<div class="developers-view">
	<SubNav
		tabId={tab.id}
		route={tab.route}
		base="/developers"
		default="sql"
		items={tabs}
		ariaLabel="Developers sections"
	/>

	<main class="content">
		{#if subTab === 'sql'}
			<DeveloperSqlView {tab} {active} />
		{:else if subTab === 'terminal'}
			<DeveloperTerminalView {tab} {active} />
		{:else if subTab === 'lake'}
			<DeveloperLakeView {tab} {active} />
		{/if}
	</main>
</div>

<style>
	.developers-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.content {
		flex: 1;
		overflow: hidden;
		min-height: 0;
		display: flex;
		flex-direction: column;
	}
</style>
