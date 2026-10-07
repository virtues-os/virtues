<!--
	TimelineReview.svelte - "Help Virtues out" (dayback/src/main.js:2113-2201,
	dayback/index.html:266-326): a pill under the date card with the count of
	places to name, and the card it opens below it.

	The card asks about the places of the day on show first, then the rest
	from other days: each place is still asked once, and its answer names it
	on every day (TIMELINE-DESIGN-PRINCIPLES.md, "Resolve once per entity").
	First whether the place with the most nights is home and the place with
	the most time otherwise is where the person works (asked, never assumed),
	then the rest, the most time first: which of several close places (any
	number picked together), whether it was a named place just out of reach,
	whose home after a night there, else what it was. "No" to Home or Work
	asks the person to tap the right place on the map instead, or skip; either
	way it isn't asked again unless another place takes over. Each place question
	offers Other and Just passing through. An answer can be undone for a few
	seconds after. Places already named are listed under "N named", where a
	wrong one is marked and corrected.

	The card copies Apple's one-question review (Photos' "Is this Sam?"): the
	question is the title, one line says when the person was there, and the
	answers are buttons. The map shows which place: the card names none, as
	its only name would be the city it's in.
-->
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { listPeople } from '$lib/wiki/api';
	import {
		askFor,
		weekly,
		fetchReview,
		homeOf,
		pickAnswer,
		sendAnswer,
		type Answer,
		type ConfirmedPlace,
		type PlaceCandidate,
		type PlaceQuestion,
		type PlaceReview,
	} from '$lib/timeline/review';

	let {
		open = $bindable(false),
		dayPlaces = [],
		room = 480,
		onfocus,
		onday,
		onanswered,
		onpoint,
	}: {
		open?: boolean;
		/** The places of the day on show, asked about first. */
		dayPlaces?: string[];
		/** The card's tallest, in px: down to a gap above the scrubber. */
		room?: number;
		/** The place being asked about, the named places around it, and where
		 *  the card ends on screen (its right edge), for the map to go to the
		 *  place clear of the card; `null` lets go. */
		onfocus: (at: { lat: number; lon: number; reachM: number; shown: PlaceCandidate[]; cardRight: number } | null) => void;
		/** "See that day": open that day. */
		onday: (slug: string) => void;
		/** An answer was saved or undone: the day's titles may have changed. */
		onanswered: () => void;
		/** Home or Work is elsewhere: the view takes over with its own mode
		 *  (a banner, a tap on the map) and hands the tap back to
		 *  `savePointed`; `null` ends it. */
		onpoint: (role: 'home' | 'work' | null) => void;
	} = $props();

	let review = $state<PlaceReview | null>(null);
	let failed = $state(false);
	let saveFailed = $state(false);
	let saving = $state(false);
	let mode = $state<'questions' | 'confirmed'>('questions');
	let idx = $state(0);
	let otherOpen = $state(false);
	let otherText = $state('');
	let otherInput = $state<HTMLInputElement | null>(null);
	/** "Which of these": the places picked so far, by name. */
	let picked = $state<string[]>([]);
	/** A named place marked wrong, whose right name is being typed. */
	let flagged = $state<string | null>(null);
	let people = $state<string[] | null>(null);
	/** The last answer, undoable for a few seconds: the places it touched. */
	let undo = $state<{ ids: string[]; said: string } | null>(null);
	/** "No" to Home or Work: the role being pointed to on the map. */
	let pointing = $state<'home' | 'work' | null>(null);
	let undoTimer = 0;
	let card = $state<HTMLElement | null>(null);

	/** The day's places first, then the rest, each in the server's order:
	 *  Home and Work first, then the most time. */
	const onDay = $derived((review?.questions ?? []).filter((q) => dayPlaces.includes(q.id)));
	const questions = $derived([...onDay, ...(review?.questions ?? []).filter((q) => !dayPlaces.includes(q.id))]);
	const at = $derived(Math.min(idx, questions.length - 1));
	const current = $derived<PlaceQuestion | null>(questions[at] ?? null);
	const ask = $derived(current ? askFor(current) : null);
	const confirmed = $derived(review?.confirmed ?? []);
	const suggestions = $derived([...new Set(ask?.other === 'person' ? (people ?? []) : (current?.naming.nearby ?? []))]);

	async function load() {
		try {
			review = await fetchReview();
			failed = false;
		} catch (e) {
			console.warn('[Timeline] place review fetch failed', e);
			failed = true;
		}
	}
	onMount(() => void load());

	// Another day on show: its places come first, so start from the top.
	$effect(() => {
		void dayPlaces;
		untrack(() => {
			idx = 0;
			resetOther();
		});
	});

	// The map follows the question on show; anything else lets it go,
	// pointing to Home or Work elsewhere included.
	$effect(() => {
		const q = open && mode === 'questions' && !pointing ? current : null;
		const right = card?.getBoundingClientRect().right ?? 0;
		onfocus(q && ask ? { lat: q.latitude, lon: q.longitude, reachM: ask.reachM, shown: ask.shown, cardRight: right } : null);
	});

	// Closed some other way (Esc, the month): stop waiting for a tap.
	$effect(() => {
		if (!open) untrack(() => pointing && resetOther());
	});

	// "Whose home" suggests the people Virtues knows, fetched the first time.
	$effect(() => {
		if (ask?.other !== 'person' || people) return;
		listPeople()
			.then((ps) => (people = ps.map((p) => p.name).filter(Boolean)))
			.catch((e) => {
				console.warn('[Timeline] people for "whose place" unavailable', e);
				people = [];
			});
	});

	export function toggle() {
		open = !open;
		if (open) {
			mode = 'questions';
			idx = 0;
			void load();
		}
		resetOther();
	}

	function resetOther() {
		if (pointing) onpoint(null);
		pointing = null;
		otherOpen = false;
		otherText = '';
		picked = [];
		flagged = null;
		saveFailed = false;
	}

	function page(dir: 1 | -1) {
		idx = Math.max(0, Math.min(questions.length - 1, at + dir));
		resetOther();
	}

	/** What an answer said, for its Undo line. */
	function said(a: Answer): string {
		switch (a.kind) {
			case 'name':
				return `Saved as ${a.name}`;
			case 'passing':
				return 'Saved as passing through';
			case 'home':
				return 'Saved as Home';
			case 'work':
				return 'Saved as where you work';
			case 'not_home':
				return 'Saved: not home';
			case 'not_work':
				return 'Saved: not where you work';
			case 'not_this':
				return `Saved: not ${a.name}`;
			case 'home_here':
				return 'Saved as Home';
			case 'work_here':
				return 'Saved as where you work';
			default:
				return 'Saved';
		}
	}

	async function answer(id: string, a: Answer) {
		if (saving) return;
		saving = true;
		saveFailed = false;
		try {
			const done = await sendAnswer(id, a);
			await load();
			resetOther();
			// The answered place leaves the list; the next one takes its place.
			idx = Math.min(at, Math.max(0, questions.length - 1));
			clearTimeout(undoTimer);
			undo = a.kind === 'reopen' ? null : { ids: [id, ...(done.place_id ? [done.place_id] : [])], said: said(a) };
			undoTimer = window.setTimeout(() => (undo = null), 8000);
			onanswered();
		} catch (e) {
			console.warn('[Timeline] place answer failed', e);
			saveFailed = true;
		} finally {
			saving = false;
		}
	}

	/** Every answer is about a place that was a question or named; forgetting
	 *  it puts the place back where it was, and the card goes back to it. */
	async function undoLast() {
		const u = undo;
		if (!u) return;
		undo = null;
		clearTimeout(undoTimer);
		for (const id of [...u.ids].reverse()) await answer(id, { kind: 'reopen' });
		undo = null;
		const i = questions.findIndex((q) => q.id === u.ids[0]);
		if (i >= 0) {
			idx = i;
			mode = 'questions';
		}
	}

	/** A button's answer: "No" to Home or Work points to the map instead. */
	function choose(a: Answer) {
		if (!current || !ask) return;
		if (ask.role && (a.kind === 'not_home' || a.kind === 'not_work')) {
			pointing = ask.role;
			onpoint(pointing);
			return;
		}
		void answer(current.id, a);
	}

	/** Home or Work is at the spot tapped on the map. */
	export function savePointed(at: { lat: number; lon: number }) {
		if (!current || !pointing) return;
		void answer(current.id, { kind: pointing === 'home' ? 'home_here' : 'work_here', ...at });
	}

	/** Back to the question. */
	export function cancelPointing() {
		resetOther();
	}

	/** Skipped: not this place, and not asked again unless another takes over. */
	export function skipPointing() {
		if (!current || !pointing) return;
		void answer(current.id, { kind: pointing === 'home' ? 'not_home' : 'not_work' });
	}

	function togglePick(name: string) {
		picked = picked.includes(name) ? picked.filter((n) => n !== name) : [...picked, name];
	}

	function savePicks() {
		if (!current || !ask) return;
		const chosen = ask.picks.filter((c) => picked.includes(c.name));
		if (chosen.length) void answer(current.id, pickAnswer(chosen));
	}

	function submitOther() {
		const text = otherText.trim();
		if (!current || !ask || !text) return;
		void answer(current.id, { kind: 'name', name: ask.other === 'person' ? homeOf(text) : text });
	}

	async function openOther() {
		otherOpen = true;
		await Promise.resolve();
		otherInput?.focus();
	}

	/** A named place's right name, typed after marking it wrong (main.js:
	 *  2141): a name saves it; left blank, the map's match is set aside, and
	 *  the person's own answer (a name, Home, Work) is forgotten, so the place
	 *  is asked about again. */
	function correct(c: ConfirmedPlace, value: string) {
		const v = value.trim();
		if (v) void answer(c.id, { kind: 'name', name: v });
		else if (c.source === 'map') void answer(c.id, { kind: 'not_this', name: c.title });
		else void answer(c.id, { kind: 'reopen' });
	}

	function seeDay(slug: string) {
		open = false;
		onday(slug);
	}

	const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? '' : 's'}`;
	const duration = (h: number) => (h < 1 ? `${Math.round(h * 60)} min` : `${h} ${h === 1 ? 'hour' : 'hours'}`);
	/** Why a place is named (main.js:2113). */
	const SOURCE = { home: 'Your home', work: 'Where you work', named: 'Named by you', map: 'From the map' };
	/** Yes and No carry Maps' check and cross. */
	const iconOf = (a: Answer) =>
		a.kind === 'home' || a.kind === 'work' ? 'ri:check-line' : a.kind === 'not_home' || a.kind === 'not_work' ? 'ri:close-line' : null;
</script>

<div class="review">
	<button class="pill tile" type="button" aria-expanded={open} onclick={toggle}>
		<span class="ic" aria-hidden="true">◎</span>Help Virtues out
		{#if review?.questions.length}<span class="badge">{review.questions.length}</span>{/if}
	</button>

	{#if open && !pointing}
		<div class="card tile" role="dialog" aria-label="Help Virtues out" bind:this={card} style="max-height: {room}px">
			{#if failed && !review}
				<div class="pad"><p class="note">Error: your server couldn't load your places. Reload the page to try again.</p></div>
			{:else if !review}
				<div class="pad"><p class="note">Loading your places…</p></div>
			{:else if mode === 'confirmed'}
				<div class="top">
					<TextAction quiet onclick={() => (mode = 'questions')}>‹ Questions</TextAction>
					<IconButton icon="ri:close-line" label="Close" size="sm" onclick={toggle} />
				</div>
				<div class="pad">
					<h3 class="q">{plural(confirmed.length, 'named place')}</h3>
					<p class="sub">Mark one with ✕ only if it's wrong.</p>
					{#each confirmed as c (c.id)}
						<div class="row" class:flagged={flagged === c.id}>
							<div class="row-main">
								<div class="row-name">{c.title}</div>
								<div class="row-meta">{SOURCE[c.source]} · {plural(c.stop_count, 'visit')} · {duration(c.hours)}</div>
								{#if flagged === c.id}
									<input
										class="field"
										placeholder="What was it? - or leave blank"
										aria-label="What was it?"
										disabled={saving}
										onkeydown={(e) => e.key === 'Enter' && correct(c, (e.currentTarget as HTMLInputElement).value)}
									/>
								{/if}
							</div>
							<button
								class="wrong"
								type="button"
								aria-label="Mark as wrong"
								aria-pressed={flagged === c.id}
								disabled={saving}
								onclick={() => (flagged = flagged === c.id ? null : c.id)}>✕</button
							>
						</div>
					{/each}
					{#if saveFailed}<p class="note">Error: your server couldn't save that. Try again.</p>{/if}
				</div>
			{:else if current && ask}
				<div class="top">
					<span class="nav">
						<IconButton icon="ri:arrow-left-s-line" label="Previous place" size="xs" disabled={at <= 0} onclick={() => page(-1)} />
						<span class="counter">{at + 1} of {questions.length}</span>
						<IconButton icon="ri:arrow-right-s-line" label="Next place" size="xs" disabled={at >= questions.length - 1} onclick={() => page(1)} />
						<span class="counter scope">{at < onDay.length ? 'This day' : 'Other days'}</span>
					</span>
					<IconButton icon="ri:close-line" label="Close" size="sm" onclick={toggle} />
				</div>
				<div class="pad">
					<h3 class="q">{ask.text}</h3>
					{#if ask.role}<p class="sub">{weekly(current)}</p>
					{:else if current.recall}<p class="sub">{current.recall}</p>{/if}

					{#if ask.picks.length}
						<ul class="checklist">
							{#each ask.picks as c (c.name)}
								<li>
									<button type="button" role="checkbox" aria-checked={picked.includes(c.name)} disabled={saving} onclick={() => togglePick(c.name)}>
										<span class="check" aria-hidden="true">{picked.includes(c.name) ? '✓' : ''}</span>
										<span class="pick-name">{c.name}</span>
									</button>
								</li>
							{/each}
						</ul>
						<button class="btn primary wide" type="button" disabled={!picked.length || saving} onclick={savePicks}>
							{picked.length > 1 ? `Save These ${picked.length}` : 'Save'}
						</button>
					{:else}
						<div class="buttons" class:grid={ask.chips.length > 2}>
							{#each ask.chips as c, i (c.label)}
								{@const icon = iconOf(c.answer)}
								<button class="btn" class:primary={i === 0 && ask.chips.length <= 2} type="button" disabled={saving} onclick={() => choose(c.answer)}>
									{#if icon}<Icon {icon} width="16" />{/if}{c.label}
								</button>
							{/each}
						</div>
					{/if}

					{#if ask.other}
						{#if otherOpen}
							<div class="other">
								<input
									class="field"
									bind:this={otherInput}
									bind:value={otherText}
									list="tl-review-suggest"
									placeholder={ask.other === 'person' ? 'Whose place?' : 'Type its name'}
									aria-label={ask.other === 'person' ? 'Whose place?' : 'What was it?'}
									onkeydown={(e) => e.key === 'Enter' && submitOther()}
								/>
								<button class="btn primary" type="button" disabled={!otherText.trim() || saving} onclick={submitOther}>Save</button>
							</div>
							<datalist id="tl-review-suggest">
								{#each suggestions as s (s)}<option value={s}></option>{/each}
							</datalist>
						{/if}
						<div class="quiet">
							{#if !otherOpen}<TextAction onclick={() => void openOther()}>Other…</TextAction>{/if}
							<TextAction quiet disabled={saving} onclick={() => void answer(current.id, { kind: 'passing' })}>Just Passing Through</TextAction>
						</div>
					{/if}

					{#if saveFailed}<p class="note">Error: your server couldn't save that. Try again.</p>{/if}
					{#if !current.naming.street_detail && ask.other}
						<p class="note">Your server's map of this area doesn't list its places, so it can't suggest names here.</p>
					{/if}
				</div>
				<div class="foot">
					{#if current.best_day}
						{@const day = current.best_day}
						<TextAction onclick={() => seeDay(day)}>See That Day</TextAction>
					{/if}
					<span class="spacer"></span>
					<TextAction quiet onclick={() => (mode = 'confirmed')}>{confirmed.length} Named</TextAction>
				</div>
			{:else}
				<div class="top">
					<span></span>
					<IconButton icon="ri:close-line" label="Close" size="sm" onclick={toggle} />
				</div>
				<div class="pad">
					<h3 class="q">All set</h3>
					<p class="sub">Nothing to sort out.</p>
				</div>
				<div class="foot">
					<TextAction onclick={() => (mode = 'confirmed')}>Review {plural(confirmed.length, 'Named Place')}</TextAction>
				</div>
			{/if}

			{#if undo}
				<div class="undo" role="status">
					<span>{undo.said}</span>
					<TextAction onclick={() => void undoLast()}>Undo</TextAction>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	/* The Timeline's one material: the surface in a hairline (TimelineView's
	   .tile), never a shadow. */
	.tile {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
	}
	.review {
		position: relative;
		display: inline-block;
		margin-top: 8px;
	}
	.pill {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		height: 32px;
		padding: 0 12px;
		border-radius: 999px;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.pill:hover,
	.pill[aria-expanded='true'] {
		color: var(--color-foreground);
	}
	.ic {
		color: var(--c-place);
	}
	.badge {
		min-width: 16px;
		height: 16px;
		padding: 0 4px;
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-foreground-muted) 22%, transparent);
		color: var(--color-foreground-muted);
		font-size: 11px;
		font-weight: 600;
		line-height: 16px;
		text-align: center;
	}

	.card {
		position: absolute;
		top: calc(100% + 8px);
		left: 0;
		z-index: 13;
		width: 320px;
		max-width: calc(100vw - 32px);
		display: flex;
		flex-direction: column;
		border-radius: var(--tile-radius);
		overflow-y: auto;
		font-family: var(--font-sans);
		color: var(--color-foreground);
	}

	.top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		padding: 8px 8px 0 12px;
		min-height: 36px;
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
	.scope {
		margin-left: 8px;
	}
	.pad {
		padding: 4px 16px 16px;
	}
	.q {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 20px;
		font-weight: 400;
		line-height: 1.25;
	}
	.sub {
		margin: 4px 0 0;
		font-size: 13px;
		line-height: 1.35;
		color: var(--color-foreground-muted);
	}

	/* Apple's alert buttons: equal widths, the likely answer filled. */
	.buttons {
		display: flex;
		gap: 8px;
		margin-top: 16px;
	}
	.buttons.grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
	}
	.btn {
		flex: 1;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		min-height: 36px;
		padding: 8px 12px;
		border: 0;
		border-radius: 12px;
		background: color-mix(in srgb, var(--color-foreground) 7%, transparent);
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 15px;
		font-weight: 500;
		cursor: pointer;
	}
	.btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-foreground) 11%, transparent);
	}
	.btn.primary {
		background: var(--color-primary);
		color: var(--color-background);
	}
	.btn.primary:hover:not(:disabled) {
		background: var(--color-primary-hover);
	}
	.btn:disabled {
		opacity: 0.4;
		cursor: default;
	}
	.btn.wide {
		width: 100%;
		margin-top: 12px;
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
	.check {
		width: 16px;
		color: var(--color-primary);
		font-weight: 600;
	}
	.pick-name {
		flex: 1;
	}

	.other {
		display: flex;
		gap: 8px;
		margin-top: 12px;
	}
	.field {
		flex: 1;
		min-width: 0;
		padding: 8px 12px;
		border: 1px solid var(--color-border);
		border-radius: 12px;
		background: var(--color-surface);
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 15px;
	}
	.row .field {
		width: 100%;
		margin-top: 8px;
	}
	.quiet {
		display: flex;
		gap: 16px;
		margin-top: 12px;
		font-size: 13px;
	}
	.note {
		margin: 12px 0 0;
		font-size: 12px;
		line-height: 1.4;
		color: var(--color-foreground-muted);
	}

	.foot {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 16px 12px;
		border-top: 1px solid var(--color-border);
		font-size: 13px;
	}
	.spacer {
		flex: 1;
	}

	.undo {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 16px;
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
