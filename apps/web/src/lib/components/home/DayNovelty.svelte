<!--
	DayNovelty.svelte — order and chaos, measured against yourself.

	The dayline's own novelty scores cannot answer for today: they are written
	by the end-of-day pass at ~4am the next morning, and the server refuses to
	fuse a day that is not over. So this computes a live novelty the same way a
	person would — by how far today's *rhythm* has drifted from their own.

	The measure is deliberately about WHEN, not HOW MUCH. Each day is the shape
	of its activity across the hours, normalised so a busy day and a quiet day
	with the same rhythm score the same. Distance from the median shape is total
	variation, which lands in 0…1 and needs no calibration.

	Two honesty rules it will not break:

	  · only complete, elapsed hours are compared, for today and for every past
	    day alike. Comparing a half-finished day against whole ones would call
	    every morning chaotic.
	  · with too few days behind it, it renders nothing rather than scoring
	    noise. A box in its first week has no "usual" to be unlike, and this is
	    the page's opening line — it only speaks when it has a claim to make.

	Source is the day-clock raster, which counts activation — app sessions,
	visits, outbound messages, transcription, browsing, listening, workouts. It
	is the record of being awake and doing something, so sleep reads as the
	quiet band it is rather than as an absence of data.
-->
<script lang="ts">
	import { getClock } from "$lib/wiki/api";

	interface Props {
		dayStartMs: number;
		nowMs: number;
		tz: string;
	}
	let { dayStartMs, nowMs, tz }: Props = $props();

	/** Days of history to judge against: trailing twelve weeks, the same
	 *  window the rest of the record judges "usual" against. */
	const WINDOW = 84;
	/** Below this many usable past days there is no "usual" worth naming. */
	const MIN_DAYS = 7;
	/** Before this many hours have elapsed, a day has not shown its shape. */
	const MIN_HOURS = 4;
	/** One or two rows is an incident, not a rhythm. */
	const MIN_EVENTS = 3;
	/** Below this much spread across the window, the days are indistinguishable
	 *  and a strip would draw differences that are not there. */
	const MIN_SPREAD = 0.02;

	const hoursElapsed = $derived(Math.floor((nowMs - dayStartMs) / 3_600_000));

	let cells = $state<number[] | null>(null);
	let columns = $state(0);
	let failed = $state(false);

	// Refetch on the hour, not on the deck's 30s beat — the raster only changes
	// resolution once an hour, and it is the most expensive read on the page.
	$effect(() => {
		const h = hoursElapsed;
		if (h < MIN_HOURS) return;
		let dropped = false;
		const from = new Date(dayStartMs - WINDOW * 86_400_000).toISOString();
		const to = new Date(dayStartMs + 86_400_000).toISOString();
		getClock(from, to, WINDOW + 1, tz)
			.then((c) => {
				if (dropped) return;
				if (!c) {
					failed = true;
					return;
				}
				cells = c.cells;
				columns = c.columns;
			})
			.catch(() => {
				if (!dropped) failed = true;
			});
		return () => {
			dropped = true;
		};
	});

	function median(xs: number[]): number {
		if (!xs.length) return 0;
		const s = [...xs].sort((a, b) => a - b);
		const m = s.length >> 1;
		return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
	}

	const analysis = $derived.by(() => {
		const c = cells;
		const H = Math.min(24, hoursElapsed);
		if (!c || columns < 2 || H < MIN_HOURS) return null;

		// Each column is a day; take only the hours today has also lived.
		const shapes: Array<{ col: number; p: number[] } | null> = [];
		for (let col = 0; col < columns; col++) {
			const row: number[] = [];
			let total = 0;
			for (let h = 0; h < H; h++) {
				const v = c[col * 24 + h] ?? 0;
				row.push(v);
				total += v;
			}
			// Too little in these hours to have a shape worth comparing.
			shapes.push(total >= MIN_EVENTS ? { col, p: row.map((v) => v / total) } : null);
		}

		const todayShape = shapes[columns - 1];
		const past = shapes.slice(0, columns - 1).filter((s): s is { col: number; p: number[] } => s !== null);
		if (!todayShape || past.length < MIN_DAYS) {
			return { ready: false as const, days: past.length, hasToday: !!todayShape };
		}

		// The usual shape: the median hour by hour, renormalised.
		const base: number[] = [];
		for (let h = 0; h < H; h++) base.push(median(past.map((s) => s.p[h])));
		const bsum = base.reduce((a, b) => a + b, 0) || 1;
		const b = base.map((v) => v / bsum);

		/** Total variation from the usual shape: 0 is identical, 1 is disjoint. */
		const dist = (p: number[]) => 0.5 * p.reduce((acc, v, h) => acc + Math.abs(v - b[h]), 0);

		const pastScores = past.map((s) => dist(s.p));
		const todayScore = dist(todayShape.p);
		const beaten = pastScores.filter((s) => s < todayScore).length;

		const lo = Math.min(todayScore, ...pastScores);
		const hi = Math.max(todayScore, ...pastScores);
		// Days that all look alike get the sentence but not the chart: placing
		// them along a stretched axis would invent distinctions.
		if (hi - lo < MIN_SPREAD) {
			return { ready: true as const, days: past.length, beaten, ticks: null, today: 0 };
		}
		const span = hi - lo;
		const at = (s: number) => (s - lo) / span;

		return {
			ready: true as const,
			days: past.length,
			ticks: pastScores.map(at),
			today: at(todayScore),
			beaten,
		};
	});

	// The strip's own geometry, in real pixels.
	let trackW = $state(320);
	const PAD = 5;
	function atX(t: number): number {
		return PAD + t * Math.max(1, trackW - 2 * PAD);
	}

	const caption = $derived.by(() => {
		const a = analysis;
		if (!a || !a.ready) return null;
		if (!a.ticks) return "Your last twelve weeks are near-identical in rhythm — today is no exception.";
		const share = a.beaten / a.days;
		const tail = `unlike ${a.beaten} of your last ${a.days} days`;
		if (share >= 0.8) return `Today has gone off your usual rhythm — ${tail}.`;
		if (share <= 0.25) return "Today is keeping to your usual rhythm.";
		return `Today is running about as usual — ${tail}.`;
	});
