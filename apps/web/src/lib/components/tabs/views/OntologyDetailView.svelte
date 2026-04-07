<!--
	OntologyDetailView.svelte

	Full-page ontology data table for /ontologies/{name} route.
-->

<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import OntologyDataTable from '$lib/components/wiki/OntologyDataTable.svelte';

	interface Props {
		tab: Tab;
		active: boolean;
	}

	let { tab }: Props = $props();

	const ontologyName = $derived.by(() => {
		const match = tab.route.match(/^\/ontologies\/([a-z_]+)$/);
		return match?.[1] || '';
	});
</script>

<div class="ontology-detail-view">
	{#if ontologyName}
		<OntologyDataTable {ontologyName} />
	{:else}
		<p class="error">Invalid ontology route</p>
	{/if}
</div>

<style>
	.ontology-detail-view {
		max-width: 72rem;
		margin: 0 auto;
		padding: 2rem;
	}

	.error {
		color: var(--color-foreground-muted);
		text-align: center;
		padding: 3rem;
	}
</style>
