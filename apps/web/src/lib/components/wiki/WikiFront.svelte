<!--
	WikiFront.svelte
	The wiki's front page, at /wiki. A person comes here to look something up,
	to see what the wiki holds, and to read what was written about the last
	few days. The page leads with those, in that order: the finder, one row
	of sections with what each holds, then the people, places, organizations
	and things that come up most, each with when it last came up or what it
	is, then this week's written days and the same date in earlier years.

	A day with no article has no line, so the page never opens on an empty
	row. A lede is the article's own markdown and is drawn by DayLede, so
	its names are links.
-->
<script lang="ts">
	import { usePageWidth } from '$lib/stores/pageWidth.svelte';
	import { onMount } from 'svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { WIKI_MODE } from '$lib/sidebar/modes';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import DayLede from './DayLede.svelte';
	import { thingsStore } from '$lib/stores/things.svelte';
	import {
		getChapters,
		listOnThisDay,
		listYears,
		type ChapterApi,
		type OnThisDayApi,
		type ThingApi,
		type YearSummaryApi,
	} from '$lib/wiki/api';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import { loadFront, whenLabel, type FrontData } from '$lib/wiki/front';

	interface Findable {
		name: string;
		route: string;
		kind: string;
		type: 'person' | 'place' | 'org';
		subtitle: string | null;
		isSelf: boolean;
	}

	let { entities, entitiesLoaded = false }: { entities: Findable[]; entitiesLoaded?: boolean } = $props();

	const width = usePageWidth(() => 'medium', { part: () => 'overview' });

	const today = getLocalDateSlug();

	let front = $state<FrontData | null>(null);
	let chapters = $state<ChapterApi[] | null>(null);
	let years = $state<YearSummaryApi[] | null>(null);
	let onThisDay = $state<OnThisDayApi[]>([]);

	onMount(async () => {
		void thingsStore.load();
		const [data, chs, ys, otd] = await Promise.all([
			loadFront(today).catch((e) => {
				console.error('[WikiFront] week failed:', e);
				return null;
			}),
			getChapters().catch(() => null),
			listYears().catch(() => null),
			listOnThisDay().catch(() => []),
		]);
		front = data;
		chapters = chs;
		years = ys;
		onThisDay = otd.filter((d) => d.lede).slice(0, 2);
	});

	// --- Finding ---

	let query = $state('');
	const matches = $derived.by(() => {
		const q = query.trim().toLowerCase();
		if (!q) return [];
		return entities.filter((e) => e.name.toLowerCase().includes(q)).slice(0, 8);
	});

	function onFindKey(e: KeyboardEvent) {
		if (e.key === 'Enter' && matches[0]) open(matches[0].route);
		if (e.key === 'Escape') query = '';
	}

	// --- Moving ---

	/** The wiki's own sections move this tab, so Back returns here. */
	function go(href: string, label: string) {
		windowShellStore.navigate(href, { label: `Wiki · ${label}` });
	}

	function open(route: string) {
		windowShellStore.openTabFromRoute(route);
	}

	function openDay(date: string) {
		open(`/day/day_${date}`);
	}

	// --- What the wiki holds ---

	const people = $derived(entities.filter((e) => e.type === 'person' && !e.isSelf));
	const places = $derived(entities.filter((e) => e.type === 'place'));
	const orgs = $derived(entities.filter((e) => e.type === 'org'));
	const things = $derived(thingsStore.things);

	/** How much is behind each section link. Null until it is known, so a
	 *  section never shows a 0 it has not counted. */
	const counts = $derived<Record<string, number | null>>({
		days: years ? years.reduce((n, y) => n + y.days_recorded, 0) : null,
		years: years ? years.filter((y) => y.days_recorded > 0).length : null,
		chapters: chapters ? chapters.filter((c) => c.kind === 'chapter').length : null,
		people: entitiesLoaded ? people.length : null,
		places: entitiesLoaded ? places.length : null,
		orgs: entitiesLoaded ? orgs.length : null,
		things: thingsStore.loaded ? things.length : null,
	});

	/** Time and subjects, as the sidebar's wiki rows list them. Today has a
	 *  door of its own in the sidebar, and its line is the first of This week. */
	const timeRows = WIKI_MODE.rows.filter((r) => r.group === 'Time' && r.id !== 'today');
	const subjectRows = WIKI_MODE.rows.filter((r) => r.group === 'Subjects');

	// --- The lists ---

	const LIST_MAX = 5;

	interface ListRow {
		name: string;
		route: string;
		meta: string;
	}

	function capitalized(s: string | null | undefined): string {
		const t = (s ?? '').trim();
		return t ? t.charAt(0).toUpperCase() + t.slice(1) : '';
	}

	/** When it last came up, in the two weeks the page can see, else what it is. */
	function entityRows(list: Findable[], seenWord: string): ListRow[] {
		return list.slice(0, LIST_MAX).map((e) => {
			const last = front?.lastSeen.get(e.route);
			const when = last ? whenLabel(last, today) : null;
			// "Seen yesterday", "Seen Monday", "Seen Nov 3".
			const said = when === 'Today' || when === 'Yesterday' ? when.toLowerCase() : when;
			return { name: e.name, route: e.route, meta: said ? `${seenWord} ${said}` : capitalized(e.subtitle) };
		});
	}

	function shortDate(iso: string): string {
		const d = new Date(`${iso.slice(0, 10)}T12:00:00`);
		const sameYear = iso.slice(0, 4) === today.slice(0, 4);
		return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: sameYear ? undefined : 'numeric' });
	}

	function thingMeta(t: ThingApi): string {
		if (t.ended_at) return `Until ${shortDate(t.ended_at)}`;
		if (t.started_at) return `Since ${shortDate(t.started_at)}`;
		return '';
	}

	const lists = $derived([
		{ id: 'people', title: 'People', href: '/wiki/people', rows: entityRows(people, 'Seen') },
		{ id: 'places', title: 'Places', href: '/wiki/places', rows: entityRows(places, 'Visited') },
		{ id: 'orgs', title: 'Organizations', href: '/wiki/orgs', rows: entityRows(orgs, 'Came up') },
		{
			id: 'things',
			title: 'Things',
			href: '/wiki/things',
			rows: things.slice(0, LIST_MAX).map((t) => ({ name: t.title, route: `/thing/${t.id}`, meta: thingMeta(t) })),
		},
	]);

	// --- Days ---

	function dayName(date: string): string {
		if (date === today) return 'Today';
		return new Date(`${date}T12:00:00`).toLocaleDateString('en-US', { weekday: 'long' });
	}

	function dayDate(date: string, long = false): string {
		return new Date(`${date}T12:00:00`).toLocaleDateString('en-US', {
			weekday: long ? 'long' : undefined,
			month: 'short',
			day: 'numeric',
		});
	}
