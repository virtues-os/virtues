<!--
	ReviewCard.svelte - "Help Virtues out"'s card, the prototype's
	(dayback/src/main.js:2113-2201), opened in the day's list in place of the
	list: one place's question, from "Answer" on its stay, while the map goes
	to the place with the named places it offers; or the names Virtues chose
	without asking or the person set, where picking one opens the last day
	the person was there with its question asked again, to change it. "‹ Back
	to the Day" returns to the list, and so does an answer.
-->
<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Pill from '$lib/components/timeline/Pill.svelte';
	import ReviewAsk from '$lib/components/timeline/ReviewAsk.svelte';
	import { askFor, nowLine, type ConfirmedPlace, type PlaceCandidate, type PlaceQuestion } from '$lib/timeline/review';
	import type { ReviewState } from '$lib/timeline/reviewState.svelte';

	let {
		review,
		q = null,
		inline = false,
		onback,
		onfocus,
		onopen,
		change,
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
		/** A name picked in the list: open the last day the person was there. */
		onopen?: (c: ConfirmedPlace) => void;
		/** `q` is a named place asked again from "Named Places": what it is now. */
		change?: ConfirmedPlace;
	} = $props();

	// The map goes to the place asked about and labels the places it
	// offers; a name being changed that the person set is the open question.
	$effect(() => {
		const at = q && !review.pointing ? q : null;
		const ask = at ? askFor(at, change?.source === 'named') : null;
		onfocus(at && ask ? { lat: at.latitude, lon: at.longitude, reachM: ask.reachM, shown: ask.shown } : null);
		return () => onfocus(null);
	});

	const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? '' : 's'}`;
	/** "45 min", "3.5 hours", "422 hours": a tenth of an hour only under 10. */
	const duration = (h: number) => {
		if (h < 1) return `${Math.round(h * 60)} min`;
		const v = h < 10 ? h : Math.round(h);
		return `${v} ${v === 1 ? 'hour' : 'hours'}`;
	};
	/** The names in groups by where they came from: Home and Work, then the
	 *  map's (guesses, the ones worth checking), then the person's own. */
	const GROUPS: { label: string; sources: ConfirmedPlace['source'][] }[] = [
		{ label: 'Home and work', sources: ['home', 'work'] },
		{ label: 'Matched by Virtues', sources: ['map'] },
		{ label: 'Named by you', sources: ['named'] },
	];
</script>

<div class="card" class:inline>
	{#if !inline}
		<div class="top">
			<Pill quiet onclick={onback}>‹ Back to the Day</Pill>
		</div>
	{/if}

	{#if q}
		<div class="pad">
			<ReviewAsk {q} {review} onclose={inline ? onback : undefined} now={change ? nowLine(change) : undefined} current={change?.source === 'named' ? change.title : undefined} />
		</div>
	{:else}
		<div class="pad">
			<h3 class="title">Named Places</h3>
			{#each GROUPS as g (g.label)}
				{@const rows = review.confirmed.filter((c) => g.sources.includes(c.source))}
				{#if rows.length}
					<p class="section-hd">{g.label}</p>
					<!-- The question card's grouped list; each row leads to its
					     day, so it carries Settings' chevron. -->
					<ul class="list">
						{#each rows as c (c.id)}
							<li>
								<button type="button" disabled={!onopen || !c.last_day} onclick={() => onopen?.(c)}>
									<span class="row-main">
										<span class="row-name">{c.title}</span>
										<span class="row-meta">{plural(c.stop_count, 'visit')} · {duration(c.hours)}</span>
									</span>
									<Icon icon="ri:arrow-right-s-line" width="16" />
								</button>
							</li>
						{/each}
					</ul>
				{/if}
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
	/* A list's header, as in the question card: quiet, in line with the
	   rows' text. */
	.section-hd {
		margin: 16px 0 0;
		padding-left: 13px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	/* The question card's inset grouped list: one white rounded box of rows,
	   hairlines between them. */
	.list {
		margin: 8px 0 0;
		padding: 0;
		list-style: none;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		background: var(--color-surface);
		overflow: hidden;
	}
	.list li + li {
		border-top: 1px solid var(--color-border);
	}
	.list button {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 8px 12px;
		border: 0;
		background: none;
		color: var(--color-foreground);
		font-family: var(--font-sans);
		text-align: left;
		cursor: pointer;
	}
	.list button:hover:not(:disabled) {
		background: var(--hover-bg);
	}
	.list button:disabled {
		cursor: default;
	}
	.list button :global(svg) {
		flex: none;
		color: var(--color-foreground-muted);
	}
	.row-main {
		flex: 1;
		min-width: 0;
	}
	.row-name {
		display: block;
		font-size: 15px;
	}
	.row-meta {
		display: block;
		margin-top: 4px;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
</style>
