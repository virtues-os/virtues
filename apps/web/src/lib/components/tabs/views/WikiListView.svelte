<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { PersonTable, PlaceTable, OrganizationTable, ThingTable } from '$lib/components/wiki';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Determine which component to render based on wikiCategory
	const category = $derived(tab.wikiCategory || 'people');
</script>

<div class="wiki-list-view">
	<header class="page-header">
		<h1>{tab.label}</h1>
	</header>

	{#if category === 'people'}
		<PersonTable />
	{:else if category === 'places'}
		<PlaceTable />
	{:else if category === 'organizations' || category === 'orgs'}
		<OrganizationTable />
	{:else if category === 'things'}
		<ThingTable />
	{:else}
		<div class="placeholder">
			<p>Unknown category: {category}</p>
		</div>
	{/if}
</div>

<style>
	.wiki-list-view {
		width: 100%;
		padding: 1.5rem 0;
		height: 100%;
		overflow-y: auto;
	}

	.page-header {
		margin-bottom: 1rem;
		padding: 0 2rem;
	}

	.page-header h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0;
		letter-spacing: -0.02em;
	}

	.placeholder {
		padding: 2rem;
		text-align: center;
		color: var(--color-foreground-muted);
	}
</style>
