<!--
	OntologyIndexView.svelte

	Browse all available ontologies grouped by domain.
	Clicking an ontology navigates to /ontologies/{name} for full data table.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { getOntologiesOverview, type OntologyOverview } from '$lib/api/client';
	import { spaceStore } from '$lib/stores/space.svelte';
	import { Page, EmptyState, LoadingState, ErrorState } from '$lib';
	import Icon from '$lib/components/Icon.svelte';

	let ontologies = $state<OntologyOverview[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const DOMAIN_ICONS: Record<string, string> = {
		health: 'ri:heart-pulse-line',
		location: 'ri:map-pin-line',
		communication: 'ri:mail-line',
		calendar: 'ri:calendar-line',
		activity: 'ri:apps-line',
		content: 'ri:file-text-line',
		financial: 'ri:bank-card-line',
		app: 'ri:terminal-box-line',
	};

	function getDomainIcon(domain: string): string {
		return DOMAIN_ICONS[domain.toLowerCase()] ?? 'ri:database-2-line';
	}

	// Group by domain
	const grouped = $derived.by(() => {
		const groups: Record<string, OntologyOverview[]> = {};
		for (const o of ontologies) {
			const d = o.domain;
			if (!groups[d]) groups[d] = [];
			groups[d].push(o);
		}
		return Object.entries(groups).sort(([a], [b]) => a.localeCompare(b));
	});

	function formatName(name: string): string {
		return name
			.replace(/^(data_|app_)/, '')
			.replace(/_/g, ' ')
			.replace(/\b\w/g, c => c.toUpperCase());
	}

	function openOntology(name: string) {
		spaceStore.openTabFromRoute(`/ontologies/${name}`);
	}

	onMount(async () => {
		try {
			ontologies = await getOntologiesOverview();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load';
		} finally {
			loading = false;
		}
	});
</script>

<Page title="Ontologies" description="Browse your data by type" maxWidth="prose">
	{#if loading}
		<LoadingState message="Loading ontologies..." />
	{:else if error}
		<ErrorState message={error} />
	{:else if ontologies.length === 0}
		<EmptyState icon="ri:database-2-line" message="No data sources connected yet." />
	{:else}
		{#each grouped as [domain, items]}
			<div class="domain-group">
				<h2 class="domain-title">
					<Icon icon={getDomainIcon(domain)} width="16" />
					{domain}
				</h2>
				<div class="ontology-grid">
					{#each items as ont}
						<button
							class="ontology-card"
							onclick={() => openOntology(ont.name)}
							type="button"
						>
							<span class="card-name">{formatName(ont.name)}</span>
							<span class="card-count">{ont.record_count.toLocaleString()} records</span>
						</button>
					{/each}
				</div>
			</div>
		{/each}
	{/if}
</Page>

<style>
	.domain-group {
		margin-bottom: 2rem;
	}

	.domain-title {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		font-size: 0.875rem;
		font-weight: 600;
		color: var(--color-foreground-muted);
		text-transform: capitalize;
		margin: 0 0 0.75rem;
		letter-spacing: 0.02em;
	}

	.ontology-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
		gap: 0.75rem;
	}

	.ontology-card {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
		padding: 1rem 1.25rem;
		background: var(--color-background-secondary);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		cursor: pointer;
		text-align: left;
		font: inherit;
		color: inherit;
		transition: all 0.15s ease;
	}

	.ontology-card:hover {
		border-color: var(--color-border-strong);
		background: var(--color-background-hover);
	}

	.card-name {
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.card-count {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.loading-state,
	.error-state,
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		padding: 3rem 2rem;
		color: var(--color-foreground-muted);
	}

	.loading-state :global(svg) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}
</style>
