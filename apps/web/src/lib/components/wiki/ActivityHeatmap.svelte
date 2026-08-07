<!--
	ActivityHeatmap.svelte

	GitHub-contributions-style heatmap showing data density over time.
	Shows approximately 1 year of days with color intensity based on activity level.
-->

<script lang="ts">
	import { getLocalDateSlug } from '$lib/utils/dateUtils';

	interface Props {
		/** Map of date slugs (YYYY-MM-DD) to activity level (0-4) */
		activityData?: Map<string, number>;
		/** Number of weeks to show (default: 26 for half year, fits better) */
		weeksToShow?: number;
		/** Callback when a day is clicked */
		onDayClick?: (date: Date, slug: string) => void;
	}

	let {
		activityData = new Map(),
		weeksToShow = 26,
		onDayClick
	}: Props = $props();

	const today = new Date();
	const todaySlug = getLocalDateSlug(today);

	// Generate grid data: array of weeks, each containing 7 days
	const gridData = $derived.by(() => {
		const weeks: { date: Date; slug: string; level: number; isToday: boolean; isFuture: boolean }[][] = [];

		// Start from weeksToShow weeks ago, aligned to Sunday
		const startDate = new Date(today);
		startDate.setDate(startDate.getDate() - (weeksToShow * 7) - today.getDay());

		for (let w = 0; w <= weeksToShow; w++) {
			const week: typeof weeks[0] = [];
			for (let d = 0; d < 7; d++) {
				const date = new Date(startDate);
				date.setDate(startDate.getDate() + (w * 7) + d);

				const slug = getLocalDateSlug(date);
				const level = activityData.get(slug) ?? 0;
				const isToday = slug === todaySlug;
				const isFuture = date > today;

				week.push({ date, slug, level, isToday, isFuture });
			}
			weeks.push(week);
		}

		return weeks;
	});

	// Month labels - find first occurrence of each month
	const monthLabels = $derived.by(() => {
		const labels: { month: string; colStart: number }[] = [];
		let lastMonth = -1;

		for (let w = 0; w < gridData.length; w++) {
			const firstDay = gridData[w][0];
			const month = firstDay.date.getMonth();

			if (month !== lastMonth) {
				labels.push({
					month: firstDay.date.toLocaleDateString("en-US", { month: "short" }),
					colStart: w,
				});
				lastMonth = month;
			}
		}

		return labels;
	});

	function handleDayClick(day: typeof gridData[0][0]) {
		if (!day.isFuture && onDayClick) {
			onDayClick(day.date, day.slug);
		}
	}

	function formatTooltip(day: typeof gridData[0][0]): string {
		const dateStr = day.date.toLocaleDateString("en-US", {
			weekday: "short",
			month: "short",
			day: "numeric",
			year: "numeric"
		});
		if (day.isFuture) return dateStr;
		if (day.level === 0) return `${dateStr} · No data`;
		const levels = ["", "Light", "Moderate", "Active", "Very active"];
		return `${dateStr} · ${levels[day.level]}`;
	}

	function getLevelClass(level: number): string {
		return `level-${level}`;
	}
</script>

