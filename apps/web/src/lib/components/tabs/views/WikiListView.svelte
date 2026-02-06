<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { PersonTable, PlaceTable, OrganizationTable } from '$lib/components/wiki';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Extract entity type from route (e.g., '/person' → 'person', '/place' → 'place')
	const entityType = $derived.by(() => {
		// Routes are /{type} format (e.g., /person, /place, /org)
		const match = tab.route.match(/^\/([a-z]+)$/);
		return match?.[1] || 'person';
	});

	// Map entity types to display labels
	const labels: Record<string, string> = {
		person: 'People',
		place: 'Places',
		org: 'Organizations',
	};
</script>

<div class="wiki-list-view">
	<header class="page-header">
		<h1>{labels[entityType] || tab.label}</h1>
	</header>

	{#if entityType === 'person'}
		<PersonTable />
	{:else if entityType === 'place'}
		<PlaceTable />
	{:else if entityType === 'org'}
		<OrganizationTable />
	{:else}
		<div class="placeholder">
			<p>Unknown entity type: {entityType}</p>
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