</script>

<div style:max-width={width.css} class="front">
	<h1>Wiki</h1>

	<div class="find">
		<label class="find-box">
			<AtlasIcon name="search" size={18} bare />
			<span class="sr-only">Find in your wiki</span>
			<input
				type="text"
				placeholder="Find a person, place, or day"
				bind:value={query}
				onkeydown={onFindKey}
				autocomplete="off"
			/>
		</label>
		{#if matches.length}
			<ul class="find-results">
				{#each matches as m (m.route)}
					<li>
						<button type="button" onclick={() => open(m.route)}>
							<span>{m.name}</span>
							<span class="find-kind">{m.kind}</span>
						</button>
					</li>
				{/each}
			</ul>
		{:else if query.trim()}
			<p class="find-none">Nothing in your wiki matches “{query.trim()}”.</p>
		{/if}
	</div>

	{#snippet door(row: (typeof WIKI_MODE.rows)[number])}
		{@const n = counts[row.id]}
		<button type="button" class="door" onclick={() => go(row.href, row.label)}>
			{#if row.glyph}<AtlasIcon name={row.glyph} size={16} bare />{/if}{row.label}{#if n != null}<span class="door-count">{n.toLocaleString('en-US')}</span>{/if}
		</button>
	{/snippet}

	<nav class="doors" aria-label="Wiki sections">
		<div class="door-group">
			{#each timeRows as row (row.id)}{@render door(row)}{/each}
		</div>
		<div class="door-group">
			{#each subjectRows as row (row.id)}{@render door(row)}{/each}
		</div>
	</nav>

	<div class="lists">
		{#each lists as list (list.id)}
			{@const n = counts[list.id]}
			<section class="list">
				<header class="list-head">
					<h2>{list.title}{#if n != null}<span class="list-count">{n.toLocaleString('en-US')}</span>{/if}</h2>
					<button type="button" class="see-all" onclick={() => go(list.href, list.title)}>See all</button>
				</header>
				{#if list.rows.length}
					<ul>
						{#each list.rows as row (row.route)}
							<li>
								<button type="button" class="ent" onclick={() => open(row.route)}>
									<span class="ent-name">{row.name}</span>
									{#if row.meta}<span class="ent-meta">{row.meta}</span>{/if}
								</button>
							</li>
						{/each}
					</ul>
				{:else if n === 0}
					<p class="list-empty">None yet.</p>
				{/if}
			</section>
		{/each}
	</div>

	{#if front?.week.length}
		<section class="recent">
			<h2>This week</h2>
			<div class="days">
				{#each front.week as d (d.date)}
					<div class="row day">
						<button type="button" class="when" onclick={() => openDay(d.date)}>
							<span class="when-name">{dayName(d.date)}</span>
							<span class="when-date">{dayDate(d.date, d.date === today)}</span>
						</button>
						<p class="lede"><DayLede markdown={d.lede} /></p>
					</div>
				{/each}
			</div>
			<button type="button" class="more" onclick={() => go('/wiki/days', 'Days')}>All days →</button>
		</section>
	{/if}

	{#if onThisDay.length}
		<section class="recent">
			<h2>On this day</h2>
			<div class="days">
				{#each onThisDay as d (d.date)}
					<div class="row day">
						<button type="button" class="when" onclick={() => openDay(d.date)}>
							<span class="when-name">{d.date.slice(0, 4)}</span>
							<span class="when-date">{dayDate(d.date, true)}</span>
						</button>
						<p class="lede"><DayLede markdown={d.lede ?? ''} /></p>
					</div>
				{/each}
			</div>
		</section>
	{/if}

	<footer>
		<button type="button" class="more quiet-link" onclick={() => go('/wiki/history', 'History')}>
			<AtlasIcon name="history" size={16} bare />History
		</button>
	</footer>
</div>

<style>
	.front {
		margin: 0 auto;
		padding-bottom: 2rem;
	}

	h1 {
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 2.625rem;
		line-height: 1.1;
		margin: 0 0 1.375rem;
		color: var(--color-foreground);
	}

	h2 {
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 1.5625rem;
		margin: 0;
		color: var(--color-foreground);
	}

	button {
		font: inherit;
		color: inherit;
		background: none;
		border: none;
		padding: 0;
		text-align: left;
		cursor: pointer;
	}

	ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	/* Finding */
	.find {
		position: relative;
	}

	.find-box {
		display: flex;
		align-items: center;
		gap: 0.625rem;
		height: 3rem;
		padding: 0 1rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		color: var(--color-foreground-subtle);
		background: var(--color-surface);
	}

	.find-box:focus-within {
		border-color: var(--color-border-strong);
	}

	.find-box input {
		flex: 1;
		min-width: 0;
		border: none;
		outline: none;
		background: transparent;
		font: inherit;
		font-size: 1rem;
		color: var(--color-foreground);
	}

	.find-results {
		margin: 0.375rem 0 0;
		padding: 0.25rem;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
	}

	.find-results button {
		display: flex;
		width: 100%;
		justify-content: space-between;
		gap: 1rem;
		padding: 0.5rem 0.75rem;
		border-radius: var(--radius-md);
		font-size: 0.9375rem;
	}

	.find-results button:hover,
	.find-results button:focus-visible {
		background: var(--color-background-hover);
	}

	.find-kind,
	.find-none {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.find-none {
		margin: 0.5rem 0 0 0.25rem;
	}

	/* Section links: time, then subjects, each group on its own run so a
	   wrap never strands one link of a group beside the other group. */
	.doors {
		display: flex;
		flex-wrap: wrap;
		gap: 0.625rem 2.25rem;
		margin-top: 1.125rem;
	}

	.door-group {
		display: flex;
		flex-wrap: wrap;
		gap: 0.625rem 1.375rem;
	}

	.door {
		display: inline-flex;
		align-items: center;
		gap: 0.4375rem;
		font-size: 0.9375rem;
		color: var(--color-foreground-muted);
	}

	.door:hover {
		color: var(--color-foreground);
	}

	.door-count,
	.list-count {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.door-count {
		margin-left: 0.125rem;
	}

	/* People, places, organizations and things, two to a row. */
	.lists {
		display: grid;
		grid-template-columns: repeat(2, minmax(0, 1fr));
		gap: 3rem 3.5rem;
		margin-top: 3rem;
	}

	.list-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 1rem;
		padding-bottom: 0.5rem;
		margin-bottom: 0.375rem;
		border-bottom: 1px solid var(--color-border);
	}

	.list-count {
		margin-left: 0.625rem;
		font-family: var(--font-sans);
	}

	.see-all {
		font-size: 0.875rem;
		color: var(--color-primary);
	}

	.see-all:hover {
		text-decoration: underline;
	}

	.ent {
		display: flex;
		align-items: baseline;
		gap: 1rem;
		width: 100%;
		padding: 0.5rem 0;
	}

	.ent-name {
		flex: 1;
		min-width: 0;
		font-size: 0.9375rem;
		color: var(--color-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.ent:hover .ent-name {
		text-decoration: underline;
	}

	.ent-meta {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	.list-empty {
		margin: 0.5rem 0 0;
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}

	/* Day rows */
	.recent {
		margin-top: 3.5rem;
	}

	.recent h2 {
		margin-bottom: 0.375rem;
	}

	.row {
		display: grid;
		grid-template-columns: 7.5rem minmax(0, 1fr);
		gap: 1.25rem;
		width: 100%;
	}

	.day {
		padding: 0.9375rem 0;
		border-bottom: 1px solid var(--color-border-subtle);
	}

	.days .day:last-child {
		border-bottom: none;
	}

	.when {
		display: block;
		align-self: start;
	}

	.when-name {
		display: block;
		font-size: 0.9375rem;
		line-height: 1.3;
		color: var(--color-foreground);
	}

	.when:hover .when-name {
		color: var(--color-primary);
	}

	.when-date {
		display: block;
		margin-top: 0.125rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.lede {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 1.15625rem;
		line-height: 1.42;
		color: var(--color-foreground);
	}

	.more {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		margin-top: 0.875rem;
		font-size: 0.875rem;
		color: var(--color-foreground-muted);
	}

	.more:hover {
		color: var(--color-foreground);
	}

	footer {
		margin-top: 4rem;
		padding-top: 1.25rem;
		border-top: 1px solid var(--color-border-subtle);
	}

	.quiet-link {
		margin-top: 0;
		color: var(--color-foreground-subtle);
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		overflow: hidden;
		clip: rect(0 0 0 0);
		white-space: nowrap;
	}

	@media (max-width: 767px) {
		.lists {
			grid-template-columns: minmax(0, 1fr);
			gap: 2.5rem;
		}

		.row {
			grid-template-columns: minmax(0, 1fr);
			gap: 0.375rem;
		}
	}
</style>
