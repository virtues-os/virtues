<script lang="ts">
	/**
	 * The last four weeks of Changed lately as one line of days. Each dot is
	 * one change or pairing, on the day it happened; changes on the same day
	 * stack, up to four, with a ring on top when there were more. Every other week is shaded so the weeks read at a glance, and
	 * the last seven days are drawn in the accent color.
	 *
	 * `listed` are the changes the list under the chart shows, drawn a little
	 * larger; `hot` is the one under the pointer in that list, ringed.
	 * There is no count anywhere: the dots are the record.
	 */
	import type { SettingsChange } from '$lib/api/client';

	let {
		changes,
		listed = [],
		hot = null,
	}: { changes: SettingsChange[]; listed?: string[]; hot?: string | null } = $props();

	const DAYS = 28;
	const HEIGHT = 60;
	const BASE = 36;
	const PAD = 6;
	/** The most dots one day's column draws. */
	const STACK = 4;

	let width = $state(0);

	const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
	const today = startOf(new Date());

	/** Days before today, 0 for today. */
	function daysAgo(iso: string): number {
		return Math.round((today - startOf(new Date(iso))) / 86_400_000);
	}

	function x(ago: number): number {
		return PAD + ((DAYS - 1 - ago) * (width - PAD * 2)) / (DAYS - 1);
	}

	const step = $derived(width > 0 ? (width - PAD * 2) / (DAYS - 1) : 0);

	/** Each change's dot, stacked upward within its day, oldest at the bottom. */
	const dots = $derived.by(() => {
		const byDay = new Map<number, SettingsChange[]>();
		for (const change of changes) {
			const ago = daysAgo(change.occurred_at);
			if (ago < 0 || ago >= DAYS) continue;
			if (!byDay.has(ago)) byDay.set(ago, []);
			byDay.get(ago)!.push(change);
		}
		return [...byDay].flatMap(([ago, day]) => {
			// Newest first from the server; the oldest sits on the bottom. A
			// busy day shows its first few and a ring on top for the rest, so
			// the column stays a column of dots.
			const stack = [...day].reverse();
			const shown = stack.slice(0, STACK);
			return shown.map((change, i) => ({
				change,
				ago,
				cy: BASE - 9 - i * 7,
				more: i === STACK - 1 && stack.length > STACK,
			}));
		});
	});

	/** The week each band starts on, oldest first; the second and fourth are shaded. */
	const weeks = [27, 20, 13, 6];

	function dayLabel(ago: number): string {
		const day = new Date(today - ago * 86_400_000);
		return day.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
	}
</script>

<div class="chart" bind:clientWidth={width}>
	{#if width > 0}
		<svg {width} height={HEIGHT} viewBox="0 0 {width} {HEIGHT}" role="img" aria-label="Your changes over the last four weeks, one dot each">
			{#each weeks as start, i (start)}
				{#if i % 2 === 1}
					{@const left = x(start) - step / 2}
					{@const right = i + 1 < weeks.length ? x(weeks[i + 1]) - step / 2 : width}
					<rect class="band" x={left} y="4" width={right - left} height={BASE - 2} rx="4" />
				{/if}
			{/each}
			<line class="base" x1="0" y1={BASE} x2={width} y2={BASE} />
			{#each Array.from({ length: DAYS }, (_, i) => i) as ago (ago)}
				<line class="tick" x1={x(ago)} y1={BASE} x2={x(ago)} y2={BASE + 3} />
			{/each}
			{#each dots as dot (dot.change.id)}
				{@const big = listed.includes(dot.change.id)}
				{#if hot === dot.change.id}
					<circle class="ring" cx={x(dot.ago)} cy={dot.cy} r="7" />
				{/if}
				<circle
					class="dot"
					class:recent={dot.ago < 7}
					class:hot={hot === dot.change.id}
					class:more={dot.more}
					cx={x(dot.ago)}
					cy={dot.cy}
					r={big ? 3.6 : 3}
				/>
			{/each}
			{#each weeks as start (start)}
				<text class="label" x={x(start)} y={HEIGHT - 4}>{dayLabel(start)}</text>
			{/each}
			<text class="label" x={width} y={HEIGHT - 4} text-anchor="end">Today</text>
		</svg>
	{/if}
</div>

<style>
	.chart {
		height: 60px;
		margin: 12px 0 8px;
	}

	svg {
		display: block;
		overflow: visible;
	}

	/* Mixed from the text color so the weeks and days show in both themes. */
	.band {
		fill: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}

	.base,
	.tick {
		stroke: color-mix(in srgb, var(--color-foreground) 16%, transparent);
		stroke-width: 1;
	}

	.dot {
		fill: color-mix(in srgb, var(--color-primary) 38%, var(--color-background));
	}

	.dot.recent {
		fill: var(--color-primary);
	}

	.dot.more {
		fill: var(--color-background);
		stroke: currentColor;
		stroke-width: 1.25;
		color: color-mix(in srgb, var(--color-primary) 60%, var(--color-background));
	}

	.dot.hot {
		fill: var(--color-primary);
	}

	.ring {
		fill: none;
		stroke: var(--color-primary);
		stroke-width: 1.25;
		opacity: 0.45;
	}

	.label {
		font-family: var(--font-sans);
		font-size: 11px;
		fill: var(--color-foreground-subtle);
	}

</style>
