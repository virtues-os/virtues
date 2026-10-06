<!--
	TimelineReview.svelte - "Help Virtues out" (dayback/src/main.js:2113-2201,
	dayback/index.html:266-326): a pill under the date card with the count of
	places to name, and the card it opens below it.

	The card asks about the places of the day on show first, then says how
	many more there are from other days and carries on through them: each
	place is still asked once, and its answer names it on every day
	(TIMELINE-DESIGN-PRINCIPLES.md, "Resolve once per entity").

	The card asks about one place at a time: first whether the place with the
	most nights is home and the place with the most time otherwise is where
	the person works (asked, never assumed), then the rest, the most time
	first: which of several close places (any number picked together),
	whether it was a named place just out of reach, whose home after a night
	there, else what it was. Each question offers Other (a typed answer, with
	the named places around it or the people Virtues knows as suggestions) and
	Just passing through, and below them what jogs the memory: when, a line
	from a conversation there, and that full day. The map goes to the place
	while it's asked about. Places already named (a clear match on the map,
	the person's own names, Home and Work as the person set them) are listed under
	the checkmark, where a wrong one is marked and corrected.
-->
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { listPeople } from '$lib/wiki/api';
	import {
		askFor,
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
		onfocus,
		onday,
		onanswered,
	}: {
		open?: boolean;
		/** The places of the day on show, asked about first. */
		dayPlaces?: string[];
		/** The place being asked about and the named places around it, for the
		 *  map to go to and label; `null` lets go. */
		onfocus: (at: { lat: number; lon: number; reachM: number; shown: PlaceCandidate[] } | null) => void;
		/** "See that full day": open that day. */
		onday: (slug: string) => void;
		/** An answer was saved: the day's titles may have changed. */
		onanswered: () => void;
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
	/** A confirmed place marked wrong, whose right name is being typed. */
	let flagged = $state<string | null>(null);
	let people = $state<string[] | null>(null);

	/** The day's places first, then the rest, each in the server's order:
	 *  Home and Work first, then the most time. */
	const onDay = $derived((review?.questions ?? []).filter((q) => dayPlaces.includes(q.id)));
	const questions = $derived([...onDay, ...(review?.questions ?? []).filter((q) => !dayPlaces.includes(q.id))]);
	const fromOthers = $derived(questions.length - onDay.length);
	const at = $derived(Math.min(idx, questions.length - 1));
	const confirmed = $derived(review?.confirmed ?? []);
	const current = $derived<PlaceQuestion | null>(questions[at] ?? null);
	const ask = $derived(current ? askFor(current) : null);
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

	// The map follows the question on show; anything else lets it go.
	$effect(() => {
		const q = open && mode === 'questions' ? current : null;
		onfocus(q && ask ? { lat: q.latitude, lon: q.longitude, reachM: ask.reachM, shown: ask.shown } : null);
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
		otherOpen = false;
		otherText = '';
		picked = [];
		flagged = null;
		saveFailed = false;
	}

	function page(dir: 1 | -1) {
		idx = Math.max(0, Math.min(questions.length - 1, idx + dir));
		resetOther();
	}

	async function answer(id: string, a: Answer) {
		if (saving) return;
		saving = true;
		saveFailed = false;
		try {
			await sendAnswer(id, a);
			await load();
			resetOther();
			// The answered place leaves the list; the next one takes its place.
			idx = Math.min(idx, Math.max(0, questions.length - 1));
			onanswered();
		} catch (e) {
			console.warn('[Timeline] place answer failed', e);
			saveFailed = true;
		} finally {
			saving = false;
		}
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

	/** A confirmed place's right name, typed after marking it wrong (main.js:
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
	/** Why a place is named (main.js:2113). */
	const SOURCE = { home: 'your home', work: 'where you work', named: 'named by you', map: 'from the map' };
</script>

<button class="pill tile" type="button" aria-expanded={open} onclick={toggle}>
	<span class="ic" aria-hidden="true">◎</span>Help Virtues out
	{#if questions.length}<span class="badge">{questions.length}</span>{/if}
</button>

{#if open}
	<div class="card tile" role="dialog" aria-label="Help Virtues out">
		<div class="hd">
			<span class="title">Help Virtues out</span>
			{#if mode === 'questions' && questions.length}
				<span class="nav">
					<IconButton icon="ri:arrow-left-s-line" label="Previous place" size="sm" disabled={idx <= 0} onclick={() => page(-1)} />
					<span class="count">{at + 1} of {questions.length}</span>
					<IconButton icon="ri:arrow-right-s-line" label="Next place" size="sm" disabled={idx >= questions.length - 1} onclick={() => page(1)} />
				</span>
			{/if}
			<IconButton icon="ri:close-line" label="Close" size="sm" onclick={toggle} />
		</div>

		<div class="body">
			{#if failed && !review}
				<p class="note">Error: your server couldn't load your places. Reload the page to try again.</p>
			{:else if !review}
				<p class="note">Loading your places…</p>
			{:else if mode === 'confirmed'}
				<TextAction quiet onclick={() => (mode = 'questions')}>‹ Back to questions</TextAction>
				<p class="hint">Mark one with ✕ only if it's wrong.</p>
				{#each confirmed as c (c.id)}
					<div class="row" class:flagged={flagged === c.id}>
						<div class="row-main">
							<div class="row-name">{c.title}</div>
							<div class="row-meta">{SOURCE[c.source]} · {plural(c.stop_count, 'visit')} · {c.hours}h</div>
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
						<span class="seg">
							<span class="ok" aria-hidden="true">✓</span>
							<button
								class="no"
								type="button"
								aria-label="Mark as wrong"
								aria-pressed={flagged === c.id}
								disabled={saving}
								onclick={() => (flagged = flagged === c.id ? null : c.id)}>✕</button
							>
						</span>
					</div>
				{/each}
				{#if saveFailed}<p class="note">Error: your server couldn't save that. Try again.</p>{/if}
			{:else if current && ask}
				{#if at < onDay.length}
					<p class="scope">This day</p>
				{:else if at === onDay.length}
					<p class="scope">
						{onDay.length ? "That's all from this day." : 'Nothing to ask about this day.'}
						{plural(fromOthers, 'place')} from other days.
					</p>
				{:else}
					<p class="scope">From another day</p>
				{/if}
				<p class="q">{ask.text}</p>
				<div class="chips">
					{#each ask.picks as c (c.name)}
						<button class="chip" type="button" aria-pressed={picked.includes(c.name)} disabled={saving} onclick={() => togglePick(c.name)}>{c.name}</button>
					{/each}
					{#each ask.chips as c (c.label)}
						<button class="chip" type="button" disabled={saving} onclick={() => void answer(current.id, c.answer)}>{c.label}</button>
					{/each}
					{#if !ask.other}
						<!-- A yes-or-no question: no typed answer, no passing through. -->
					{:else if otherOpen}
						<input
							class="field"
							bind:this={otherInput}
							bind:value={otherText}
							list="tl-review-suggest"
							placeholder={ask.other === 'person' ? 'Whose place?' : 'Type its name'}
							aria-label={ask.other === 'person' ? 'Whose place?' : 'What was it?'}
							onkeydown={(e) => e.key === 'Enter' && submitOther()}
						/>
					{:else}
						<button class="chip ghost" type="button" onclick={() => void openOther()}>Other</button>
					{/if}
					{#if ask.other}
						<button class="chip ghost" type="button" disabled={saving} onclick={() => void answer(current.id, { kind: 'passing' })}>Just passing through</button>
					{/if}
				</div>
				{#if ask.picks.length}
					<p class="picks">
						<TextAction disabled={!picked.length || saving} onclick={savePicks}>
							{picked.length > 1 ? `Save these ${picked.length}` : 'Save'}
						</TextAction>
						<span class="hint">Pick each one you went to.</span>
					</p>
				{/if}
				<datalist id="tl-review-suggest">
					{#each suggestions as s (s)}<option value={s}></option>{/each}
				</datalist>
				{#if saveFailed}<p class="note">Error: your server couldn't save that. Try again.</p>{/if}
				<div class="ctx">
					<div class="ctx-label">More context</div>
					<p class="recall">
						{current.recall || `${plural(current.stop_count, 'visit')} · ${current.hours}h`}{current.naming.city ? ` · near ${current.naming.city}` : ''}
					</p>
					{#if current.snippet}<p class="snippet">“{current.snippet}”</p>{/if}
					{#if !current.naming.street_detail}
						<p class="snippet">Your server's map of this area doesn't list its places, so it can't suggest names here.</p>
					{/if}
				</div>
				<div class="links">
					{#if current.best_day}
						{@const day = current.best_day}
						<TextAction onclick={() => seeDay(day)}>See that full day →</TextAction>
					{/if}
					<TextAction quiet onclick={() => (mode = 'confirmed')}>{confirmed.length} named ✓</TextAction>
				</div>
			{:else}
				<p class="done">All set - nothing to sort out.</p>
				<TextAction onclick={() => (mode = 'confirmed')}>Review {plural(confirmed.length, 'named place')} ›</TextAction>
			{/if}
		</div>
	</div>
{/if}

<style>
	/* The Timeline's one material: the surface in a hairline (TimelineView's
	   .tile), never a shadow. */
	.tile {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
	}
	.pill {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		height: 32px;
		margin-top: 8px;
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
		width: 336px;
		max-width: calc(100vw - 32px);
		max-height: 48vh;
		display: flex;
		flex-direction: column;
		border-radius: var(--tile-radius);
		overflow: hidden;
		font-family: var(--font-sans);
		color: var(--color-foreground);
	}
	.hd {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px 12px 8px 16px;
	}
	.title {
		flex: 1;
		font-family: var(--font-serif);
		font-size: 16px;
	}
	.nav {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.count {
		min-width: 44px;
		font-size: 12px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		text-align: center;
	}
	.body {
		overflow-y: auto;
		padding: 8px 16px 16px;
		border-top: 1px solid var(--color-border);
	}
	.scope {
		margin: 4px 0 0;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}
	.q {
		margin: 4px 0 12px;
		font-family: var(--font-serif);
		font-size: 16px;
		line-height: 1.3;
	}
	.chips {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
		margin-bottom: 12px;
	}
	.chip {
		padding: 8px 12px;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		background: var(--color-surface);
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
	}
	.chip:hover:not(:disabled) {
		border-color: var(--color-foreground-muted);
	}
	.chip[aria-pressed='true'] {
		border-color: var(--color-primary);
		color: var(--color-primary);
	}
	.chip.ghost {
		border-style: dashed;
		color: var(--color-foreground-muted);
		font-size: 12px;
	}
	.field {
		flex: 1;
		min-width: 150px;
		padding: 8px 12px;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		background: var(--color-surface);
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 13px;
	}
	.row .field {
		margin-top: 8px;
		width: 100%;
	}
	.picks {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 4px 12px;
		white-space: nowrap;
		margin: 0 0 16px;
		font-size: 13px;
	}
	.ctx-label {
		margin-bottom: 4px;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
	}
	.recall,
	.snippet,
	.note,
	.hint {
		margin: 0;
		font-size: 12px;
		line-height: 1.4;
		color: var(--color-foreground-muted);
	}
	.snippet {
		margin-top: 4px;
		color: var(--color-foreground-subtle);
	}
	.note {
		margin-bottom: 12px;
	}
	.hint {
		margin: 8px 0 4px;
	}
	.picks .hint {
		margin: 0;
	}
	.links {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 12px;
		margin-top: 12px;
		font-size: 13px;
	}
	.done {
		margin: 12px 0 8px;
		font-family: var(--font-serif);
		font-size: 16px;
	}
	.row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 0;
		border-top: 1px solid var(--color-border);
	}
	.row-main {
		flex: 1;
		min-width: 0;
	}
	.row-name {
		font-family: var(--font-serif);
		font-size: 14px;
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
	.seg {
		display: inline-flex;
		gap: 4px;
	}
	.ok,
	.no {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 28px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: none;
		font-size: 13px;
		font-weight: 600;
		color: var(--color-foreground-muted);
	}
	.ok {
		color: var(--color-foreground);
	}
	.no {
		cursor: pointer;
	}
	.no:hover:not(:disabled),
	.no[aria-pressed='true'] {
		color: var(--color-foreground);
		border-color: var(--color-foreground-muted);
	}
</style>
