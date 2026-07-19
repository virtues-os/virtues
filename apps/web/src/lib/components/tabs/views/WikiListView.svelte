<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import { PersonTable, PlaceTable, OrganizationTable } from '$lib/components/wiki';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	const entityType = $derived.by(() => {
		const match = tab.route.match(/^\/([a-z]+)$/);
		return match?.[1] || 'person';
	});

	const labels: Record<string, string> = {
		person: 'People',
		place: 'Places',
		org: 'Organizations',
	};
</script>

<Page title={labels[entityType] || tab.label} maxWidth="wide">
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
</Page>

<style>
	.placeholder {
		padding: 2rem;
		text-align: center;
		color: var(--color-foreground-muted);
	}
</style>