</script>

{#if hoursElapsed >= MIN_HOURS && !failed && analysis?.ready && caption}
	<!-- The sentence leads and the strip is its evidence: this block opens the
	     page, in the counted voice the remark used to hold. -->
	<figure class="nov">
		<figcaption>{caption}</figcaption>
		{#if analysis.ticks}
			<div class="strip">
				<span class="end mono">ordinary</span>
				<!-- Measured, not stretched: a viewBox scaled to the column width
				     would draw the day's dot as an ellipse. -->
				<div class="track" bind:clientWidth={trackW}>
					<svg width={trackW} height="14" role="img" aria-label={caption ?? ""}>
						<line class="rail" x1={PAD} y1="7" x2={trackW - PAD} y2="7" />
						{#each analysis.ticks as t, i (i)}
							<line class="tick" x1={atX(t)} y1="2.5" x2={atX(t)} y2="11.5" />
						{/each}
						<circle class="me" cx={atX(analysis.today)} cy="7" r="3.5" />
					</svg>
				</div>
				<span class="end mono">unusual</span>
			</div>
		{/if}
	</figure>
{/if}

<style>
	.nov { margin: 0 0 26px; padding-top: 14px; border-top: 1px solid var(--color-border); max-width: 58ch; }
	.mono { font-family: var(--font-mono); }

	.strip { display: flex; align-items: center; gap: 12px; margin-top: 12px; }
	.track { flex: 1; min-width: 0; }
	.track svg { display: block; overflow: visible; }
	.end { font-size: 9.5px; letter-spacing: 0.04em; color: var(--color-foreground-subtle); flex: none; }

	.rail { stroke: var(--color-foreground); stroke-opacity: 0.14; stroke-width: 1; }
	.tick { stroke: var(--color-foreground); stroke-opacity: 0.26; stroke-width: 1; }
	.me { fill: var(--color-primary); }

	figcaption {
		font-family: var(--font-sans); font-size: 15px; line-height: 1.5;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground); margin: 0;
	}
</style>
