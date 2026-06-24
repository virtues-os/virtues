<!--
	EntitiesView.svelte

	Unified entities page with Overview + per-type tabs.
	Overview shows counts, search across all types, and a merged entity list.
	Per-type tabs show the full data grid with type-appropriate columns.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import type { Tab } from '$lib/tabs/types';
	import { Page } from '$lib';
	import { PersonTable, PlaceTable, OrganizationTable, ThingTable } from '$lib/components/wiki';
	import Icon from '$lib/components/Icon.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import {
		listPeople,
		listPlaces,
		listOrganizations,
		listThings,
		type WikiPersonListItem,
		type WikiPlaceListItem,
		type WikiOrganizationListItem,
		type WikiThingListItem,
	} from '$lib/wiki/api';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type EntityFilter = 'overview' | 'person' | 'place' | 'org' | 'thing';

	let activeFilter = $state<EntityFilter>('overview');
	let searchQuery = $state('');

	const filters: { key: EntityFilter; label: string }[] = [
		{ key: 'overview', label: 'Overview' },
		{ key: 'person', label: 'People' },
		{ key: 'place', label: 'Places' },
		{ key: 'org', label: 'Organizations' },
		{ key: 'thing', label: 'Things' },
	];

	// --- Overview data ---

	interface UnifiedEntity {
		id: string;
		name: string;
		entityType: 'person' | 'place' | 'org' | 'thing';
		icon: string;
		subtitle: string | null;
		route: string;
	}

	const typeConfig = {
		person: { icon: 'ri:user-line', label: 'People' },
		place: { icon: 'ri:map-pin-line', label: 'Places' },
		org: { icon: 'ri:building-line', label: 'Organizations' },
		thing: { icon: 'ri:lightbulb-line', label: 'Things' },
	} as const;

	let allEntities = $state<UnifiedEntity[]>([]);
	let loading = $state(true);

	let counts = $derived({
		person: allEntities.filter(e => e.entityType === 'person').length,
		place: allEntities.filter(e => e.entityType === 'place').length,
		org: allEntities.filter(e => e.entityType === 'org').length,
		thing: allEntities.filter(e => e.entityType === 'thing').length,
		total: allEntities.length,
	});

	let filteredEntities = $derived.by(() => {
		if (!searchQuery.trim()) return allEntities;
		const q = searchQuery.toLowerCase();
		return allEntities.filter(
			e => e.name.toLowerCase().includes(q) || e.subtitle?.toLowerCase().includes(q)
		);
	});

	async function loadAllEntities() {
		loading = true;
		try {
			const [people, places, orgs, things] = await Promise.all([
				listPeople(),
				listPlaces(),
				listOrganizations(),
				listThings(),
			]);

			const unified: UnifiedEntity[] = [
				...people.map((p: WikiPersonListItem): UnifiedEntity => ({
					id: p.id,
					name: p.canonical_name,
					entityType: 'person',
					icon: typeConfig.person.icon,
					subtitle: p.relationship_category,
					route: `/person/${p.id}`,
				})),
				...places.map((p: WikiPlaceListItem): UnifiedEntity => ({
					id: p.id,
					name: p.name,
					entityType: 'place',
					icon: typeConfig.place.icon,
					subtitle: p.category || p.address,
					route: `/place/${p.id}`,
				})),
				...orgs.map((o: WikiOrganizationListItem): UnifiedEntity => ({
					id: o.id,
					name: o.canonical_name,
					entityType: 'org',
					icon: typeConfig.org.icon,
					subtitle: o.organization_type || o.relationship_type,
					route: `/org/${o.id}`,
				})),
				...things.map((t: WikiThingListItem): UnifiedEntity => ({
					id: t.id,
					name: t.name,
					entityType: 'thing',
					icon: typeConfig.thing.icon,
					subtitle: t.category || t.description,
					route: `/thing/${t.id}`,
				})),
			];

			unified.sort((a, b) => a.name.localeCompare(b.name));
			allEntities = unified;
		} catch (e) {
			console.error('Failed to load entities:', e);
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadAllEntities();
	});

	function handleEntityClick(entity: UnifiedEntity) {
		windowShellStore.openTabFromRoute(entity.route);
	}

	function handleCountClick(type: EntityFilter) {
		activeFilter = type;
	}
