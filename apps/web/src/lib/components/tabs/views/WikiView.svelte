<!--
	WikiView.svelte

	The wiki room: the wikipedia of one life. Four sections, route-driven via
	SubNav (deep-linkable):

	  /wiki           Overview — the front page: standfirst, activity, on
	                  this day, the latest entry, and the index.
	  /wiki/days      Days — the temporal spine, a year calendar + chronicle.
	  /wiki/entities  Entities — one index; person/place/org are filters.
	  /wiki/identity  Narrative identity — user-authored, essay register.

	Legacy routes (/wiki/people, /wiki/places, /wiki/orgs, /wiki/unlinked,
	/entities) fold into the entities section, presetting its type filter.

	The overview is set as an essay with a marginalia rail: the main column
	carries the text and the charts; the margin carries the numbers and
	asides. Charts and inline components stay crisp; the prose stays serif.
-->

<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import {
		ActivityHeatmap,
		DaysChronicle,
		NarrativeIdentitySection,
	} from '$lib/components/wiki';
	import SubNav, { type SubNavItem } from '$lib/components/SubNav.svelte';
	import UniversalDataGrid, {
		type Column,
	} from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import type { FilterDef } from '$lib/components/datagrid/types';
	import { onMount } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { getLocalDateSlug, formatLongDate } from '$lib/utils/dateUtils';
	import {
		listPeople,
		listPlaces,
		listOrganizations,
		listDays,
		listDayActivity,
		listOnThisDay,
		getNarrativeIdentity,
		type WikiPersonListItem,
		type WikiPlaceListItem,
		type WikiOrganizationListItem,
		type OnThisDayApi,
	} from '$lib/wiki/api';
	import { toActivityLevels } from '$lib/wiki/activity';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type Section = 'overview' | 'days' | 'entities' | 'identity';

	const sections = $derived<SubNavItem[]>([
		{ id: 'overview', label: 'Overview' },
		{ id: 'days', label: 'Days' },
		{ id: 'entities', label: 'Entities' },
		{ id: 'identity', label: 'Narrative Identity' },
	]);

	// Active section is derived from the route (SubNav owns the writing side).
	// Legacy per-type routes land in the unified entities section.
	const LEGACY_TYPE: Record<string, 'person' | 'place' | 'org'> = {
		people: 'person',
		places: 'place',
		orgs: 'org',
	};

	const routeSegment = $derived(
		tab.route.match(
			/^\/wiki\/(days|entities|identity|people|places|orgs|unlinked)$/
		)?.[1] ?? (tab.route === '/entities' ? 'entities' : null)
	);

	const section = $derived<Section>(
		routeSegment == null
			? 'overview'
			: routeSegment === 'days' || routeSegment === 'identity'
				? routeSegment
				: 'entities'
	);

	/** Type filter preset when arriving via a legacy per-type route. */
	const entityTypePreset = $derived(
		routeSegment ? (LEGACY_TYPE[routeSegment] ?? null) : null
	);

	/** What SubNav highlights: legacy per-type routes read as Entities. */
	const subNavRoute = $derived(
		section === 'entities' ? '/wiki/entities' : tab.route
	);

	// --- Unified entity list (also feeds the overview index) ---

	interface UnifiedEntity {
		id: string;
		name: string;
		entityType: 'person' | 'place' | 'org';
		subtitle: string | null;
		route: string;
	}

	const typeConfig = {
		person: { icon: 'ri:user-line', label: 'Person', plural: 'People', legacy: 'people' },
		place: { icon: 'ri:map-pin-line', label: 'Place', plural: 'Places', legacy: 'places' },
		org: { icon: 'ri:building-line', label: 'Organization', plural: 'Organizations', legacy: 'orgs' },
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

	// One filter instead of three tabs: person/place/org narrow the same index.
	const entityFilters = $derived<FilterDef<UnifiedEntity>[]>([
		{
			id: 'type',
			label: 'Type',
			kind: 'multi',
			field: 'entityType',
			defaultValue: entityTypePreset ? [entityTypePreset] : null,
			options: [
				{ value: 'person', label: 'People', icon: typeConfig.person.icon },
				{ value: 'place', label: 'Places', icon: typeConfig.place.icon },
				{ value: 'org', label: 'Organizations', icon: typeConfig.org.icon },
			],
		},
	]);

	// --- Overview data ---

	let activityData = $state<Map<string, number>>(new Map());
	let loadingActivity = $state(true);
	let activityStats = $state<{ recorded: number; narrated: number; stubs: number }>({
		recorded: 0,
		narrated: 0,
		stubs: 0,
	});
	let onThisDay = $state<OnThisDayApi[]>([]);
	let latestEntry = $state<{ slug: string; label: string; epigraph: string | null } | null>(null);
	let standfirst = $state<string | null>(null);

	// One source of truth for the window: fetch exactly what the heatmap draws.
	const HEATMAP_WEEKS = 26;

	onMount(async () => {
		loadAllEntities();

		try {
			const endDate = new Date();
			const startDate = new Date();
			startDate.setDate(startDate.getDate() - HEATMAP_WEEKS * 7);
			const recentStart = new Date();
			recentStart.setDate(recentStart.getDate() - 45);

			const [activity, otd, recent, identity] = await Promise.all([
				listDayActivity(getLocalDateSlug(startDate), getLocalDateSlug(endDate)),
				listOnThisDay(),
				listDays(getLocalDateSlug(recentStart), getLocalDateSlug(endDate)),
				getNarrativeIdentity(),
			]);

			activityData = toActivityLevels(activity);
			activityStats = {
				recorded: activity.filter((d) => d.event_count > 0).length,
				narrated: activity.filter((d) => d.narrated).length,
				stubs: activity.filter((d) => d.event_count > 0 && !d.narrated).length,
			};

			onThisDay = otd;

			// recent is date DESC; the latest narrated day is the featured entry.
			const featured = recent.find((d) => d.autobiography);
			if (featured) {
				latestEntry = {
					slug: featured.date,
					label: new Date(featured.date + 'T12:00:00').toLocaleDateString('en-US', {
						weekday: 'long',
						month: 'long',
						day: 'numeric',
						year: 'numeric',
					}),
					epigraph: featured.epigraph,
				};
			}

			// The identity's first line is the front page's standfirst.
			const firstLine = identity?.content
				?.split('\n')
				.map((l) => l.replace(/^[#>*\-\s]+/, '').trim())
				.find((l) => l.length > 0);
			if (firstLine) {
				standfirst =
					firstLine.length > 180 ? firstLine.slice(0, 177) + '…' : firstLine;
			}
		} catch (e) {
			console.error('Failed to load overview data:', e);
		} finally {
			loadingActivity = false;
		}
	});

	// Handle day click from heatmap / chronicle / links
	function openDay(slug: string) {
		windowShellStore.openTabFromRoute(`/day/day_${slug}`);
	}

	function openEntity(entity: UnifiedEntity) {
		windowShellStore.openTabFromRoute(entity.route);
	}

	// Overview cards switch this pane to the matching section.
	function goTo(path: string) {
		windowShellStore.updateTab(tab.id, { route: path });
	}

	// Today's formatted date
	const today = new Date();
	const todaySlug = getLocalDateSlug(today);
	const todayFormatted = formatLongDate(today);

	function yearOf(slug: string): string {
		return slug.slice(0, 4);
	}
</script>

<div class="wiki-view">
	<SubNav
		tabId={tab.id}
		route={subNavRoute}
		base="/wiki"
		default="overview"
		items={sections}
		ariaLabel="Wiki sections"
	/>

	<main class="content">
		{#if section === 'overview'}
			<div class="ovw">
				<header class="mast">
					<h1>Wiki</h1>
					<p class="standfirst">
						{standfirst ??
							'A record of your life — its days, its people and places, and the story they add up to.'}
					</p>
					<p class="today-line">
						Today's entry is
						<button onclick={() => openDay(todaySlug)} class="today-link">
							{todayFormatted}
						</button>
					</p>
				</header>

				<section class="sec">
					<div class="sec-main">
						<h2>Activity</h2>
						{#if loadingActivity}
							<p class="quiet">Loading activity…</p>
						{:else}
							<ActivityHeatmap
								{activityData}
								onDayClick={(_d, slug) => openDay(slug)}
							/>
						{/if}
					</div>
					<aside class="sec-aside">
						{#if !loadingActivity}
							<dl class="stat-stack">
								<div>
									<dt>Days recorded</dt>
									<dd>{activityStats.recorded}</dd>
								</div>
								<div>
									<dt>Narrated</dt>
									<dd>{activityStats.narrated}</dd>
								</div>
								<div>
									<dt>Awaiting narration</dt>
									<dd>{activityStats.stubs}</dd>
								</div>
							</dl>
							<p class="aside-note">The last six months, day by day.</p>
						{/if}
					</aside>
				</section>

				<section class="sec">
					<div class="sec-main">
						<h2>On this day</h2>
						{#if onThisDay.length === 0}
							<p class="quiet">
								No earlier years share this date yet — the record is young.
							</p>
						{:else}
							<ul class="otd">
								{#each onThisDay as entry (entry.date)}
									<li>
										<button class="otd-row" onclick={() => openDay(entry.date)}>
											<span class="otd-year">{yearOf(entry.date)}</span>
											{#if entry.epigraph}
												<span class="otd-epigraph">{entry.epigraph}</span>
											{:else if entry.narrated}
												<span class="otd-epigraph">A narrated day</span>
											{:else}
												<span class="otd-stub">
													{entry.event_count}
													{entry.event_count === 1 ? 'event' : 'events'}, unwritten
												</span>
											{/if}
										</button>
									</li>
								{/each}
							</ul>
						{/if}
					</div>
					<aside class="sec-aside">
						<p class="aside-note">{todayFormatted.replace(/,\s*\d{4}$/, '')} in earlier years.</p>
					</aside>
				</section>

				{#if latestEntry}
					<section class="sec">
						<div class="sec-main">
							<h2>The latest entry</h2>
							<button class="featured" onclick={() => openDay(latestEntry!.slug)}>
								<span class="featured-date">{latestEntry.label}</span>
								{#if latestEntry.epigraph}
									<blockquote class="featured-epigraph">
										{latestEntry.epigraph}
									</blockquote>
								{/if}
								<span class="featured-open">Read the entry →</span>
							</button>
						</div>
						<aside class="sec-aside">
							<p class="aside-note">The most recent day the nightly narration has written.</p>
						</aside>
					</section>
				{/if}

				<section class="sec">
					<div class="sec-main">
						<h2>Index</h2>
						<div class="index-row">
							{#each Object.entries(typeConfig) as [key, cfg] (key)}
								<button
									class="index-card"
									onclick={() => goTo(`/wiki/${cfg.legacy}`)}
								>
									<Icon icon={cfg.icon} class="index-icon" />
									<span class="index-label">{cfg.plural}</span>
									<span class="index-count">{counts[key as keyof typeof counts]}</span>
								</button>
							{/each}
						</div>
					</div>
					<aside class="sec-aside">
						<p class="aside-note">Everything the record names, in one list.</p>
					</aside>
				</section>
			</div>
		{:else if section === 'days'}
			<div class="days-wrap">
				<DaysChronicle onOpenDay={openDay} />
			</div>
		{:else if section === 'entities'}
			<div class="grid-wrap">
				{#key entityTypePreset}
					<UniversalDataGrid
						items={allEntities}
						columns={entityColumns}
						entityType="entities"
						{loading}
						{error}
						filters={entityFilters}
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
				{/key}
			</div>
		{:else if section === 'identity'}
			<div class="identity-wrap">
				<NarrativeIdentitySection />
			</div>
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

	.days-wrap {
		padding: 2rem 1.5rem 3rem;
		max-width: 50rem;
		width: 100%;
		margin: 0 auto;
	}

	.identity-wrap {
		padding: 2.5rem 1.5rem 3rem;
		max-width: 44rem;
		width: 100%;
		margin: 0 auto;
	}

	/* ===== Overview: essay column + marginalia rail ===== */

	.ovw {
		max-width: 54rem;
		width: 100%;
		margin: 0 auto;
		padding: 2.5rem 1.5rem 4rem;
	}

	.mast {
		margin-bottom: 2.5rem;
	}

	.mast h1 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 2rem;
		font-weight: 500;
		letter-spacing: -0.01em;
		color: var(--color-foreground);
		margin: 0 0 0.625rem;
	}

	.standfirst {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.0625rem;
		font-style: italic;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0 0 1rem;
		max-width: 40rem;
	}

	.today-line {
		font-size: 0.875rem;
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
	}

	.today-link:hover {
		text-decoration: underline;
	}

	/* Each section is one grid row: the essay column and its margin. */
	.sec {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 11rem;
		gap: 2.25rem;
		padding: 1.75rem 0;
		border-top: 1px solid var(--color-border);
	}

	.sec-main h2 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.25rem;
		font-weight: 400;
		color: var(--color-foreground);
		margin: 0 0 1rem;
	}

	/* Marginalia: quiet, small, aligned to the section's first baseline. */
	.sec-aside {
		padding-top: 0.375rem;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.aside-note {
		font-size: 0.6875rem;
		line-height: 1.5;
		letter-spacing: 0.02em;
		color: var(--color-foreground-subtle);
		margin: 0;
	}

	.stat-stack {
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}

	.stat-stack div {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
	}

	.stat-stack dt {
		font-size: 0.6875rem;
		letter-spacing: 0.02em;
		color: var(--color-foreground-subtle);
		flex: 1;
	}

	.stat-stack dd {
		margin: 0;
		font-size: 0.8125rem;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground);
	}

	.quiet {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		margin: 0;
	}

	/* On this day */
	.otd {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.otd-row {
		display: flex;
		align-items: baseline;
		gap: 1rem;
		width: 100%;
		padding: 0.4375rem 0;
		background: none;
		border: none;
		border-bottom: 1px solid color-mix(in srgb, var(--color-border) 45%, transparent);
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	.otd-row:last-child {
		border-bottom: none;
	}

	.otd-year {
		flex: none;
		font-size: 0.75rem;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
	}

	.otd-row:hover .otd-year {
		color: var(--color-primary);
	}

	.otd-epigraph {
		font-family: var(--font-serif, Georgia, serif);
		font-style: italic;
		font-size: 0.9375rem;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.otd-stub {
		font-size: 0.8125rem;
		font-style: italic;
		color: var(--color-foreground-subtle);
	}

	/* Featured entry */
	.featured {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.625rem;
		width: 100%;
		background: none;
		border: none;
		padding: 0;
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	.featured-date {
		font-size: 0.75rem;
		letter-spacing: 0.04em;
		color: var(--color-foreground-muted);
	}

	.featured-epigraph {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.1875rem;
		font-style: italic;
		line-height: 1.5;
		color: var(--color-foreground);
		margin: 0;
		padding-left: 1rem;
		border-left: 2px solid var(--color-border);
	}

	.featured-open {
		font-size: 0.8125rem;
		color: var(--color-primary);
	}

	.featured:hover .featured-open {
		text-decoration: underline;
	}

	/* Index */
	.index-row {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 0.75rem;
	}

	.index-card {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		padding: 0.875rem 1rem;
		background: var(--color-surface-elevated);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		cursor: pointer;
		transition: all 0.15s ease;
		text-align: left;
		font: inherit;
	}

	.index-card:hover {
		border-color: var(--color-border-subtle);
		background: var(--color-surface-hover);
	}

	.index-card :global(.index-icon) {
		font-size: 1.125rem;
		color: var(--color-foreground-muted);
	}

	.index-label {
		flex: 1;
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-foreground);
	}

	.index-count {
		font-size: 0.8125rem;
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

	/* The margin collapses before the essay column does. */
	@media (max-width: 880px) {
		.sec {
			grid-template-columns: 1fr;
			gap: 0.875rem;
		}

		.sec-aside {
			padding-top: 0;
		}

		.stat-stack {
			flex-direction: row;
			gap: 1.25rem;
		}
	}

	@media (max-width: 640px) {
		.index-row {
			grid-template-columns: 1fr;
		}
	}
</style>
