<!--
	WikiFront.svelte
	The wiki's front page, at /wiki. A person comes here to look something up,
	to read what was written about the last few days, to see who and where has
	come up lately, and to go back in time. The page leads with those four, in
	that order.

	The week's lines and the place swatches draw real records: a gray bar is
	time away from home, a dot is a calendar item or a workout at its hour,
	and a swatch's colors are the hours you spend at that place.
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';
	import { WIKI_MODE } from '$lib/sidebar/modes';
	import AtlasIcon from '$lib/components/sidebar/AtlasIcon.svelte';
	import { getSkyDay } from '$lib/api/client';
	import { getChapters, listOnThisDay, type ChapterApi, type OnThisDayApi } from '$lib/wiki/api';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import {
		loadFront,
		stripOf,
		hourLight,
		whenLabel,
		type DayStrip,
		type FrontData,
	} from '$lib/wiki/front';

	interface Findable {
		name: string;
		route: string;
		kind: string;
	}

	let { entities }: { entities: Findable[] } = $props();

	const today = getLocalDateSlug();

	let front = $state<FrontData | null>(null);
	let chapters = $state<ChapterApi[]>([]);
	let onThisDay = $state<(OnThisDayApi & { strip: DayStrip | null })[]>([]);

	onMount(async () => {
		const [data, chs, otd] = await Promise.all([
			loadFront(today).catch((e) => {
				console.error('[WikiFront] week failed:', e);
				return null;
			}),
			getChapters().catch(() => []),
			listOnThisDay().catch(() => []),
		]);
		front = data;
		chapters = chs;
		const written = otd.filter((d) => d.lede).slice(0, 2);
		onThisDay = await Promise.all(
			written.map(async (d) => {
				const sky = await getSkyDay(d.date).catch(() => null);
				return { ...d, strip: sky ? stripOf(sky, data?.homeId ?? null) : null };
			})
		);
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

	/** Time and subjects, as the sidebar's wiki rows list them. Today is the
	 *  first line of This week, so it has no link of its own here. */
	const timeRows = WIKI_MODE.rows.filter((r) => r.group === 'Time' && r.id !== 'today');
	const subjectRows = WIKI_MODE.rows.filter((r) => r.group === 'Subjects');

	// --- Drawing ---

	const HOUR_TICKS = [
		{ h: 0, label: '12a' },
		{ h: 6, label: '6a' },
		{ h: 12, label: '12p' },
		{ h: 18, label: '6p' },
	];

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

	/** Chapters on one line, each as wide as its years. */
	const life = $derived.by(() => {
		const now = Date.now();
		return chapters.map((c) => {
			const start = new Date(c.started_at).getTime();
			const end = c.ended_at ? new Date(c.ended_at).getTime() : now;
			return {
				id: c.id,
				title: c.title ?? 'An unnamed stretch',
				years: `${c.started_at.slice(0, 4)}–${c.ended_at ? c.ended_at.slice(0, 4) : 'now'}`,
				weight: Math.max(0.5, (end - start) / (365.25 * 86_400_000)),
				current: c.is_current || !c.ended_at,
			};
		});
	});

	/** A place's hours: the ones you keep most painted solid, the rest faint. */
	function swatch(hours: number[]) {
		const max = Math.max(...hours);
		if (max <= 0) return [];
		return hours
			.map((m, h) => ({ h, strength: m / max }))
			.filter((b) => b.strength >= 0.2)
			.map((b) => ({ x: b.h, fill: hourLight(b.h + 0.5), opacity: b.strength >= 0.5 ? 1 : 0.55 }));
	}
</script>

<div class="front">
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

	<nav class="doors" aria-label="Wiki sections">
		{#each timeRows as row (row.id)}
			<button type="button" class="door" onclick={() => go(row.href, row.label)}>
				{#if row.glyph}<AtlasIcon name={row.glyph} size={16} bare />{/if}{row.label}
			</button>
		{/each}
		<span class="doors-gap" aria-hidden="true"></span>
		{#each subjectRows as row (row.id)}
			<button type="button" class="door" onclick={() => go(row.href, row.label)}>
				{#if row.glyph}<AtlasIcon name={row.glyph} size={16} bare />{/if}{row.label}
			</button>
		{/each}
	</nav>

	{#snippet strip(s: DayStrip)}
		<svg class="strip" viewBox="0 0 120 14" aria-hidden="true">
			<rect x="0" y="6.5" width={s.until * 5} height="1" class="strip-track" />
			{#each s.stays as [a, b], i (i)}
				<rect x={a * 5} y="5" width={Math.max(2, (b - a) * 5)} height="4" rx="2" class="strip-stay" />
			{/each}
			{#each s.moments as h, i (i)}
				<circle cx={h * 5} cy="7" r="2.6" class="strip-moment" />
			{/each}
		</svg>
	{/snippet}

	<div class="cols">
		<div class="main">
			<section>
				<h2>This week</h2>
				<div class="row axis" aria-hidden="true">
					<div class="ticks">
						{#each HOUR_TICKS as t (t.h)}
							<span style:left="{t.h * 5}px">{t.label}</span>
						{/each}
					</div>
				</div>
				{#if front}
					<div class="days">
					{#each front.week as d (d.date)}
						<button type="button" class="row day" onclick={() => openDay(d.date)}>
							<span class="when">
								<span class="when-name">{dayName(d.date)}</span>
								<span class="when-date">{dayDate(d.date, d.date === today)}</span>
								{#if d.strip}{@render strip(d.strip)}{/if}
							</span>
							{#if d.lede}
								<span class="lede">{d.lede}</span>
							{:else if d.places.length}
								<span class="lede faint">
									{d.date === today ? 'So far: ' : ''}{d.places.join(', ')}.
								</span>
							{:else}
								<span></span>
							{/if}
						</button>
					{/each}
					</div>
				{/if}
				<button type="button" class="more" onclick={() => go('/wiki/days', 'Days')}>All days →</button>
			</section>

			{#if life.length}
				<section>
					<h2>Your life</h2>
					<div class="life">
						{#each life as c (c.id)}
							<button
								type="button"
								class="life-seg"
								style:flex-grow={c.weight}
								onclick={() => go('/wiki/chapters', 'Chapters')}
							>
								<span class="life-bar" class:current={c.current}></span>
								<span class="life-name">{c.title}</span>
								<span class="life-years">{c.years}</span>
							</button>
						{/each}
					</div>
					<div class="more-row">
						<button type="button" class="more" onclick={() => go('/timeline', 'Timeline')}>Timeline →</button>
						<button type="button" class="more" onclick={() => go('/wiki/years', 'Years')}>Years →</button>
					</div>
				</section>
			{/if}

			{#if onThisDay.length}
				<section>
					<h2>On this day</h2>
					<div class="days">
					{#each onThisDay as d (d.date)}
						<button type="button" class="row day" onclick={() => openDay(d.date)}>
							<span class="when">
								<span class="when-name">{d.date.slice(0, 4)}</span>
								<span class="when-date">{dayDate(d.date, true)}</span>
								{#if d.strip}{@render strip(d.strip)}{/if}
							</span>
							<span class="lede">{d.lede}</span>
						</button>
					{/each}
					</div>
				</section>
			{/if}
		</div>

		{#if front && (front.people.length || front.places.length)}
			<aside>
				<h2>Lately</h2>
				{#if front.people.length}
					<p class="sub">People</p>
					{#each front.people as p (p.url)}
						<button type="button" class="ent" onclick={() => open(p.url)}>
							<span class="initials" aria-hidden="true">
								{p.label
									.split(/\s+/)
									.map((w) => w[0])
									.join('')
									.slice(0, 2)
									.toUpperCase()}
							</span>
							<span class="ent-name">{p.label}</span>
							<span class="ent-when">{whenLabel(p.last, today)}</span>
						</button>
					{/each}
				{/if}
				{#if front.places.length}
					<p class="sub">Places</p>
					{#each front.places as p (p.url)}
						<button type="button" class="ent" onclick={() => open(p.url)}>
							<svg class="swatch" viewBox="0 0 24 16" aria-hidden="true">
								<rect width="24" height="16" class="swatch-ground" />
								{#each swatch(p.hours ?? []) as b (b.x)}
									<rect x={b.x} y="0" width="1.02" height="16" fill={b.fill} opacity={b.opacity} />
								{/each}
							</svg>
							<span class="ent-name">{p.label}</span>
							<span class="ent-when">{whenLabel(p.last, today)}</span>
						</button>
					{/each}
				{/if}
				<button type="button" class="more" onclick={() => go('/wiki/entities', 'Entities')}>
					All people and places →
				</button>
			</aside>
		{/if}
	</div>

	<footer>
		<button type="button" class="more quiet-link" onclick={() => go('/wiki/history', 'History')}>
			<AtlasIcon name="history" size={16} bare />History
		</button>
	</footer>
</div>

<style>
	.front {
		max-width: 60rem;
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
		margin: 0 0 0.375rem;
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
		list-style: none;
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

	/* Section links */
	.doors {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 0.625rem 1.375rem;
		margin-top: 1.125rem;
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

	.doors-gap {
		width: 0.875rem;
	}

	/* Columns */
	.cols {
		display: grid;
		grid-template-columns: minmax(0, 1fr) 17rem;
		gap: 4.5rem;
		margin-top: 3.25rem;
	}

	section + section {
		margin-top: 3.5rem;
	}

	/* Day rows */
	.row {
		display: grid;
		grid-template-columns: 7.5rem minmax(0, 1fr);
		gap: 1.25rem;
		width: 100%;
	}

	.axis {
		margin-top: 0.25rem;
	}

	.ticks {
		position: relative;
		width: 120px;
		height: 14px;
	}

	.ticks span {
		position: absolute;
		top: 0;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		transform: translateX(-50%);
	}

	.ticks span:first-child {
		transform: none;
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
	}

	.when-name {
		display: block;
		font-size: 0.9375rem;
		line-height: 1.3;
		color: var(--color-foreground);
	}

	.when-date {
		display: block;
		margin-top: 0.125rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.lede {
		font-family: var(--font-serif);
		font-size: 1.15625rem;
		line-height: 1.42;
		color: var(--color-foreground);
	}

	.lede.faint {
		color: var(--color-foreground-subtle);
	}

	.day:hover .lede:not(.faint) {
		color: var(--color-foreground-muted);
	}

	.strip {
		display: block;
		width: 120px;
		height: 14px;
		margin-top: 0.5625rem;
	}

	.strip-track {
		fill: var(--color-border);
	}

	.strip-stay {
		fill: var(--color-border-strong);
	}

	.strip-moment {
		fill: var(--color-foreground-muted);
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

	.more-row {
		display: flex;
		gap: 1.5rem;
	}

	/* Lifeline */
	.life {
		display: flex;
		gap: 4px;
		margin-top: 1rem;
	}

	.life-seg {
		display: block;
		flex-basis: 0;
		min-width: 0;
	}

	.life-bar {
		display: block;
		height: 10px;
		border-radius: var(--radius-full);
		background: var(--color-border-strong);
	}

	.life-bar.current {
		background: var(--color-foreground);
	}

	.life-seg:hover .life-bar:not(.current) {
		background: var(--color-foreground-subtle);
	}

	.life-name {
		display: block;
		margin-top: 0.625rem;
		font-family: var(--font-serif);
		font-size: 1.0625rem;
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.life-years {
		display: block;
		font-size: 0.78125rem;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	/* Lately */
	.sub {
		margin: 1.125rem 0 0.125rem;
		font-size: 0.78125rem;
		color: var(--color-foreground-subtle);
	}

	.ent {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		width: 100%;
		height: 2.875rem;
	}

	.initials {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 1.875rem;
		height: 1.875rem;
		flex: none;
		border-radius: 50%;
		background: var(--color-border-subtle);
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}

	.swatch {
		width: 1.875rem;
		height: 1.25rem;
		flex: none;
		border-radius: var(--radius-sm);
	}

	.swatch-ground {
		fill: var(--color-border-subtle);
	}

	.ent-name {
		flex: 1;
		min-width: 0;
		font-size: 0.9375rem;
		color: var(--color-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.ent:hover .ent-name {
		color: var(--color-foreground-muted);
	}

	.ent-when {
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
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
		.cols {
			grid-template-columns: minmax(0, 1fr);
			gap: 3rem;
		}

		.life-years {
			display: none;
		}
	}
</style>
