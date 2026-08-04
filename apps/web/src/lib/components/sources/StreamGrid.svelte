<!--
	The arrival grid: one row per stream, one cell per day.

	Replaces a list that showed a single "last seen" per stream. That list could
	not show the one thing worth knowing at a glance — whether streams stopped
	*together*. Nineteen rows all reading "Jul 7" is one event, a device that
	stopped, but as a list it reads as nineteen separate problems. On a day axis
	the same data draws a vertical cliff across every row that device feeds, and
	the shape says it without a word of copy.

	It also shows rhythm, which a scalar cannot. A calendar that fills only on
	weekdays is healthy; a heart rate that fills only on weekdays is a phone left
	at home.

	Rows group by source rather than by ontology because the fix for anything
	wrong here is always "go look at that device". What's *missing* is a
	different question with a different answer ("connect something"), and it
	lives in the catalog.
-->
<script lang="ts">
	import type { StreamDays } from '$lib/api/client';

	let {
		title,
		subtitle,
		streams,
		start
	}: {
		title: string;
		subtitle?: string;
		streams: StreamDays[];
		/** UTC date of column 0, from the server. Labels must come from the same
		 *  calendar the server bucketed on — deriving them from a local
		 *  `new Date()` shifts every tick and tooltip by a day for anyone west of
		 *  UTC in the evening, which undercuts the exact reading this exists for. */
		start: string | null;
	} = $props();

	/** Column index → its UTC date. */
	function dayAt(i: number): Date | null {
		if (!start) return null;
		const d = new Date(`${start}T00:00:00Z`);
		d.setUTCDate(d.getUTCDate() + i);
		return d;
	}

	const FMT: Intl.DateTimeFormatOptions = { month: 'short', day: 'numeric', timeZone: 'UTC' };

	// Four steps. Volume across streams spans orders of magnitude — a location
	// point every few seconds against one calendar event a day — so the scale is
	// per-row, relative to that row's own busiest day. The question a cell
	// answers is "was this a normal day for *this* stream", not "how does its
	// volume compare to some other stream's".
	function levels(days: number[]): number[] {
		const peak = Math.max(...days, 0);
		if (peak === 0) return days.map(() => 0);
		return days.map((n) => {
			if (n === 0) return 0;
			const r = n / peak;
			return r > 0.66 ? 4 : r > 0.33 ? 3 : 2;
		});
	}

	const rows = $derived(
		streams.map((s) => ({
			...s,
			cells: levels(s.days),
			total: s.days.reduce((a, b) => a + b, 0)
		}))
	);

	// Month ticks along the top, placed at the first cell of each month so the
	// eye can date a cliff without counting squares.
	const ticks = $derived.by(() => {
		const n = streams[0]?.days.length ?? 0;
		if (n === 0 || !start) return [];
		const out: { i: number; label: string }[] = [];
		let last = '';
		for (let i = 0; i < n; i++) {
			const d = dayAt(i);
			if (!d) break;
			const label = d.toLocaleDateString(undefined, { month: 'short', timeZone: 'UTC' });
			if (label !== last) {
				out.push({ i, label });
				last = label;
			}
		}
		return out;
	});

	function dayLabel(i: number, count: number, name: string): string {
		const d = dayAt(i);
		const when = d ? d.toLocaleDateString(undefined, FMT) : `day ${i + 1}`;
		return count === 0 ? `${name} — nothing on ${when}` : `${name} — ${count} on ${when}`;
	}
</script>

<section class="grid-block">
	<header>
		<h2>{title}</h2>
		{#if subtitle}<span class="sub">{subtitle}</span>{/if}
	</header>

	<div class="scale" style="--n: {streams[0]?.days.length ?? 0}">
		{#each ticks as t (t.i)}
			<span class="tick" style="grid-column: {t.i + 1}">{t.label}</span>
		{/each}
	</div>

	{#each rows as r (r.name)}
		<div class="row">
			<span class="name" class:empty={r.total === 0}>{r.display_name}</span>
			<div class="cells" style="--n: {r.cells.length}">
				{#each r.cells as level, i (i)}
					<span
						class="cell l{level}"
						title={dayLabel(i, r.days[i], r.display_name)}
					></span>
				{/each}
			</div>
		</div>
	{/each}
</section>

<style>
	.grid-block {
		margin-bottom: 1.75rem;
	}

	header {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		padding-bottom: 0.375rem;
		margin-bottom: 0.5rem;
		border-bottom: 1px solid color-mix(in srgb, var(--color-foreground) 8%, transparent);
	}
	h2 {
		margin: 0;
		font-size: 1rem;
		font-weight: 600;
		letter-spacing: -0.008em;
		color: var(--color-foreground, #111827);
	}
	.sub {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted, #6b7280);
	}

	/* The month ruler shares the row grid so a tick sits over its own column. */
	.scale {
		display: grid;
		grid-template-columns: repeat(var(--n), 1fr);
		margin-left: 9.5rem;
		margin-bottom: 0.25rem;
		height: 0.875rem;
	}
	.tick {
		font-size: 0.6875rem;
		line-height: 1;
		white-space: nowrap;
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.0625rem 0;
	}
	.name {
		flex-shrink: 0;
		width: 8.75rem;
		font-size: 0.8125rem;
		color: var(--color-foreground, #111827);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	/* A stream that has never delivered keeps its row — an empty lane is the
	   clearest possible statement of "nothing here", and it stays aligned with
	   its neighbours so the eye can compare. */
	.name.empty {
		color: var(--color-foreground-subtle, #9ca3af);
	}

	.cells {
		flex: 1;
		display: grid;
		grid-template-columns: repeat(var(--n), 1fr);
		gap: 1.5px;
	}
	.cell {
		height: 11px;
		border-radius: 1.5px;
		background: color-mix(in srgb, var(--color-foreground) 5%, transparent);
	}
	/* One hue, four weights. Volume is a quantity, not a category, so it wants
	   intensity rather than different colours. */
	.cell.l2 {
		background: color-mix(in srgb, var(--color-primary) 28%, transparent);
	}
	.cell.l3 {
		background: color-mix(in srgb, var(--color-primary) 58%, transparent);
	}
	.cell.l4 {
		background: color-mix(in srgb, var(--color-primary) 88%, transparent);
	}

	@media (max-width: 720px) {
		.name {
			width: 6.5rem;
		}
		.scale {
			margin-left: 7.25rem;
		}
	}
</style>
