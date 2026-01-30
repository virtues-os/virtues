<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { PersonTable, PlaceTable, OrganizationTable } from '$lib/components/wiki';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Extract category from route (e.g., '/wiki/people' → 'people')
	const category = $derived.by(() => {
		const match = tab.route.match(/^\/wiki\/([a-z]+)$/);
		return match?.[1] || 'people';
	});
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
