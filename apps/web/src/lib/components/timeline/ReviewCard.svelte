<!--
	ReviewCard.svelte - "Help Virtues out"'s Review: the places on other days,
	asked one at a time inside the day's list, as Photos asks "Is this Sam?"
	(dayback/src/main.js:2113-2201). Home and Work first, then the most time.
	The map goes to each place while it's asked about. When nothing is left,
	"All caught up", and the names Virtues chose without asking, where a wrong
	one is marked and corrected.
-->
<script lang="ts">
	import IconButton from '$lib/components/IconButton.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import ReviewAsk from '$lib/components/timeline/ReviewAsk.svelte';
	import { askFor, type ConfirmedPlace, type PlaceCandidate, type PlaceQuestion } from '$lib/timeline/review';
	import type { ReviewState } from '$lib/timeline/reviewState.svelte';

	let {
		review,
		questions,
		onback,
		onday,
		onfocus,
	}: {
		review: ReviewState;
		/** The places to ask about, in order. */
		questions: PlaceQuestion[];
		/** Back to the day's list. */
		onback: () => void;
		/** "See That Day": open that day. */
		onday: (slug: string) => void;
		/** The place being asked about and the named places it offers, for
		 *  the map; `null` lets go. */
		onfocus: (at: { lat: number; lon: number; reachM: number; shown: PlaceCandidate[] } | null) => void;
	} = $props();

	let idx = $state(0);
	let mode = $state<'questions' | 'names'>('questions');
	/** A named place marked wrong, whose right name is being typed. */
	let flagged = $state<string | null>(null);
	const at = $derived(Math.max(0, Math.min(idx, questions.length - 1)));
	const current = $derived(questions[at] ?? null);

	$effect(() => {
		const q = mode === 'questions' && !review.pointing ? current : null;
		const ask = q ? askFor(q) : null;
		onfocus(q && ask ? { lat: q.latitude, lon: q.longitude, reachM: ask.reachM, shown: ask.shown } : null);
		return () => onfocus(null);
	});

	/** A named place's right name, typed after marking it wrong (main.js:
	 *  2141): a name saves it; left blank, the map's match is set aside, and
	 *  the person's own answer (a name, Home, Work) is forgotten, so the place
	 *  is asked about again. */
	function correct(c: ConfirmedPlace, value: string) {
		const v = value.trim();
		flagged = null;
		if (v) void review.answer(c.id, { kind: 'name', name: v });
		else if (c.source === 'map') void review.answer(c.id, { kind: 'not_this', name: c.title });
		else void review.answer(c.id, { kind: 'reopen' });
	}

	const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? '' : 's'}`;
	const duration = (h: number) => (h < 1 ? `${Math.round(h * 60)} min` : `${h} ${h === 1 ? 'hour' : 'hours'}`);
	/** Why a place is named (main.js:2113). */
	const SOURCE = { home: 'Your home', work: 'Where you work', named: 'Named by you', map: 'From the map' };
</script>

<div class="card">
	<div class="top">
		<TextAction quiet onclick={() => (mode === 'names' ? (mode = 'questions') : onback())}>
			{mode === 'names' ? '‹ Back' : '‹ Back to the Day'}
		</TextAction>
		{#if mode === 'questions' && current}
			<span class="nav">
				<IconButton icon="ri:arrow-left-s-line" label="Previous place" size="xs" disabled={at <= 0} onclick={() => (idx = at - 1)} />
				<span class="counter">{at + 1} of {questions.length}</span>
				<IconButton icon="ri:arrow-right-s-line" label="Next place" size="xs" disabled={at >= questions.length - 1} onclick={() => (idx = at + 1)} />
			</span>
		{/if}
	</div>

	{#if mode === 'names'}
		<div class="pad">
			<h3 class="title">Auto-selected names</h3>
			<p class="sub">Virtues chose these without asking, or you set them. Mark one with ✕ if it's wrong.</p>
			{#each review.confirmed as c (c.id)}
				<div class="row" class:flagged={flagged === c.id}>
					<div class="row-main">
						<div class="row-name">{c.title}</div>
						<div class="row-meta">{SOURCE[c.source]} · {plural(c.stop_count, 'visit')} · {duration(c.hours)}</div>
						{#if flagged === c.id}
							<input
								class="field"
								placeholder="What was it? - or leave blank"
								aria-label="What was it?"
								disabled={review.saving}
								onkeydown={(e) => e.key === 'Enter' && correct(c, (e.currentTarget as HTMLInputElement).value)}
							/>
						{/if}
					</div>
					<button class="wrong" type="button" aria-label="Mark as wrong" aria-pressed={flagged === c.id} disabled={review.saving} onclick={() => (flagged = flagged === c.id ? null : c.id)}>✕</button>
				</div>
			{/each}
		</div>
	{:else if current}
		<div class="pad">
			<ReviewAsk q={current} {review} />
		</div>
		{#if current.best_day}
			{@const day = current.best_day}
			<div class="foot"><TextAction onclick={() => onday(day)}>See That Day</TextAction></div>
		{/if}
	{:else}
		<div class="pad">
			<h3 class="title">All caught up</h3>
			<p class="sub">Nothing left to ask about.</p>
		</div>
		<div class="foot"><TextAction onclick={() => (mode = 'names')}>View Auto-Selected Names</TextAction></div>
	{/if}

	{#if review.undo}
		<div class="undo" role="status">
			<span>{review.undo.said}</span>
			<TextAction onclick={() => void review.undoLast()}>Undo</TextAction>
		</div>
	{/if}
</div>

<style>
	.card {
		font-family: var(--font-sans);
		color: var(--color-foreground);
	}
	.top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		min-height: 44px;
		padding: 8px 12px 8px 16px;
		border-bottom: 1px solid var(--color-border);
		font-size: 13px;
	}
	.nav {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.counter {
		font-size: 12px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.pad {
		padding: 16px;
	}
	.title {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 20px;
		font-weight: 400;
	}
	.sub {
		margin: 4px 0 0;
		font-size: 13px;
		line-height: 1.35;
		color: var(--color-foreground-muted);
	}
	.foot,
	.undo {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 16px 12px;
		border-top: 1px solid var(--color-border);
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 0;
		border-top: 1px solid var(--color-border);
	}
	.row:first-of-type {
		margin-top: 12px;
	}
	.row-main {
		flex: 1;
		min-width: 0;
	}
	.row-name {
		font-size: 15px;
	}
	.row.flagged .row-name {
		color: var(--color-foreground-muted);
		text-decoration: line-through;
	}
	.row-meta {
		margin-top: 4px;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.field {
		width: 100%;
		margin-top: 8px;
		padding: 8px 12px;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 15px;
	}
	.wrong {
		width: 28px;
		height: 28px;
		border: 0;
		border-radius: 50%;
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground-muted);
		font-size: 12px;
		cursor: pointer;
	}
	.wrong[aria-pressed='true'],
	.wrong:hover:not(:disabled) {
		color: var(--color-foreground);
		background: color-mix(in srgb, var(--color-foreground) 12%, transparent);
	}
</style>
