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

	Sections are life-domains — Health, Location, Communication — not the
	companies the rows came from. The page is read as "what does the box hold,
	and is it still arriving", and that question is asked in the vocabulary of a
	life rather than of vendors: a heart rate is a heart rate whether an iPhone
	or a watch wrote it, and Google is not a category of anything.

	Grouping by source instead would put every stream a device feeds side by
	side, which is what made a device's cliff obvious. Each row therefore names
	its own provider — same information, attached to the row it describes
	instead of to the heading, so "go look at that device" survives the regroup.

	What's *missing* is a different question with a different answer ("connect
	something"), and it lives in the catalog.
-->
<script lang="ts" module>
	import type { StreamDays } from '$lib/api/client';

	/** A stream's arrivals, plus the human name of whatever wrote them. */
	export type GridRow = StreamDays & { providerLabel?: string };
</script>

<script lang="ts">
	let {
		title,
		subtitle,
		streams,
		start
	}: {
		title: string;
		subtitle?: string;
		streams: GridRow[];
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

	<!-- Keyed on stream AND provider: one stream fed by two sources is two rows
	     (bookmarks from a Mac and from GitHub), and within a domain section both
	     are present, so the stream name alone is not unique. -->
	{#each rows as r (`${r.name}:${r.provider}`)}
		<div class="row">
			<span class="label" class:empty={r.total === 0}>
				<span class="name">{r.display_name}</span>
				{#if r.providerLabel}<span class="provider">{r.providerLabel}</span>{/if}
			</span>
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
		/* label width + the row's gap */
		margin-left: 13.25rem;
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
	/* One fixed-width label holding two facts: what the stream is, and who
	   wrote it. Fixed so every lane starts at the same x and a cliff reads
	   straight down the column. */
	.label {
		flex-shrink: 0;
		display: flex;
		align-items: baseline;
		gap: 0.4375rem;
		width: 12.5rem;
		font-size: 0.8125rem;
		overflow: hidden;
		white-space: nowrap;
	}
	.name {
		flex: 0 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		color: var(--color-foreground, #111827);
	}
	/* Secondary: you read down the stream names and only ask "which device"
	   when a lane looks wrong. First to truncate, for the same reason. */
	.provider {
		flex: 1 1 auto;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle, #9ca3af);
	}
	/* A stream that has never delivered keeps its row — an empty lane is the
	   clearest possible statement of "nothing here", and it stays aligned with
	   its neighbours so the eye can compare. */
	.label.empty .name {
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
		.label {
			width: 7.5rem;
		}
		/* No room for both; the stream name is the one you scan. */
		.provider {
			display: none;
		}
		.scale {
			margin-left: 8.25rem;
		}
	}
</style>
