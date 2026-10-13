<!--
	ReviewAsk.svelte - one "Help Virtues out" question and its answers
	(dayback/src/main.js:2156-2182), in the card that opens from its stay.
	A guess the evidence supports ("Were you at X?" Yes / No), several close
	places to check, or "Is this home?" / "Is this where you work?", where
	"No" hands over to pointing on the map. Anything else is an open
	question, in Apple's grouped lists: the named places around with a row
	to type a name, then the kinds of place for when the name isn't known
	(a hotel or someone's home ask which one). From "Named Places", a named
	place's question is asked again, under what it is now, to change it.
-->
<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import Pill from '$lib/components/timeline/Pill.svelte';
	import { listPeople } from '$lib/wiki/api';
	import { askFor, homeOf, pickAnswer, weekly, type Answer, type Kind, type PlaceQuestion } from '$lib/timeline/review';
	import type { ReviewState } from '$lib/timeline/reviewState.svelte';

	let {
		q,
		review,
		onclose,
		now,
		current,
	}: {
		q: PlaceQuestion;
		review: ReviewState;
		/** Close the question (its card in the list); no Close without it. */
		onclose?: () => void;
		/** A named place asked again: what it is now ("Named by you"). */
		now?: string;
		/** The name the person set, being changed: the first row, checked,
		 *  and the question always the open one. */
		current?: string;
	} = $props();

	/** A name to offer, with the kind of place it is when the map knows. */
	type Offer = { name: string; category: string | null };

	const ask = $derived(askFor(q, !!current));
	let picked = $state<string[]>([]);
	/** "None of these" on the checklist: the name field instead. */
	let noneOfThese = $state(false);
	/** A kind that asks a follow-up (a hotel: which; a home: whose). */
	let kind = $state<Kind | null>(null);
	let text = $state('');
	let input = $state<HTMLInputElement | null>(null);
	let people = $state<string[] | null>(null);
	// Under the question: what a named place is now, and for Home and Work
	// the week there. When the person was there is the day on show, so it
	// isn't repeated (the owner's call, 2026-10-13).
	const sub = $derived([now, ask.role ? weekly(q) : null].filter(Boolean).join(' · '));
	const open = $derived(!ask.chips.length && (!ask.picks.length || noneOfThese));
	/** The hotels "Which hotel?" offers: the map's, less one set aside. */
	const hotels = $derived(q.naming.hotels.filter((h) => h.name !== q.rejected_match));
	/** The names offered: whose home, the hotels around (a night is only
	 *  ever asked about hotels), or every named place around. Each name
	 *  once: Virtues' people can hold the same name twice. */
	const offers = $derived.by((): Offer[] => {
		const list: Offer[] =
			kind?.then === 'person'
				? (people ?? []).map((name) => ({ name, category: null }))
				: kind?.then === 'hotel' || q.overnight
					? hotels
					: q.naming.nearby.filter((c) => c.name !== q.rejected_match);
		const seen = new Set<string>(kind ? [] : [current ?? '']);
		return list.filter((o) => !seen.has(o.name) && seen.add(o.name));
	});

	// Another question in this spot: start it clean.
	$effect(() => {
		void q.id;
		picked = [];
		noneOfThese = false;
		kind = null;
		text = '';
	});

	// "Whose home?" suggests the people Virtues knows, fetched the first time.
	$effect(() => {
		if (kind?.then !== 'person' || people) return;
		listPeople()
			.then((ps) => (people = ps.map((p) => p.name).filter(Boolean)))
			.catch((e) => {
				console.warn('[Timeline] people for "whose home" unavailable', e);
				people = [];
			});
	});

	const answer = (a: Answer) => void review.answer(q.id, a);
	const named = (name: string, category: string | null = null): Answer => ({ kind: 'name', name, category });

	/** A button's answer: "No" to Home or Work points to the map instead. */
	function choose(a: Answer) {
		if (ask.role && (a.kind === 'not_home' || a.kind === 'not_work')) review.startPointing(q.id, ask.role);
		else answer(a);
	}

	/** A kind of place, for when the person doesn't know its name: saved as
	 *  it is ("Restaurant"), filed under Work, or followed by which hotel or
	 *  whose home. */
	async function pickKind(k: Kind) {
		if (k.then === 'work') {
			const work = review.confirmed.find((c) => c.source === 'work');
			if (work) void review.fileUnder(q.id, work.id);
			else answer({ kind: 'work' });
			return;
		}
		if (k.then === 'name') return answer(named(k.label, k.category));
		kind = k;
		text = '';
		await Promise.resolve();
		input?.focus();
	}

	/** A name, typed or picked: a home is "<name>'s place"; a hotel is a
	 *  hotel; anything else takes the kind the map knows for that name. */
	function saveName(name: string, category: string | null = null) {
		const t = name.trim();
		if (!t) return;
		if (kind?.then === 'person') return answer(named(homeOf(t)));
		if (kind?.then === 'hotel') return answer(named(t, 'lodging'));
		answer(named(t, category ?? offers.find((o) => o.name === t)?.category ?? null));
	}

	/** Named no further: the kind alone ("Hotel", "Someone's home"). */
	function skip() {
		if (!kind) return;
		answer(named(kind.then === 'hotel' ? 'Hotel' : kind.label, kind.category));
	}

	function togglePick(name: string) {
		picked = picked.includes(name) ? picked.filter((n) => n !== name) : [...picked, name];
	}

	/** The places checked, the close ones and the ones further out together. */
	function savePicks() {
		const close = ask.picks.filter((c) => picked.includes(c.name));
		if (close.length === picked.length && close.length) answer(pickAnswer(close));
		else if (picked.length) answer(named(picked.join(', ')));
	}

	/** The names under a field: the nearest five, or the five holding what's
	 *  typed anywhere in them ("bake" finds "The Riverside Bakehouse").
	 *  Drawn by the card: a browser's own list shows in Safari
	 *  only once the typing matches a name's start. */
	const SUGGESTED = 5;
	function matches(list: Offer[], typed: string): Offer[] {
		const t = typed.trim().toLowerCase();
		return (t ? list.filter((o) => o.name.toLowerCase().includes(t)) : list).slice(0, SUGGESTED);
	}

	/** Yes and No carry a check and a cross. */
	const iconOf = (a: Answer) =>
		a.kind === 'home' || a.kind === 'work' ? 'ri:check-line' : a.kind === 'not_home' || a.kind === 'not_work' ? 'ri:close-line' : null;
	const placeholder = $derived(kind?.then === 'person' ? 'Their name' : kind?.then === 'hotel' ? 'Another hotel' : 'Name this place');
</script>

{#snippet options(items: Offer[])}
	<!-- One list: the names to pick, then a row to type one, as Reminders'
	     new-item row. A name picked from the map's carries its kind. -->
	<ul class="list">
		{#if current && !kind}
			<!-- The answer it has now, checked, as Settings marks the option
			     in use; any other row changes it. -->
			<li class="current" aria-current="true">
				<Icon icon="ri:check-line" width="16" />
				<span>{current}</span>
			</li>
		{/if}
		{#each items as o (o.name)}
			<li>
				<button type="button" disabled={review.saving} onclick={() => saveName(o.name, o.category)}>
					<Icon icon={kind?.then === 'person' ? 'ri:user-line' : 'ri:map-pin-line'} width="16" />
					<span>{o.name}</span>
				</button>
			</li>
		{/each}
		<li class="entry">
			<Icon icon="ri:edit-line" width="16" />
			<input
				bind:this={input}
				bind:value={text}
				{placeholder}
				aria-label={placeholder}
				onkeydown={(e) => e.key === 'Enter' && saveName(text)}
			/>
			{#if text.trim()}<Pill filled disabled={review.saving} onclick={() => saveName(text)}>Save</Pill>{/if}
		</li>
	</ul>
{/snippet}

<div class="ask">
	{#if kind}
		<!-- A hotel or a home: which one, if the person wants to say. -->
		<p class="q">{kind.then === 'hotel' ? 'Which hotel?' : 'Whose home?'}</p>
		{#if kind.then === 'hotel' && !hotels.length}<p class="sub">Virtues couldn’t find any hotels nearby.</p>{/if}
		{@render options(matches(offers, text))}
		<div class="foot">
			<Pill quiet onclick={() => (kind = null)}>‹ Back</Pill>
			<span class="end">
				<Pill quiet disabled={review.saving} onclick={skip}>Skip the Name</Pill>
				{#if onclose}<Pill quiet onclick={onclose}>Close</Pill>{/if}
			</span>
		</div>
	{:else}
		<p class="q">{noneOfThese ? 'What was this spot?' : ask.text}</p>
		{#if sub}<p class="sub">{sub}</p>{/if}
		{#if ask.role}<p class="sub">Virtues uses your home and work to tell the story of your day.</p>{/if}

		{#if ask.picks.length && !noneOfThese}
			<ul class="checklist">
				{#each ask.picks as c (c.name)}
					<li>
						<button type="button" role="checkbox" aria-checked={picked.includes(c.name)} disabled={review.saving} onclick={() => togglePick(c.name)}>
							<span class="check" aria-hidden="true">{#if picked.includes(c.name)}<Icon icon="ri:check-line" width="16" />{/if}</span>
							<span class="pick-name">{c.name}</span>
						</button>
					</li>
				{/each}
			</ul>
			<div class="save">
				<Pill filled size="card" wide disabled={!picked.length || review.saving} onclick={savePicks}>
					{picked.length > 1 ? `Save These ${picked.length}` : 'Save'}
				</Pill>
			</div>
			<div class="foot">
				<Pill quiet onclick={() => (noneOfThese = true)}>None of These</Pill>
			</div>
		{:else if ask.chips.length}
			<div class="buttons">
				{#each ask.chips as c, i (c.label)}
					{@const icon = iconOf(c.answer)}
					<!-- A guess's Yes is filled, as the prototype's card fills it. -->
					<Pill size="card" wide filled={i === 0} disabled={review.saving} onclick={() => choose(c.answer)}>
						{#if icon}<Icon {icon} width="16" />{/if}{c.label}
					</Pill>
				{/each}
			</div>
		{/if}

		{#if open}
			{#if noneOfThese && ask.more.length}
				<!-- More named places around: the close ones weren't it, but
				     the person may have been at one a little further out. -->
				<p class="sub more-hd">More places nearby</p>
				<ul class="checklist">
					{#each ask.more as name (name)}
						<li>
							<button type="button" role="checkbox" aria-checked={picked.includes(name)} disabled={review.saving} onclick={() => togglePick(name)}>
								<span class="check" aria-hidden="true">{#if picked.includes(name)}<Icon icon="ri:check-line" width="16" />{/if}</span>
								<span class="pick-name">{name}</span>
							</button>
						</li>
					{/each}
				</ul>
				{#if picked.length}
					<div class="save">
						<Pill filled size="card" wide disabled={review.saving} onclick={savePicks}>{picked.length > 1 ? `Save These ${picked.length}` : 'Save'}</Pill>
					</div>
				{/if}
			{/if}
			<!-- The places around first, the likely answer in one tap; a name
			     picked from the map's carries its kind of place. Then the
			     field, which also narrows the list as it's typed into; the
			     kinds last, for when the name isn't known (the owner's call,
			     2026-10-10). -->
			{@render options(matches(offers, text))}
			<!-- For when the name isn't known: a second list, under its own
			     header, in the same rows. -->
			<p class="section-hd">Or a kind of place</p>
			<ul class="list">
				{#each ask.kinds as k (k.label)}
					<li>
						<button type="button" disabled={review.saving} onclick={() => void pickKind(k)}>
							<Icon icon={k.icon} width="16" />
							<span>{k.label}</span>
						</button>
					</li>
				{/each}
			</ul>
		{/if}
		{#if (ask.passing && !ask.role) || onclose}
			<div class="foot">
				{#if ask.passing && !ask.role}<Pill quiet disabled={review.saving} onclick={() => answer({ kind: 'passing' })}>Just Passing Through</Pill>{/if}
				{#if onclose}<span class="end"><Pill quiet onclick={onclose}>Close</Pill></span>{/if}
			</div>
		{/if}
	{/if}

	{#if review.saveFailed === q.id}<p class="note">Error: your server couldn't save that. Try again.</p>{/if}
	{#if !q.naming.street_detail && !ask.role}
		<p class="note">Your server's map of this area doesn't list its places, so it can't suggest names here.</p>
	{/if}
</div>

<style>
	.q {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 20px;
		font-weight: 400;
		line-height: 1.25;
	}
	/* Under a serif question, the sans line starts its ink where the
	   question's does: Avenir's capitals sit about 0.08em in from their
	   box, the serif's at its edge, so the line moves out by that much. */
	.sub {
		margin: 4px 0 0;
		font-size: 13px;
		line-height: 1.35;
		text-indent: -0.08em;
		color: var(--color-foreground-muted);
	}

	/* Apple's alert buttons: equal widths, the likely answer filled (Pill). */
	.buttons {
		display: flex;
		gap: 8px;
		margin-top: 16px;
	}
	.save {
		margin-top: 12px;
	}
	.more-hd {
		margin-top: 16px;
	}
	/* A list's header, Apple's: quiet, in line with the rows' icons. */
	.section-hd {
		margin: 16px 0 0;
		padding-left: 13px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	/* Names to pick, as Apple's inset grouped list (Settings, Maps'
	   suggestions): one white rounded box of rows, each with its icon,
	   hairlines between them, its edges in line with the field below. */
	.list {
		margin: 12px 0 0;
		padding: 0;
		list-style: none;
		border: 1px solid var(--color-border);
		border-radius: 12px;
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
		min-height: 40px;
		padding: 8px 12px;
		border: 0;
		background: none;
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 15px;
		text-align: left;
		cursor: pointer;
	}
	.list button :global(svg) {
		flex: none;
		color: var(--color-foreground-muted);
	}
	.list button:hover:not(:disabled) {
		background: var(--hover-bg);
	}
	/* The answer in use: a row like the others, not pressable, its tick in
	   the accent. */
	.list .current {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 40px;
		padding: 8px 12px;
		font-size: 15px;
	}
	.list .current > :global(svg) {
		flex: none;
		color: var(--color-primary);
	}
	/* The last row types a name: the same height and inset as a row, the
	   field itself borderless, Save only once there's something to save. */
	.list .entry {
		display: flex;
		align-items: center;
		gap: 8px;
		min-height: 40px;
		padding: 4px 4px 4px 12px;
	}
	.list .entry > :global(svg) {
		flex: none;
		color: var(--color-foreground-muted);
	}
	.entry input {
		flex: 1;
		min-width: 0;
		height: 32px;
		padding: 0;
		border: 0;
		outline: none;
		background: none;
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 15px;
	}
	.entry input::placeholder {
		color: var(--color-foreground-muted);
	}
	.list:has(.entry input:focus-visible) {
		border-color: var(--color-border-focus);
	}
	/* Apple's settings list: a checkmark on each row picked. */
	.checklist {
		margin: 12px 0 0;
		padding: 0;
		list-style: none;
		border-top: 1px solid var(--color-border);
	}
	.checklist li {
		border-bottom: 1px solid var(--color-border);
	}
	.checklist button {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		min-height: 40px;
		padding: 8px 0;
		border: 0;
		background: none;
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 15px;
		text-align: left;
		cursor: pointer;
	}
	/* Virtues' own tick (MenuItem, the pickers), in the accent. */
	.check {
		display: inline-flex;
		width: 16px;
		color: var(--color-primary);
	}
	.pick-name {
		flex: 1;
	}

	/* One row at the foot: the way back or past on the left, Skip and Close
	   on the right. */
	.foot {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 16px;
	}
	.foot .end {
		display: flex;
		gap: 8px;
		margin-left: auto;
	}
	.note {
		margin: 12px 0 0;
		font-size: 12px;
		line-height: 1.4;
		color: var(--color-foreground-muted);
	}
</style>
