<!--
	DaysChronicle.svelte

	The wiki's temporal spine: a year of activity as a calendar, then the
	recent record as a month-grouped chronicle. Each day is one line — date,
	epigraph if the night's narration has run, an honest "unwritten" stub if
	it hasn't. Reads like an annal, not a feed.
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import ActivityHeatmap from './ActivityHeatmap.svelte';
	import { getLocalDateSlug } from '$lib/utils/dateUtils';
	import { listDayActivity, listDays, type DayActivityApi } from '$lib/wiki/api';
	import { toActivityLevels } from '$lib/wiki/activity';

	interface Props {
		onOpenDay: (slug: string) => void;
	}

	let { onOpenDay }: Props = $props();

	interface DayRow {
		slug: string;
		dayLabel: string; // "Mon 28"
		epigraph: string | null;
		narrated: boolean;
		eventCount: number;
	}

	interface MonthGroup {
		key: string;
		label: string; // "July 2026"
		days: DayRow[];
		narratedCount: number;
	}

	const CHRONICLE_DAYS = 180;

	let loading = $state(true);
	let activityData = $state<Map<string, number>>(new Map());
	let months = $state<MonthGroup[]>([]);

	onMount(async () => {
		try {
			const end = new Date();
			const calStart = new Date();
			calStart.setDate(calStart.getDate() - 52 * 7);
			const listStart = new Date();
			listStart.setDate(listStart.getDate() - CHRONICLE_DAYS);

			const [activity, days] = await Promise.all([
				listDayActivity(getLocalDateSlug(calStart), getLocalDateSlug(end)),
				listDays(getLocalDateSlug(listStart), getLocalDateSlug(end)),
			]);

			activityData = toActivityLevels(activity);

			const countByDate = new Map<string, DayActivityApi>(
				activity.map((a) => [a.date, a])
			);

			// listDays comes back date DESC — group in place.
			const grouped = new Map<string, MonthGroup>();
			for (const day of days) {
				const date = new Date(day.date + 'T12:00:00');
				const key = day.date.slice(0, 7);
				let group = grouped.get(key);
				if (!group) {
					group = {
						key,
						label: date.toLocaleDateString('en-US', {
							month: 'long',
							year: 'numeric',
						}),
						days: [],
						narratedCount: 0,
					};
					grouped.set(key, group);
				}
				const narrated = day.article != null && day.article !== '';
				if (narrated) group.narratedCount += 1;
				group.days.push({
					slug: day.date,
					dayLabel: date.toLocaleDateString('en-US', {
						weekday: 'short',
						day: 'numeric',
					}),
					epigraph: day.epigraph,
					narrated,
					eventCount: countByDate.get(day.date)?.event_count ?? 0,
				});
			}
			months = [...grouped.values()];
		} catch (e) {
			console.error('Failed to load chronicle:', e);
		} finally {
			loading = false;
		}
	});

	function handleCalendarClick(_date: Date, slug: string) {
		onOpenDay(slug);
	}
</script>

<div class="chronicle">
	<section class="cal">
		<ActivityHeatmap {activityData} weeksToShow={52} onDayClick={handleCalendarClick} />
	</section>

	{#if loading}
		<p class="quiet">Loading the record…</p>
	{:else if months.length === 0}
		<p class="quiet">
			No days on record yet. Entries appear here as your sources come in.
		</p>
	{:else}
		{#each months as month (month.key)}
			<section class="month">
				<header class="month-head">
					<h2>{month.label}</h2>
					<span class="month-note">
						{month.days.length}
						{month.days.length === 1 ? 'day' : 'days'} · {month.narratedCount} narrated
					</span>
				</header>
				<ol class="days">
					{#each month.days as day (day.slug)}
						<li>
							<button class="day" onclick={() => onOpenDay(day.slug)}>
								<span class="day-date">{day.dayLabel}</span>
								{#if day.epigraph}
									<span class="day-epigraph">{day.epigraph}</span>
								{:else if day.narrated}
									<span class="day-epigraph">Narrated, without an epigraph</span>
								{:else}
									<span class="day-stub">Unwritten</span>
								{/if}
								{#if day.eventCount > 0}
									<span class="day-count"
										>{day.eventCount}
										{day.eventCount === 1 ? 'event' : 'events'}</span
									>
								{/if}
							</button>
						</li>
					{/each}
				</ol>
			</section>
		{/each}
	{/if}
</div>

<style>
	.chronicle {
		display: flex;
		flex-direction: column;
		gap: 2.5rem;
	}

	.cal {
		overflow-x: auto;
		padding-bottom: 0.25rem;
	}

	.quiet {
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
		margin: 0;
	}

	.month-head {
		display: flex;
		align-items: baseline;
		gap: 0.75rem;
		border-bottom: 1px solid var(--color-border);
		padding-bottom: 0.5rem;
		margin-bottom: 0.25rem;
	}

	.month-head h2 {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 1.125rem;
		font-weight: 500;
		color: var(--color-foreground);
		margin: 0;
	}

	.month-note {
		margin-left: auto;
		font-size: 0.6875rem;
		letter-spacing: 0.04em;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.days {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.day {
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

	.day:hover .day-date {
		color: var(--color-primary);
	}

	.day-date {
		flex: none;
		width: 4.25rem;
		font-size: 0.75rem;
		letter-spacing: 0.02em;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		transition: color 0.12s ease;
	}

	.day-epigraph {
		flex: 1;
		min-width: 0;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 0.9375rem;
		line-height: 1.4;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	/* A day the pipeline has seen but nobody has written — the wiki's
	   red link. Muted, not alarming: an invitation, not an error. */
	.day-stub {
		flex: 1;
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	.day-count {
		flex: none;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	@media (max-width: 640px) {
		.day-count {
			display: none;
		}
	}
</style>
