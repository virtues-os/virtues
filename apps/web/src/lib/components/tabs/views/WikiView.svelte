<!--
	WikiView.svelte
	The wiki room. Its sections are route-driven and deep-linkable, and this
	view renders whatever the route names.

	  /wiki           The front page (WikiFront): find, this week, your life,
	                  on this day and lately. Its links reach the sections.
	  /wiki/days      Days: a year's heatmap and the chronicle.
	  /wiki/entities  One index; person, place and organization are filters.
	  /wiki/identity  Narrative identity, written by you.
	  /wiki/years, /wiki/chapters, /wiki/things, /wiki/history

	Legacy routes (/wiki/people, /wiki/places, /wiki/orgs, /wiki/unlinked,
	/entities) fold into the entities section, presetting its type filter.
-->

<script lang="ts">
	import { usePageWidth } from '$lib/stores/pageWidth.svelte';
	import type { Tab } from '$lib/tabs/types';
	import { TextAction } from '$lib';
	import { WIKI_SECTION_RE } from '$lib/tabs/registry';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { openMenuItems } from '$lib/utils/openMenuItems';
	import {
		ActivityHeatmap,
		ChaptersSection,
		ThingsSection,
		DaysChronicle,
		NarrativeIdentitySection,
	} from '$lib/components/wiki';
	import UniversalDataGrid, {
		type Column,
	} from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { onMount, untrack } from 'svelte';
	import { Button } from '$lib';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import { ledeSentence } from '$lib/wiki/lede';
	import { veilMarks } from '$lib/wiki/dayArticle';
	import { veiled } from '$lib/actions/veil';
	import { veil } from '$lib/stores/veil.svelte';
	import {
		listPeople,
		listPlaces,
		listOrganizations,
		listYears,
		getYear,
		type YearApi,
		type YearSummaryApi,
		type WikiPersonListItem,
		type WikiPlaceListItem,
		type WikiOrganizationListItem,
	} from '$lib/wiki/api';
	import { activityThresholds, toActivityDays } from '$lib/wiki/activity';
	import { getProfile } from '$lib/api/client';
	import WikiHistory from '$lib/components/wiki/WikiHistory.svelte';
	import NotesRail from '$lib/components/wiki/NotesRail.svelte';
	import { contextMenu } from '$lib/stores/contextMenu.svelte';
	import { getKeepMenuItems } from '$lib/utils/contextMenuItems';
	import { reclassifyPersonAsOrg, createPerson, deleteEntity } from '$lib/wiki/api';
	import WikiFront from '$lib/components/wiki/WikiFront.svelte';
	import ListRow from '$lib/components/datagrid/ListRow.svelte';
	import PersonPicture from '$lib/components/wiki/PersonPicture.svelte';
	import Icon from '$lib/components/Icon.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// The section list lives in one place: WIKI_MODE in `lib/sidebar/modes.ts`,
	// whose Time and Subjects rows the front page links to. People, Places and
	// Organizations stay their own rows there even though the content folds
	// them into one Entities index: the legacy segment presets the index's type
	// filter (see LEGACY_TYPE below). History has no row; the front page links
	// to it at the bottom.

	type Section = 'overview' | 'things' | 'days' | 'years' | 'entities' | 'identity' | 'chapters' | 'history';


	// The active section comes from the route.
	// Legacy per-type routes land in the unified entities section.
	const LEGACY_TYPE: Record<string, 'person' | 'place' | 'org'> = {
		people: 'person',
		places: 'place',
		orgs: 'org',
	};

	const routeSegment = $derived(
		tab.route.match(WIKI_SECTION_RE)?.[1] ?? (tab.route === '/entities' ? 'entities' : null)
	);

	/**
	 * The only segments that are NOT their own section.
	 *
	 * This was written the other way round — an allowlist of sections that
	 * render themselves, with everything else folding into Entities — which
	 * meant every new section silently became Entities until someone remembered
	 * to add it. Lifeline and History both did exactly that: the route matched,
	 * the tab opened, and the entity index appeared.
	 *
	 * Inverted, the default is "a section is itself" and only these four
	 * historical aliases are special. Adding a section now requires no edit
	 * here at all.
	 */
	const FOLDS_INTO_ENTITIES = ['people', 'places', 'orgs', 'unlinked'] as const;

	const section = $derived<Section>(
		routeSegment == null
			? 'overview'
			: (FOLDS_INTO_ENTITIES as readonly string[]).includes(routeSegment)
				? 'entities'
				: // A tab saved while the room was called Stories opens Things.
					routeSegment === 'stories'
					? 'things'
					: (routeSegment as Section)
	);

	// Each section of the room keeps its own width: the days' grid is full,
	// the lists and articles are set to the reading measure. The overview
	// (WikiFront) sets its own.
	const width = usePageWidth(() => (section === 'overview' ? null : section === 'days' ? 'full' : 'reading'), {
		part: () => section,
	});

	// The lifeline left the wiki for the Timeline, the rail's own room for a
	// life laid out in time. Its old door lands there.
	$effect(() => {
		if (routeSegment === 'lifeline') windowShellStore.navigate('/timeline', { label: 'Timeline' });
	});

	// --- Years ---
	//
	// Derived, not stored: there is no years endpoint, so the index is grouped
	// from day activity. Only years with recorded days appear — an empty year
	// is not a year of your life you'd want listed.

	// Years come from the box now. They used to be derived here from a ten-year
	// window of day activity, with a comment explaining that no years endpoint
	// existed — so a year before the record simply was not a year, and every
	// row linked to the days index regardless of which year you clicked.
	let years = $state<YearSummaryApi[]>([]);
	let yearsLoaded = $state(false);

	async function loadYears() {
		if (yearsLoaded) return;
		years = await listYears();
		yearsLoaded = true;
	}

	$effect(() => {
		if (section === 'years') void loadYears();
	});

	/** Type filter preset when arriving via a legacy per-type route. */
	const entityTypePreset = $derived(
		routeSegment ? (LEGACY_TYPE[routeSegment] ?? null) : null
	);

	/** What the rail highlights: legacy per-type routes read as Entities. */

	// --- Unified entity list (also feeds the overview index) ---

	interface UnifiedEntity {
		id: string;
		name: string;
		entityType: 'person' | 'place' | 'org';
		/** A person's picture, usually their contact photo. Places and orgs have none. */
		picture: string | null;
		subtitle: string | null;
		route: string;
		refCount: number;
		isSelf: boolean;
	}

	const typeConfig = {
		person: { icon: 'mynaui:user', label: 'Person', plural: 'People', legacy: 'people' },
		place: { icon: 'mynaui:map-pin', label: 'Place', plural: 'Places', legacy: 'places' },
		org: { icon: 'mynaui:building', label: 'Organization', plural: 'Organizations', legacy: 'orgs' },
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
			// The owner is in the graph like anyone else — a real box carries a
			// person row for them, built from contacts and email. The profile
			// says which one (0080); without it the index lists you as a
			// stranger among your own contacts.
			const [people, places, orgs, profile] = await Promise.all([
				listPeople(),
				listPlaces(),
				listOrganizations(),
				getProfile().catch(() => null),
			]);
			const selfId = profile?.self_person_id ?? null;

			const unified: UnifiedEntity[] = [
				...people.map((p: WikiPersonListItem): UnifiedEntity => ({
					id: p.id,
					name: p.name,
					entityType: 'person',
					picture: p.picture,
					subtitle: p.relationship_category,
					route: `/person/${p.id}`,
					refCount: p.ref_count ?? 0,
					isSelf: p.id === selfId,
				})),
				...places.map((p: WikiPlaceListItem): UnifiedEntity => ({
					id: p.id,
					name: p.name,
					entityType: 'place',
					picture: null,
					subtitle: p.category || p.address,
					route: `/place/${p.id}`,
					refCount: p.ref_count ?? 0,
					isSelf: false,
				})),
				...orgs.map((o: WikiOrganizationListItem): UnifiedEntity => ({
					id: o.id,
					name: o.name,
					entityType: 'org',
					picture: null,
					subtitle: o.organization_type || o.relationship_type,
					route: `/org/${o.id}`,
					refCount: o.ref_count ?? 0,
					isSelf: false,
				})),
			];

			// By mentions, not by name. Each of the three endpoints already returns
			// its own list in this order; the merge has to re-apply it or the
			// interleave silently restores the alphabet — which is what this
			// index did before, and why hundreds of contacts arrived with no order
			// at all. Name is the tie-break, so the long tail of 0-mention rows is
			// still browsable.
			unified.sort((a, b) => b.refCount - a.refCount || a.name.localeCompare(b.name));
			allEntities = unified;
		} catch (e) {
			console.error('Failed to load entities:', e);
			error = e instanceof Error ? e.message : 'Failed to load entities';
		} finally {
			loading = false;
		}
	}

	// The list is read, not scanned: a name and a line under it, grouped by
	// kind. Within a group the order is by mentions (see loadAllEntities).
	const entityColumns: Column<UnifiedEntity>[] = [
		{ key: 'name', label: 'Name' },
		{ key: 'subtitle', label: 'Details', hidden: true },
		{
			key: 'entityType',
			label: 'Kind',
			hidden: true,
			groupable: true,
			groupOrder: ['People', 'Places', 'Organizations'],
			getValue: (item) => typeConfig[item.entityType].plural,
		},
	];

	/** The row's second line, with its first letter raised: "friend" reads "Friend". */
	function detailLine(entity: UnifiedEntity): string {
		const text = entity.subtitle?.trim() ?? '';
		return text ? text[0].toUpperCase() + text.slice(1) : '';
	}

	/**
	 * Row menu: the grid's default items, plus the one correction this index
	 * exists to make possible.
	 *
	 * A person row is not always a person. `extract_name_from_email()` mints one
	 * for any unseen sender, so the index arrives holding Acme Payroll, Slack and
	 * The Plaid Team alongside real contacts. Ranking by mentions sinks them, but
	 * sinking is not fixing — this is the fix, and it belongs on the row rather
	 * than behind a detail page nobody opens for a company.
	 *
	 * Only offered for people: place and org rows have nowhere to go.
	 */
	function entityContextMenu(entity: UnifiedEntity, e: MouseEvent) {
		e.preventDefault();
		const items = [
			...openMenuItems(entity.route, entity.name),
			...getKeepMenuItems({
				url: entity.route,
				label: entity.name,
				icon: typeConfig[entity.entityType].icon,
			}),
		];

		// The owner is a person by definition — the server refuses this anyway
		// (migration 0080), but offering it and then failing is worse than not
		// offering it.
		if (entity.entityType === 'person' && !entity.isSelf) {
			items.push({
				id: 'reclassify-org',
				label: 'This is an organization',
				icon: 'mynaui:building',
				dividerBefore: true,
				action: () => void reclassifyEntity(entity),
			});
		}
		// Deleting takes the refs, article, notes and any pins with it — see
		// purge_subject. Not offered for the owner.
		if (!entity.isSelf) {
			items.push({
				id: 'delete-entity',
				label: 'Delete',
				icon: 'mynaui:trash',
				dividerBefore: entity.entityType !== 'person',
				action: () => void removeEntity(entity),
			});
		}
		contextMenu.show({ x: e.clientX, y: e.clientY }, items);
	}

	async function removeEntity(entity: UnifiedEntity) {
		// Irreversible and it takes the edges with it, so it asks first.
		if (!confirm(`Delete ${entity.name}? This also removes its records links, notes and pins.`)) return;
		try {
			await deleteEntity(entity.entityType, entity.id);
			await loadAllEntities();
		} catch (err) {
			error = err instanceof Error ? err.message : 'Could not delete';
		}
	}

	async function addPerson() {
		const name = prompt('Name of the person to add:')?.trim();
		if (!name) return;
		try {
			const made = await createPerson(name);
			await loadAllEntities();
			windowShellStore.openTabFromRoute(made.route, { label: name, focusExisting: true });
		} catch (err) {
			error = err instanceof Error ? err.message : 'Could not create that person';
		}
	}

	async function reclassifyEntity(entity: UnifiedEntity) {
		try {
			await reclassifyPersonAsOrg(entity.id);
			// Reload rather than patch in place: the row changes id, type and
			// route all at once, so a local edit would leave a row pointing at
			// a person route that no longer resolves.
			await loadAllEntities();
		} catch (err) {
			console.error('[WikiView] reclassify failed:', err);
			error = err instanceof Error ? err.message : 'Could not reclassify';
		}
	}

	// Grouping by kind does what a type filter would. A legacy per-kind
	// route (/wiki/people) lists that kind alone.
	const listedEntities = $derived(
		entityTypePreset ? allEntities.filter((e) => e.entityType === entityTypePreset) : allEntities
	);

	// What the front page's finder looks through.
	const findable = $derived(
		allEntities.map((e) => ({
			name: e.name,
			route: e.route,
			kind: e.isSelf ? 'You' : typeConfig[e.entityType].label,
		}))
	);

	onMount(() => {
		loadAllEntities();
	});

	// --- Days: one year as a heatmap ---
	//
	// A calendar year, Jan to Dec, from the year endpoint, which carries each
	// day's count and opening line in one call. Selecting a day reads that line
	// underneath; opening it is a second, deliberate step, so a tap on a small
	// square never lands on the wrong day.

	let mapYear = $state<YearApi | null>(null);
	let mapLoading = $state(false);
	let selectedDay = $state<string | null>(null);

	const mapDays = $derived(
		mapYear
			? toActivityDays(mapYear.days, new Date(mapYear.year, 0, 1), new Date(mapYear.year, 11, 31))
			: []
	);
	const mapLimits = $derived(activityThresholds(mapDays));
	/** Years worth switching to: the ones with recorded days, newest first. */
	const mapYears = $derived(years.filter((y) => y.days_recorded > 0).map((y) => y.year));

	async function loadMapYear(year: number) {
		mapLoading = true;
		const next = await getYear(year);
		if (next) {
			mapYear = next;
			selectedDay = null;
		}
		mapLoading = false;
	}

	$effect(() => {
		if (section !== 'days') return;
		untrack(() => {
			void loadYears();
			if (!mapYear && !mapLoading) void loadMapYear(new Date().getFullYear());
		});
	});

	const selectedLine = $derived.by(() => {
		if (!selectedDay || !mapYear) return null;
		const day = mapYear.days.find((d) => d.date === selectedDay);
		const count = day?.event_count ?? 0;
		// The lede sentence is plain words already, drawn as they are.
		const marked = veilMarks(ledeSentence(day?.lede) ?? '', 'text');
		return {
			date: new Date(selectedDay + 'T12:00:00').toLocaleDateString('en-US', {
				weekday: 'long',
				month: 'long',
				day: 'numeric',
			}),
			count: count ? `${count} ${count === 1 ? 'event' : 'events'}` : 'No events',
			lede: marked.markdown || null,
			phrases: marked.phrases,
			unwritten: count > 0 && !day?.narrated,
		};
	});

	// Handle day click from heatmap / chronicle / links
	function openDay(slug: string) {
		windowShellStore.openTabFromRoute(`/day/day_${slug}`);
	}

	function openEntity(entity: UnifiedEntity) {
		windowShellStore.openTabFromRoute(entity.route);
	}

	const today = new Date();
	const todaySlug = getLocalDateSlug(today);

