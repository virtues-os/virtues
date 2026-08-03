<script lang="ts">
	/**
	 * The feed — what is actually inside the window.
	 *
	 * **The panel is the payload.** Selecting a stretch of time and being handed
	 * a column of sums is the same answer chat already gives badly. The reason to
	 * draw a timeline is that a range on it can give you back the rows.
	 *
	 * **It is one half of a linked view.** Hovering a row marks the chart;
	 * hovering the chart scrolls the matching row into sight here. That loop is
	 * most of what makes a chart feel alive — it stops being a picture and starts
	 * pointing at things you can read. Clicking a row takes the window there.
	 *
	 * **It never blanks.** A window that moves keeps the previous rows on screen,
	 * dimmed, until the new ones land. A spinner in a panel this size costs more
	 * than it explains, and a blank one loses your place.
	 *
	 * **Two registers, and the difference between them is the point.** RAW is
	 * every collector row, back to 2017. PROCESSED is what Virtues has made of
	 * it — and mostly it has not, yet. So an empty processed window reports how
	 * far interpretation actually reaches instead of just looking broken.
	 */
	import {
		getFeed,
		getProcessed,
		type LifelineRecord,
		type Interpreted
	} from '$lib/wiki/api';

	interface Props {
		from: number;
		to: number;
		lane?: string | null;
		mode: 'raw' | 'processed';
		/** Record the chart is pointing at, scrolled into view and marked. */
		highlight?: string | null;
		coverageDays?: number;
		coverageStart?: number | null;
		coverageEnd?: number | null;
		onhover?: (id: string | null) => void;
		ongoto?: (t: number) => void;
	}
	let {
		from,
		to,
		lane = null,
		mode,
		highlight = null,
		coverageDays = 0,
		coverageStart = null,
		coverageEnd = null,
		onhover,
		ongoto
	}: Props = $props();

	const PAGE = 40;

	let records = $state<LifelineRecord[]>([]);
	let events = $state<Interpreted[]>([]);
	let hasMore = $state(false);
	let stale = $state(false);
	let firstLoad = $state(true);
	let paging = $state(false);
	let rows: Record<string, HTMLLIElement> = {};

	let seq = 0;
	let timer: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		const a = new Date(from).toISOString();
		const b = new Date(to).toISOString();
		const m = mode;
		const l = lane;
		clearTimeout(timer);
		stale = true;
		timer = setTimeout(async () => {
			const mine = ++seq;
			if (m === 'processed') {
				const p = await getProcessed(a, b);
				if (mine !== seq) return;
				events = p?.items ?? [];
				hasMore = false;
			} else {
				const f = await getFeed(a, b, { lanes: l ? [l] : undefined, limit: PAGE });
				if (mine !== seq) return;
				records = f?.records ?? [];
				hasMore = f?.has_more ?? false;
			}
			stale = false;
			firstLoad = false;
		}, 140);
	});

	// Follow the chart's pointer. `block: 'nearest'` so a row already on screen
	// does not yank the list under the reader for no reason.
	$effect(() => {
		if (!highlight) return;
		rows[highlight]?.scrollIntoView({ block: 'nearest' });
	});

	async function more() {
		if (paging || !hasMore) return;
		paging = true;
		const mine = seq;
		const f = await getFeed(new Date(from).toISOString(), new Date(to).toISOString(), {
			lanes: lane ? [lane] : undefined,
			limit: PAGE,
			offset: records.length
		});
		// A page that landed after the window moved belongs to a different
		// question; dropping it is the only correct thing to do with it.
		if (mine === seq && f) {
			records = [...records, ...f.records];
			hasMore = f.has_more;
		}
		paging = false;
	}

	/** The part of `message:imessage` worth showing beside every row. */
	const chip = (kind: string, ontology: string) =>
		(kind || ontology).split(':')[0].replace(/_/g, ' ');

	function when(iso: string): string {
		const d = new Date(iso);
		const days = (to - from) / 86_400_000;
		if (days < 2) return d.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
		if (days < 330) return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
		return d.toLocaleDateString('en-US', { month: 'short', year: '2-digit' });
	}

	function duration(e: Interpreted): string {
		if (!e.end) return '';
		const m = (new Date(e.end).getTime() - new Date(e.start).getTime()) / 60000;
		return m < 90 ? `${Math.round(m)}m` : `${(m / 60).toFixed(1)}h`;
	}

	const coverageLine = $derived.by(() => {
		if (coverageStart === null || coverageEnd === null) return null;
		const f = (t: number) =>
			new Date(t).toLocaleDateString('en-US', {
				month: 'short',
				day: 'numeric',
				year: 'numeric'
			});
		return `${f(coverageStart)} – ${f(coverageEnd)}`;
	});