</script>

<Page title="Entities" maxWidth="prose">
	{#snippet actions()}
		<nav class="filter-tabs">
			{#each filters as filter}
				<button
					class="filter-tab"
					class:active={activeFilter === filter.key}
					onclick={() => activeFilter = filter.key}
				>
					{filter.label}
				</button>
			{/each}
		</nav>
	{/snippet}

	<div class="entities-content">
		{#if activeFilter === 'overview'}
			<!-- Counts -->
			<div class="counts-row">
				{#each (['person', 'place', 'org', 'thing'] as const) as type}
					<button class="count-card" onclick={() => handleCountClick(type)}>
						<div class="count-icon">
							<Icon icon={typeConfig[type].icon} width="18" />
						</div>
						<span class="count-number">{counts[type]}</span>
						<span class="count-label">{typeConfig[type].label}</span>
					</button>
				{/each}
			</div>

			<!-- Search -->
			<div class="search-bar">
				<Icon icon="ri:search-line" width="16" />
				<input
					type="text"
					placeholder="Search all entities..."
					bind:value={searchQuery}
				/>
				{#if searchQuery}
					<button class="search-clear" onclick={() => searchQuery = ''}>
						<Icon icon="ri:close-line" width="14" />
					</button>
				{/if}
			</div>

			<!-- Entity list -->
			{#if loading}
				<div class="loading-state">
					<Icon icon="ri:loader-4-line" width="20" class="spin" />
				</div>
			{:else if filteredEntities.length === 0}
				<div class="empty-state">
					{#if searchQuery}
						<Icon icon="ri:search-line" width="32" />
						<p>No entities matching "{searchQuery}"</p>
					{:else}
						<Icon icon="ri:group-line" width="32" />
						<p>No entities yet</p>
					{/if}
				</div>
			{:else}
				<div class="entity-list">
					{#each filteredEntities as entity (entity.id)}
						<button class="entity-row" onclick={() => handleEntityClick(entity)}>
							<div class="entity-icon entity-icon--{entity.entityType}">
								<Icon icon={entity.icon} width="16" />
							</div>
							<div class="entity-info">
								<span class="entity-name">{entity.name}</span>
								{#if entity.subtitle}
									<span class="entity-subtitle">{entity.subtitle}</span>
								{/if}
							</div>
							<span class="entity-type-badge badge--{entity.entityType}">
								{typeConfig[entity.entityType].label.replace(/s$/, '')}
							</span>
						</button>
					{/each}
				</div>
			{/if}
		{:else if activeFilter === 'person'}
			<PersonTable />
		{:else if activeFilter === 'place'}
			<PlaceTable />
		{:else if activeFilter === 'org'}
			<OrganizationTable />
		{:else if activeFilter === 'thing'}
			<ThingTable />
		{/if}
	</div>
</Page>

<style>
	.entities-view {
		width: 100%;
		height: 100%;
		overflow-y: auto;
		padding: 1.5rem 0;
	}

	.page-header {
		padding: 0 2rem;
		margin-bottom: 1rem;
	}

	.page-header h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0 0 1rem;
		letter-spacing: -0.02em;
	}

	.filter-tabs {
		display: flex;
		gap: 0.25rem;
		border-bottom: 1px solid var(--color-border);
		padding-bottom: 0;
	}

	.filter-tab {
		padding: 0.5rem 0.75rem;
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		cursor: pointer;
		transition: color 0.15s, border-color 0.15s;
		margin-bottom: -1px;
	}

	.filter-tab:hover {
		color: var(--color-foreground);
	}

	.filter-tab.active {
		color: var(--color-foreground);
		border-bottom-color: var(--color-primary);
	}

	.entities-content {
		padding: 0;
	}

	/* ── Counts row ── */

	.counts-row {
		display: flex;
		gap: 0.75rem;
		padding: 1.25rem 2rem;
	}

	.count-card {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.25rem;
		padding: 1rem 0.75rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 10px;
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s;
	}

	.count-card:hover {
		border-color: var(--color-border-strong);
		background: var(--color-surface-hover, var(--color-surface));
	}

	.count-icon {
		color: var(--color-foreground-subtle);
	}

	.count-number {
		font-size: 1.5rem;
		font-weight: 600;
		color: var(--color-foreground);
		line-height: 1;
	}

	.count-label {
		font-size: 0.6875rem;
		font-weight: 500;
		color: var(--color-foreground-subtle);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	/* ── Search ── */

	.search-bar {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		margin: 0 2rem 0.75rem;
		padding: 0.5rem 0.75rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		color: var(--color-foreground-subtle);
	}

	.search-bar:focus-within {
		border-color: var(--color-primary);
	}

	.search-bar input {
		flex: 1;
		border: none;
		background: none;
		outline: none;
		font-size: 0.8125rem;
		color: var(--color-foreground);
	}

	.search-bar input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.search-clear {
		display: flex;
		align-items: center;
		justify-content: center;
		background: none;
		border: none;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		padding: 2px;
		border-radius: 4px;
	}

	.search-clear:hover {
		color: var(--color-foreground);
		background: var(--color-surface-hover, color-mix(in srgb, var(--color-foreground) 8%, transparent));
	}

	/* ── Entity list ── */

	.entity-list {
		padding: 0 1rem;
	}

	.entity-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		padding: 0.5rem 1rem;
		background: none;
		border: none;
		border-radius: 8px;
		cursor: pointer;
		text-align: left;
		transition: background 0.1s;
	}

	.entity-row:hover {
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}

	.entity-icon {
		width: 32px;
		height: 32px;
		border-radius: 8px;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.entity-icon--person {
		background: color-mix(in srgb, #3b82f6 12%, transparent);
		color: #3b82f6;
	}

	.entity-icon--place {
		background: color-mix(in srgb, #22c55e 12%, transparent);
		color: #22c55e;
	}

	.entity-icon--org {
		background: color-mix(in srgb, #8b5cf6 12%, transparent);
		color: #8b5cf6;
	}

	.entity-icon--thing {
		background: color-mix(in srgb, #f97316 12%, transparent);
		color: #f97316;
	}

	.entity-info {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.entity-name {
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.entity-subtitle {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		text-transform: capitalize;
	}

	.entity-type-badge {
		flex-shrink: 0;
		font-size: 0.6875rem;
		font-weight: 500;
		padding: 0.125rem 0.5rem;
		border-radius: 9999px;
	}

	.badge--person {
		background: color-mix(in srgb, #3b82f6 12%, transparent);
		color: #2563eb;
	}

	.badge--place {
		background: color-mix(in srgb, #22c55e 12%, transparent);
		color: #16a34a;
	}

	.badge--org {
		background: color-mix(in srgb, #8b5cf6 12%, transparent);
		color: #7c3aed;
	}

	.badge--thing {
		background: color-mix(in srgb, #f97316 12%, transparent);
		color: #ea580c;
	}

	/* ── States ── */

	.loading-state {
		display: flex;
		justify-content: center;
		padding: 3rem 0;
		color: var(--color-foreground-subtle);
	}

	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		padding: 3rem 0;
		color: var(--color-foreground-subtle);
	}

	.empty-state p {
		margin: 0;
		font-size: 0.875rem;
	}

	/* ── Responsive ── */

	@media (max-width: 640px) {
		.counts-row {
			flex-wrap: wrap;
			gap: 0.5rem;
		}

		.count-card {
			min-width: calc(50% - 0.5rem);
		}
	}
</style>