</script>

<div class="wiki-view" style:--page-width={width.css}>
	<main class="content">
		{#if section === 'overview'}
			<WikiFront entities={findable} />
		{:else if section === 'history'}
			<div class="measure">
				<WikiHistory />
			</div>
		{:else if section === 'years'}
			<div class="measure">
				{#if !yearsLoaded}
					<p class="quiet">Loading…</p>
				{:else if years.length === 0}
					<p class="quiet">No years yet.</p>
				{:else}
					<ul class="years">
						{#each years as y (y.year)}
							<li>
								<a href="/year/{y.id}">{y.title ?? y.year}</a>
								<span class="quiet">
									{#if y.days_recorded === 0}
										nothing recorded{#if y.chapters.length} · {y.chapters[0]}{/if}
									{:else}
										{y.days_recorded} day{y.days_recorded === 1 ? '' : 's'} · {y.days_narrated}
										narrated{#if y.has_article} · written up{/if}
									{/if}
								</span>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		{:else if section === 'days'}
			<div class="days-wrap">
				<!-- How much data arrived is a fact about the collector rather
				     than about the life, so this lives on the days index, where
				     its counts are the subject, and not on the front page. -->
				<section class="year-map" aria-label="Activity">
					{#if mapYear}
						<ActivityHeatmap
							days={mapDays}
							thresholds={mapLimits}
							lastDay={todaySlug}
							label="Events per day in {mapYear.year}"
							period={String(mapYear.year)}
							selectedDate={selectedDay}
							onSelectDate={(date) => (selectedDay = selectedDay === date ? null : date)}
						>
							{#snippet actions()}
								{#if mapYears.length > 1}
									<div class="year-switch" role="group" aria-label="Year">
										{#each mapYears as y (y)}
											<button
												type="button"
												class="year-seg"
												aria-pressed={y === mapYear?.year}
												disabled={mapLoading}
												onclick={() => loadMapYear(y)}>{y}</button
											>
										{/each}
									</div>
								{/if}
							{/snippet}
							{#snippet footer()}
								{#if selectedLine && selectedDay}
									{@const slug = selectedDay}
									<p class="day-line">
										<span class="day-line-date">{selectedLine.date}:</span>
										{selectedLine.count}.
										{#key slug}
											{#if selectedLine.lede}
												<span use:veiled={{ hiding: veil.hiding, phrases: selectedLine.phrases }}
													>{selectedLine.lede}</span
												>
											{:else if selectedLine.unwritten}
												Unwritten.
											{/if}
										{/key}
										<TextAction inline onclick={() => openDay(slug)}>Open the day</TextAction>
									</p>
								{:else}
									<p class="day-line">
										{mapYear?.days_recorded ?? 0}
										{mapYear?.days_recorded === 1 ? 'day' : 'days'} recorded, {mapYear?.days_narrated ?? 0}
										narrated. Select a day to read its first line.
									</p>
								{/if}
							{/snippet}
						</ActivityHeatmap>
					{:else}
						<p class="quiet">Loading activity…</p>
					{/if}
				</section>

				<DaysChronicle onOpenDay={openDay} />
			</div>
		{:else if section === 'entities'}
			<div class="list-wrap">
				{#key entityTypePreset}
					<UniversalDataGrid
						items={listedEntities}
						columns={entityColumns}
						entityType="entities-list"
						viewModes={['list']}
						defaultViewMode="list"
						defaultGroupBy="entityType"
						showCounts={false}
						pageSize={100000}
						{loading}
						{error}
						emptyIcon="mynaui:users-group"
						emptyMessage="People, places and organizations show up here as your server finds them in your records. Add someone with New person."
						loadingMessage="Loading…"
						searchPlaceholder={entityTypePreset ? `Search ${typeConfig[entityTypePreset].plural.toLowerCase()}` : 'Search people, places, organizations'}
						onItemClick={openEntity}
						onItemContextMenu={entityContextMenu}
						onRetry={loadAllEntities}
					>
						{#snippet toolbarActions()}
							{#if !entityTypePreset || entityTypePreset === 'person'}
							<!-- People arrived only by resolution before this: from a
							     contact sync or an email sender. The people who matter
							     most are often the ones you never email. -->
							<Button
								variant="secondary"
								icon="mynaui:user-plus"
								onclick={addPerson}>New person</Button
							>
							{/if}
						{/snippet}

						{#snippet listRow(entity: UnifiedEntity)}
							<!-- A person shows their face, or their initials without one,
							     the same as on their page. Places and orgs keep a glyph. -->
							<ListRow
								icon={typeConfig[entity.entityType].icon}
								title={entity.name}
								line={detailLine(entity)}
								note={entity.isSelf ? 'You' : null}
							>
								{#snippet lead()}
									{#if entity.entityType === 'person'}
										<span class="row-picture"><PersonPicture name={entity.name} picture={entity.picture} size={28} /></span>
									{:else}
										<Icon icon={typeConfig[entity.entityType].icon} width="16" />
									{/if}
								{/snippet}
							</ListRow>
						{/snippet}
					</UniversalDataGrid>
				{/key}
			</div>
		{:else if section === 'identity'}
			<div class="identity-wrap">
				<NarrativeIdentitySection />
				<!-- Where the assistant's proposals wait. It may suggest an
				     addition; it may never make one. -->
				<NotesRail subjectType="narrative_identity" subjectId="nar_identity_001" />
			</div>
		{:else if section === 'chapters'}
			<div class="identity-wrap">
				<ChaptersSection />
			</div>
		{:else if section === 'things'}
			<div class="identity-wrap">
				<ThingsSection />
			</div>
		{/if}
	</main>
</div>

<style>
	/* ListRow sets its glyph 0.25rem down to sit on the name's first line; a
	   28px picture is taller than that line, so it moves up to center on it. */
	.row-picture {
		display: block;
		margin-top: -0.375rem;
	}

	.wiki-view {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	/* Mirrors the Page shell's box model, not just its numbers: padding on the
	   OUTER scroller, measure centred INSIDE it. Padding within the centred
	   block instead (what this used to do) lands the text ~48px further in
	   than every neighbouring room, so the columns don't line up when you move
	   between tabs even though both claim 72rem. */
	/* This room hand-rolls the Page shell, so it hand-rolls the shell's phone
	   gutter too — 3rem a side leaves 279px of a 375px screen, and the room's
	   own grids then push the page sideways. Mobile-first at the same 768px
	   step `Page.svelte` uses, so the two shells change measure on exactly the
	   same pixel. */
	.content {
		flex: 1;
		overflow-y: auto;
		min-height: 0;
		/* Bottom room is the home indicator's safe-area only — the phone has no
		   bottom chrome anymore. Same measure as `Page.svelte`. */
		padding: 2rem 1.25rem calc(1.5rem + env(safe-area-inset-bottom));
	}

	@media (min-width: 768px) {
		.content {
			padding: 3rem;
		}
	}

	/* This room's own wraps take the section's width (`usePageWidth`), on
	   the shell's 3rem padding, so the column does not shift between the
	   room's tabs or its neighbors. */
	.days-wrap {
		max-width: var(--page-width);
		width: 100%;
		margin: 0 auto;
	}
	.list-wrap {
		max-width: var(--page-width);
		width: 100%;
		margin: 0 auto;
	}

	/* The year's heatmap opens the days index: no rule, no heading, no
	   margin column. Its summary line is the heading, and the line under it
	   carries what the stats column used to. */
	.year-map {
		padding: 8px 0 48px;
	}

	.day-line {
		margin: 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.day-line-date {
		color: var(--color-foreground);
		font-weight: 500;
	}

	.year-switch {
		display: flex;
		gap: 4px;
		padding: 4px;
		border-radius: var(--radius-full);
		background: color-mix(in oklab, var(--color-foreground) 5%, var(--color-surface));
	}

	.year-seg {
		padding: 4px 12px;
		border: 0;
		border-radius: var(--radius-full);
		background: transparent;
		color: var(--color-foreground-muted);
		font: inherit;
		font-size: 13px;
		font-variant-numeric: tabular-nums;
		cursor: pointer;
	}

	.year-seg:hover {
		color: var(--color-foreground);
	}

	.year-seg[aria-pressed='true'] {
		background: var(--color-surface);
		color: var(--color-foreground);
	}

	.identity-wrap {
		max-width: var(--page-width);
		width: 100%;
		margin: 0 auto;
	}


	.quiet {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		margin: 0;
	}

	/* Years: a plain index, set to the reading measure. */
	.measure {
		max-width: var(--page-width);
		width: 100%;
		margin: 0 auto;
		padding: 1.5rem 0;
	}

	.years {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.years li {
		display: flex;
		justify-content: space-between;
		gap: 1rem;
		align-items: baseline;
		padding: 0.5rem 0;
		border-bottom: 1px solid var(--color-border-subtle);
	}

	.years a {
		color: var(--color-foreground);
		text-decoration: none;
	}

	.years a:hover {
		text-decoration: underline;
	}

</style>