</script>

<div class="feed" class:stale>
	{#if mode === 'processed'}
		{#if events.length}
			<ul>
				{#each events as e (e.id)}
					<li
						class="row"
						class:sleep={e.tag === 'sleep'}
						onmouseenter={() => onhover?.(e.id)}
						onmouseleave={() => onhover?.(null)}
					>
						<div class="head">
							<span class="chip">{e.tag ?? 'event'}</span>
							<button
								type="button"
								class="at"
								onclick={() => ongoto?.(new Date(e.start).getTime())}
							>
								{when(e.start)}{#if duration(e)} · {duration(e)}{/if}
							</button>
						</div>
						<p class="label">{e.label ?? 'Unlabeled'}</p>
						{#if e.summary}<p class="prose">{e.summary}</p>{/if}
					</li>
				{/each}
			</ul>
		{:else if !firstLoad}
			<p class="quiet">
				Nothing interpreted in this window.
				{#if coverageLine}
					Virtues has processed <strong>{coverageDays} days</strong>, covering
					{coverageLine}. Everything outside that exists as raw records only.
					{#if coverageStart !== null}
						<button
							type="button"
							class="link"
							onclick={() => ongoto?.((coverageStart + coverageEnd!) / 2)}
						>
							Go there →
						</button>
					{/if}
				{:else}
					No days have been processed yet.
				{/if}
			</p>
		{/if}
	{:else if records.length}
		<ul>
			{#each records as r (r.ontology + r.id)}
				<li
					class="row"
					class:on={highlight === r.id}
					bind:this={rows[r.id]}
					onmouseenter={() => onhover?.(r.id)}
					onmouseleave={() => onhover?.(null)}
				>
					<div class="head">
						<span class="chip">{chip(r.kind, r.ontology)}</span>
						<button type="button" class="at" onclick={() => ongoto?.(new Date(r.at).getTime())}>
							{when(r.at)}
						</button>
					</div>
					{#if r.label}<p class="label">{r.label}</p>{/if}
					{#if r.preview}<p class="prose">{r.preview}</p>{/if}
				</li>
			{/each}
		</ul>
		{#if hasMore}
			<button type="button" class="more" disabled={paging} onclick={more}>
				{paging ? 'Loading…' : 'More'}
			</button>
		{/if}
	{:else if !firstLoad}
		<p class="quiet">No records in this window.</p>
	{/if}
</div>

<style>
	@reference "../../../app.css";

	/* Kept on screen while the next window loads, so the panel never blanks and
	   never shifts. Dimming is the whole loading state. */
	.feed.stale {
		opacity: 0.45;
		transition: opacity 90ms ease;
	}

	.feed ul {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.row {
		padding: 0.4375rem 0.375rem;
		margin: 0 -0.375rem;
		border-radius: 5px;
		border-top: 1px solid var(--color-border);
	}

	.row:first-child {
		border-top: none;
	}

	.row:hover {
		background: var(--color-highlight);
	}

	/* The chart is pointing at this one. */
	.row.on {
		background: var(--color-highlight);
		box-shadow: inset 2px 0 0 var(--color-primary);
	}

	.row.sleep {
		opacity: 0.7;
	}

	.head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.5rem;
	}

	/* A hairline outline, not a fill. A row's type is the least important thing
	   about it, and a colored pill would make it the loudest. */
	.chip {
		padding: 0.0625rem 0.25rem;
		border: 1px solid var(--color-border);
		border-radius: 3px;
		font-size: 0.5625rem;
		letter-spacing: 0.02em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
		white-space: nowrap;
	}

	.at {
		padding: 0;
		background: none;
		border: none;
		font: inherit;
		font-size: 0.625rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		cursor: pointer;
	}

	.at:hover {
		color: var(--color-primary);
	}

	.label {
		margin: 0.1875rem 0 0;
		font-size: 0.75rem;
		line-height: 1.35;
	}

	.prose {
		margin: 0.0625rem 0 0;
		font-size: 0.6875rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle);
		overflow-wrap: anywhere;
	}

	.more {
		width: 100%;
		margin-top: 0.5rem;
		padding: 0.25rem;
		background: none;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		font: inherit;
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}

	.more:hover:not(:disabled) {
		color: var(--color-foreground);
	}

	.quiet {
		margin: 0;
		font-size: 0.6875rem;
		line-height: 1.5;
		color: var(--color-foreground-subtle);
	}

	.link {
		padding: 0;
		background: none;
		border: none;
		font: inherit;
		color: var(--color-primary);
		cursor: pointer;
	}
</style>