<div class="heatmap">
	<!-- Month labels row -->
	<div class="month-row">
		<div class="day-labels-spacer"></div>
		<div class="month-labels">
			{#each monthLabels as label}
				<span class="month-label" style="left: {label.colStart * 13}px">
					{label.month}
				</span>
			{/each}
		</div>
	</div>

	<!-- Grid with day labels -->
	<div class="grid-row">
		<!-- Day labels -->
		<div class="day-labels">
			<span></span>
			<span>Mon</span>
			<span></span>
			<span>Wed</span>
			<span></span>
			<span>Fri</span>
			<span></span>
		</div>

		<!-- Weeks grid -->
		<div class="weeks-grid">
			{#each gridData as week}
				<div class="week-column">
					{#each week as day}
						<button
							class="day-cell {getLevelClass(day.level)}"
							class:is-today={day.isToday}
							class:is-future={day.isFuture}
							title={formatTooltip(day)}
							onclick={() => handleDayClick(day)}
							disabled={day.isFuture}
							aria-label={formatTooltip(day)}
						></button>
					{/each}
				</div>
			{/each}
		</div>
	</div>

	<!-- Legend -->
	<div class="legend">
		<span class="legend-text">Less</span>
		<div class="legend-cells">
			<span class="legend-cell level-0"></span>
			<span class="legend-cell level-1"></span>
			<span class="legend-cell level-2"></span>
			<span class="legend-cell level-3"></span>
			<span class="legend-cell level-4"></span>
		</div>
		<span class="legend-text">More</span>
	</div>
</div>

<style>
	/* Six months of 13px columns is ~366px — wider than a phone's content
	   column, and the absolutely-positioned month labels reach further still.
	   The heatmap scrolls itself rather than handing that width to the page:
	   the rows below keep their intrinsic width, and `contain` stops the swipe
	   chaining out to the shell when the grid reaches its end. */
	.heatmap {
		--cell-size: 10px;
		--cell-gap: 3px;
		--cell-radius: 2px;
		display: flex;
		flex-direction: column;
		gap: 4px;
		overflow-x: auto;
		overscroll-behavior-x: contain;
	}

	/* Month labels */
	.month-row {
		display: flex;
		height: 14px;
		width: max-content;
		min-width: 100%;
	}

	.day-labels-spacer {
		width: 28px;
		flex-shrink: 0;
	}

	.month-labels {
		position: relative;
		flex: 1;
	}

	.month-label {
		position: absolute;
		font-size: 10px;
		color: var(--color-foreground-muted);
	}

	/* Grid row */
	.grid-row {
		display: flex;
		gap: 4px;
		width: max-content;
		min-width: 100%;
	}

	/* On touch the cells stop being controls. A day is a 10px square with 190
	   neighbours: a finger cannot pick one, it can only pick *a* one — every
	   tap is a lottery that opens the wrong day. Six months at a tappable size
	   is not a heatmap any more, so the honest answer is that here it is a
	   picture of the year, read but not operated. Days remain reachable from
	   Home's stepper and from the wiki's own lists. */
	@media (max-width: 768px), (pointer: coarse) {
		.day-cell {
			pointer-events: none;
		}
	}

	.day-labels {
		display: flex;
		flex-direction: column;
		justify-content: space-around;
		width: 28px;
		flex-shrink: 0;
		font-size: 9px;
		color: var(--color-foreground-muted);
	}

	.day-labels span {
		height: var(--cell-size);
		line-height: var(--cell-size);
	}

	.weeks-grid {
		display: flex;
		gap: var(--cell-gap);
	}

	.week-column {
		display: flex;
		flex-direction: column;
		gap: var(--cell-gap);
	}

	.day-cell {
		width: var(--cell-size);
		height: var(--cell-size);
		border-radius: var(--cell-radius);
		border: none;
		padding: 0;
		cursor: pointer;
		transition: outline 0.1s ease;
	}

	.day-cell:hover:not(.is-future) {
		outline: 1px solid var(--color-foreground-muted);
		outline-offset: 1px;
	}

	.day-cell:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 1px;
	}

	/* Activity levels */
	.day-cell.level-0 {
		background: color-mix(in srgb, var(--color-foreground) 10%, transparent);
	}
	.day-cell.level-1 {
		background: color-mix(in srgb, var(--color-primary) 30%, transparent);
	}
	.day-cell.level-2 {
		background: color-mix(in srgb, var(--color-primary) 50%, transparent);
	}
	.day-cell.level-3 {
		background: color-mix(in srgb, var(--color-primary) 75%, transparent);
	}
	.day-cell.level-4 {
		background: var(--color-primary);
	}

	.day-cell.is-today {
		outline: 1px solid var(--color-foreground);
		outline-offset: 1px;
	}

	.day-cell.is-future {
		opacity: 0.3;
		cursor: default;
	}

	/* Legend */
	.legend {
		display: flex;
		align-items: center;
		justify-content: flex-start;
		gap: 6px;
		margin-top: 4px;
		padding-left: 32px;
	}

	.legend-text {
		font-size: 10px;
		color: var(--color-foreground-muted);
	}

	.legend-cells {
		display: flex;
		gap: 2px;
	}

	.legend-cell {
		width: var(--cell-size);
		height: var(--cell-size);
		border-radius: var(--cell-radius);
	}

	.legend-cell.level-0 {
		background: color-mix(in srgb, var(--color-foreground) 10%, transparent);
	}
	.legend-cell.level-1 {
		background: color-mix(in srgb, var(--color-primary) 30%, transparent);
	}
	.legend-cell.level-2 {
		background: color-mix(in srgb, var(--color-primary) 50%, transparent);
	}
	.legend-cell.level-3 {
		background: color-mix(in srgb, var(--color-primary) 75%, transparent);
	}
	.legend-cell.level-4 {
		background: var(--color-primary);
	}

	/* Responsive */
	@media (max-width: 640px) {
		.heatmap {
			--cell-size: 8px;
			--cell-gap: 2px;
		}

		.day-labels {
			display: none;
		}

		.day-labels-spacer {
			display: none;
		}
	}
</style>
