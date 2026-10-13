<!--
	ReviewCard.svelte - "Help Virtues out"'s card, the prototype's
	(dayback/src/main.js:2113-2201), opened in the day's list in place of the
	list: one place's question, from "Answer" on its stay, while the map goes
	to the place with the named places it offers; or the names Virtues chose
	without asking, where a wrong one is marked and corrected. "‹ Back to the
	Day" returns to the list, and so does an answer.
-->
<script lang="ts">
	import Pill from '$lib/components/timeline/Pill.svelte';
	import ReviewAsk from '$lib/components/timeline/ReviewAsk.svelte';
	import { askFor, type ConfirmedPlace, type PlaceCandidate, type PlaceQuestion } from '$lib/timeline/review';
	import type { ReviewState } from '$lib/timeline/reviewState.svelte';

	let {
		review,
		q = null,
		inline = false,
		onback,
		onfocus,
	}: {
		review: ReviewState;
		/** Open in the day's list under its stay: no header, a Close at the foot. */
		inline?: boolean;
		/** The place asked about; `null` lists the names Virtues chose. */
		q?: PlaceQuestion | null;
		/** Back to the day's list. */
		onback: () => void;
		/** The place being asked about and the named places it offers, for
		 *  the map; `null` lets go. */
		onfocus: (at: { lat: number; lon: number; reachM: number; shown: PlaceCandidate[] } | null) => void;
	} = $props();

	/** A named place marked wrong, whose right name is being typed. */
	let flagged = $state<string | null>(null);

	$effect(() => {
		const at = q && !review.pointing ? q : null;
		const ask = at ? askFor(at) : null;
		onfocus(at && ask ? { lat: at.latitude, lon: at.longitude, reachM: ask.reachM, shown: ask.shown } : null);
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

<div class="card" class:inline>
	{#if !inline}
		<div class="top">
			<Pill quiet onclick={onback}>‹ Back to the Day</Pill>
		</div>
	{/if}

	{#if q}
		<div class="pad">
			<ReviewAsk {q} {review} onclose={inline ? onback : undefined} />
		</div>
	{:else}
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
		{#if review.undo}
			<div class="undo" role="status">
				<span>{review.undo.said}</span>
				<Pill onclick={() => void review.undoLast()}>Undo</Pill>
			</div>
		{/if}
	{/if}
</div>

<style>
	.card {
		font-family: var(--font-sans);
		color: var(--color-foreground);
	}
	/* Under its stay, on the stay's own tint: the question's text in line
	   with the stay's name and set a step below it. */
	.card.inline :global(.q) {
		font-size: 17px;
	}
	.top {
		display: flex;
		align-items: center;
		min-height: 44px;
		padding: 8px 12px 8px 16px;
		border-bottom: 1px solid var(--color-border);
		font-size: 13px;
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
	.undo {
		display: flex;
		align-items: center;
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
