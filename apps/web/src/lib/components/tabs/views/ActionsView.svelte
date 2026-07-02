<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import ActionsPanel from '$lib/components/actions/ActionsPanel.svelte';
	import TemplatesPanel from '$lib/components/actions/TemplatesPanel.svelte';
	import HistoryPanel from '$lib/components/actions/HistoryPanel.svelte';
	import SubNav, { type SubNavItem } from '$lib/components/SubNav.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type SubTab = 'actions' | 'templates' | 'history';

	// Active sub-tab is derived from the route (SubNav owns the writing side).
	const subTab = $derived<SubTab>(
		(tab.route.match(/^\/actions\/(actions|templates|history)$/)?.[1] as SubTab) ?? 'actions'
	);

	const tabs: SubNavItem[] = [
		{ id: 'actions', label: 'Actions' },
		{ id: 'templates', label: 'Templates' },
		{ id: 'history', label: 'History' }
	];
</script>

<div class="actions-view">
	<SubNav
		tabId={tab.id}
		route={tab.route}
		base="/actions"
		default="actions"
		items={tabs}
		ariaLabel="Actions sections"
	/>

	<main class="content">
		{#if subTab === 'actions'}
			<ActionsPanel />
		{:else if subTab === 'templates'}
			<TemplatesPanel />
		{:else if subTab === 'history'}
			<HistoryPanel />
		{/if}
	</main>
</div>

<style>
	.actions-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.content {
		flex: 1;
		overflow-y: auto;
		padding: 1.25rem 1.5rem 2rem;
		max-width: 1100px;
		width: 100%;
		margin: 0 auto;
	}
</style>
