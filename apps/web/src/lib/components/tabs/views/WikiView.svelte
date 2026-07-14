<!--
	WikiView.svelte

	The wiki room: Overview dashboard plus the entity sections, absorbed from
	the former /entities page. Sub-navigation is route-driven via SubNav
	(/wiki, /wiki/entities, /wiki/people, ...), so sections are deep-linkable.
-->

<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { Page } from '$lib';
	import {
		ActivityHeatmap,
		PersonTable,
		PlaceTable,
		OrganizationTable,
	} from '$lib/components/wiki';
	import SubNav, { type SubNavItem } from '$lib/components/SubNav.svelte';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { getLocalDateSlug, formatLongDate } from '$lib/utils/dateUtils';
	import {
		listPeople,
		listPlaces,
		listOrganizations,
		type WikiPersonListItem,
		type WikiPlaceListItem,
		type WikiOrganizationListItem,
	} from '$lib/wiki/api';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type Section = 'overview' | 'entities' | 'people' | 'places' | 'orgs';

	const sections: SubNavItem[] = [
		{ id: 'overview', label: 'Overview' },
		{ id: 'entities', label: 'Entities' },
		{ id: 'people', label: 'People' },
		{ id: 'places', label: 'Places' },
		{ id: 'orgs', label: 'Organizations' },
	];

	// Active section is derived from the route (SubNav owns the writing side).
	const section = $derived<Section>(
		(tab.route.match(/^\/wiki\/(entities|people|places|orgs)$/)?.[1] as Section) ??
			'overview'
	);

	// --- Unified entity list (also feeds the overview counts) ---

	interface UnifiedEntity {
		id: string;
		name: string;
		entityType: 'person' | 'place' | 'org';
		subtitle: string | null;
		route: string;
	}

	const typeConfig = {
		person: { icon: 'ri:user-line', label: 'Person', section: 'people' },
		place: { icon: 'ri:map-pin-line', label: 'Place', section: 'places' },
		org: { icon: 'ri:building-line', label: 'Organization', section: 'orgs' },
	} as const;

	let allEntities = $state<UnifiedEntity[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let counts = $derived({
		person: allEntities.filter((e) => e.entityType === 'person').length,
		place: allEntities.filter((e) => e.entityType === 'place').length,
		org: allEntities.filter((e) => e.entityType === 'org').length,
	});

	async function loadAllEntities() {
		loading = true;
		error = null;
		try {
			const [people, places, orgs] = await Promise.all([
				listPeople(),
				listPlaces(),
				listOrganizations(),
			]);

			const unified: UnifiedEntity[] = [
				...people.map((p: WikiPersonListItem): UnifiedEntity => ({
					id: p.id,
					name: p.canonical_name,
					entityType: 'person',
					subtitle: p.relationship_category,
					route: `/person/${p.id}`,
				})),
				...places.map((p: WikiPlaceListItem): UnifiedEntity => ({
					id: p.id,
					name: p.name,
					entityType: 'place',
					subtitle: p.category || p.address,
					route: `/place/${p.id}`,
				})),
				...orgs.map((o: WikiOrganizationListItem): UnifiedEntity => ({
					id: o.id,
					name: o.canonical_name,
					entityType: 'org',
					subtitle: o.organization_type || o.relationship_type,
					route: `/org/${o.id}`,
				})),
			];

			unified.sort((a, b) => a.name.localeCompare(b.name));
			allEntities = unified;
		} catch (e) {
			console.error('Failed to load entities:', e);
			error = e instanceof Error ? e.message : 'Failed to load entities';
		} finally {
			loading = false;
		}
	}

	const entityColumns: Column<UnifiedEntity>[] = [
		{
			key: 'name',
			label: 'Name',
			icon: 'ri:group-line',
			width: '45%',
			minWidth: '200px',
		},
		{
			key: 'entityType',
			label: 'Type',
			icon: 'ri:price-tag-3-line',
			width: '20%',
			minWidth: '120px',
			getValue: (item) => typeConfig[item.entityType].label,
		},
		{
			key: 'subtitle',
			label: 'Details',
			icon: 'ri:information-line',
			width: '35%',
			minWidth: '140px',
			hideOnMobile: true,
		},
	];

	// --- Overview data ---

	let activityData = $state<Map<string, number>>(new Map());
	let loadingActivity = $state(true);

	onMount(async () => {
		loadAllEntities();

		// Load activity data for the past year
		try {
			const endDate = new Date();
			const startDate = new Date();
			startDate.setFullYear(startDate.getFullYear() - 1);

			const res = await fetch(
				`/api/wiki/days?start_date=${getLocalDateSlug(startDate)}&end_date=${getLocalDateSlug(endDate)}`
			);

			if (res.ok) {
				const days = await res.json();
				const dataMap = new Map<string, number>();

				for (const day of days) {
					// Count activity based on whether there's content
					const hasContent = day.autobiography || day.autobiography_sections;
					if (hasContent) {
						dataMap.set(day.date, 1);
					}
				}

				activityData = dataMap;
			}
		} catch (e) {
			console.error('Failed to load activity data:', e);
		} finally {
			loadingActivity = false;
		}
	});

	// Handle day click from heatmap
	function handleDayClick(_date: Date, slug: string) {
		// slug is a date string like "2026-01-24"
		windowShellStore.openTabFromRoute(`/day/day_${slug}`);
	}

	function openEntity(entity: UnifiedEntity) {
		windowShellStore.openTabFromRoute(entity.route);
	}

	// Overview cards switch this pane to the matching section.
	function goToSection(id: Section) {
		windowShellStore.updateTab(tab.id, { route: id === 'overview' ? '/wiki' : `/wiki/${id}` });
	}

	// Today's formatted date
	const today = new Date();
	const todaySlug = getLocalDateSlug(today);
	const todayFormatted = formatLongDate(today);

	// Entity display config for the overview cards
	const entityCards = [
		{ key: 'person', label: 'People' },
		{ key: 'place', label: 'Places' },
		{ key: 'org', label: 'Organizations' },
	] as const;
</script>

<div class="wiki-view">
	<SubNav
		tabId={tab.id}
		route={tab.route}
		base="/wiki"
		default="overview"
		items={sections}
		ariaLabel="Wiki sections"
	/>

	<main class="content">
		{#if section === 'overview'}
			<Page title="Wiki" description="Your personal knowledge base" maxWidth="prose">
				<!-- Today context -->
				<div class="today-context">
					<p>
						Today's entry is
						<button
							onclick={() => windowShellStore.openTabFromRoute(`/day/day_${todaySlug}`)}
							class="today-link"
						>
							{todayFormatted}
						</button>
					</p>
				</div>

				<!-- Activity Heatmap -->
				<section class="section heatmap-section">
					<h2>Activity</h2>
					{#if loadingActivity}
						<div class="heatmap-loading">
							<span class="loading-text">Loading activity...</span>
						</div>
					{:else}
						<ActivityHeatmap {activityData} onDayClick={handleDayClick} />
					{/if}
				</section>

				<hr class="divider" />

				<!-- Entities -->
				<section class="section">
					<h2>Entities</h2>
					<p class="section-description">
						The people, places, and organizations that appear in your data.
					</p>

					<div class="entity-grid">
						{#each entityCards as card}
							<button
								onclick={() => goToSection(typeConfig[card.key].section)}
								class="entity-card"
							>
								<Icon icon={typeConfig[card.key].icon} class="entity-icon" />
								<span class="entity-label">{card.label}</span>
								<span class="entity-count">{counts[card.key]}</span>
							</button>
						{/each}
					</div>
				</section>
			</Page>
		{:else if section === 'entities'}
			<div class="grid-wrap">
				<UniversalDataGrid
					items={allEntities}
					columns={entityColumns}
					entityType="entities"
					{loading}
					{error}
					emptyIcon="ri:group-line"
					emptyMessage="No entities yet"
					loadingMessage="Loading entities..."
					searchPlaceholder="Search all entities..."
					onItemClick={openEntity}
					onRetry={loadAllEntities}
				>
					{#snippet tableRow(entity: UnifiedEntity)}
						<td class="col-name">
							<div class="name-cell">
								<Icon icon={typeConfig[entity.entityType].icon} width="16" />
								<span class="name-text">{entity.name}</span>
							</div>
						</td>
						<td class="col-type">
							<span class="badge badge-muted">{typeConfig[entity.entityType].label}</span>
						</td>
						<td class="col-details hide-mobile">
							{#if entity.subtitle}
								<span class="subtitle-text">{entity.subtitle}</span>
							{:else}
								<span class="empty-cell">—</span>
							{/if}
						</td>
					{/snippet}

					{#snippet card(entity: UnifiedEntity)}
						<div class="card-content">
							<Icon icon={typeConfig[entity.entityType].icon} width="28" />
							<span class="card-name">{entity.name}</span>
							<span class="badge badge-muted">{typeConfig[entity.entityType].label}</span>
						</div>
					{/snippet}
				</UniversalDataGrid>
			</div>
		{:else if section === 'people'}
			<div class="grid-wrap"><PersonTable /></div>
		{:else if section === 'places'}
			<div class="grid-wrap"><PlaceTable /></div>
		{:else if section === 'orgs'}
			<div class="grid-wrap"><OrganizationTable /></div>
		{/if}
	</main>
</div>

<style>
	.wiki-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.content {
		flex: 1;
		overflow-y: auto;
		min-height: 0;
	}

	.grid-wrap {
		padding: 1.25rem 1.5rem 2rem;
		max-width: 72rem;
		width: 100%;
		margin: 0 auto;
	}

	/* Today context */
	.today-context {
		margin-bottom: 2rem;
	}

	.today-context p {
		font-size: 1rem;
		color: var(--color-foreground-muted);
		margin: 0;
	}

	.today-link {
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		font-weight: 500;
		cursor: pointer;
		text-decoration: none;
		transition: opacity 0.15s ease;
	}

	.today-link:hover {
		opacity: 0.8;
		text-decoration: underline;
	}

	/* Sections */
	.section {
		margin-bottom: 2rem;
	}

	.section h2 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.25rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0 0 0.75rem 0;
	}

	.section-description {
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
		margin: 0 0 1rem 0;
		line-height: 1.5;
	}

	.heatmap-section {
		margin-bottom: 1.5rem;
	}

	.heatmap-loading {
		padding: 2rem;
		text-align: center;
	}

	.loading-text {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}

	/* Divider */
	.divider {
		border: none;
		border-top: 1px solid var(--color-border);
		margin: 1.5rem 0;
	}

	/* Entity grid */
	.entity-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 0.75rem;
	}

	.entity-card {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 1rem;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		cursor: pointer;
		transition: all 0.15s ease;
		text-align: left;
		font: inherit;
	}

	.entity-card:hover {
		border-color: var(--color-border-subtle);
		background: var(--color-surface-hover);
	}

	.entity-card :global(.entity-icon) {
		font-size: 1.25rem;
		color: var(--color-foreground-muted);
	}

	.entity-label {
		flex: 1;
		font-size: 0.9375rem;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.entity-count {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	/* Unified entity grid cells */
	.name-cell {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--color-foreground-subtle);
	}

	.name-text {
		font-weight: 500;
		color: var(--color-foreground);
	}

	.subtitle-text {
		color: var(--color-foreground-muted);
		font-size: 0.8125rem;
		text-transform: capitalize;
	}

	.empty-cell {
		color: var(--color-foreground-subtle);
	}

	.col-name {
		width: 45%;
		min-width: 200px;
		padding: 0.625rem 0.75rem;
		padding-left: 0;
	}

	.col-type {
		width: 20%;
		min-width: 120px;
		padding: 0.625rem 0.75rem;
	}

	.col-details {
		width: 35%;
		min-width: 140px;
		padding: 0.625rem 0.75rem;
		padding-right: 0;
	}

	/* Card mode */
	.card-content {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.75rem;
		text-align: center;
		color: var(--color-foreground-subtle);
	}

	.card-name {
		font-weight: 600;
		font-size: 0.9375rem;
		color: var(--color-foreground);
		line-height: 1.3;
	}

	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}
	}

	/* Responsive */
	@media (max-width: 640px) {
		.entity-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
