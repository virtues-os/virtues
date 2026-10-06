<!--
	TimelineReview.svelte - "Help Virtues out" (dayback/src/main.js:2113-2201,
	dayback/index.html:266-326): a pill under the date card with the count of
	places to name, and the card it opens.

	The card asks about the places of the day on show first, then the rest
	from other days: each place is still asked once, and its answer names it
	on every day (TIMELINE-DESIGN-PRINCIPLES.md, "Resolve once per entity").
	First whether the place with the most nights is home and the place with
	the most time otherwise is where the person works (asked, never assumed),
	then the rest, the most time first: which of several close places (any
	number picked together), whether it was a named place just out of reach,
	whose home after a night there, else what it was. Each question offers
	Other and Just passing through, and below them a line from a conversation
	there and that full day. An answer can be undone for a few seconds after.
	Places already named are listed under "N named", where a wrong one is
	marked and corrected.

	The card copies Apple's one-question review (Photos' "Is this Sam?"): the
	place first, large, then the question and its answers as buttons. A
	dev-only switch compares it with Apple Maps' place card, and the card
	under the pill with a popover beside it.
-->
<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
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
		room = 480,
		onfocus,
		onday,
		onanswered,
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
	} = $props();

	// ─── Dev only: compare the layouts and placements; never shipped ───
	type Layout = 'photos' | 'maps';
	type Placement = 'below' | 'popover';
	const DEV = import.meta.env.DEV;
	const stored = <T extends string>(key: string, allowed: readonly T[], fallback: T): T => {
		try {
			const v = localStorage.getItem(key);
			return allowed.includes(v as T) ? (v as T) : fallback;
		} catch {
			return fallback;
		}
	};
	let layout = $state<Layout>(DEV ? stored('tl-review-layout', ['photos', 'maps'] as const, 'photos') : 'photos');
	let placement = $state<Placement>(DEV ? stored('tl-review-placement', ['below', 'popover'] as const, 'below') : 'below');
	function remember(key: string, v: string) {
		try {
			localStorage.setItem(key, v);
		} catch {
			// A private window: the choice lasts this visit only.
		}
	}

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
	/** The last answer, undoable for a few seconds. */
	let undo = $state<{ id: string; said: string } | null>(null);
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
	const counter = $derived(`${at + 1} of ${questions.length} · ${at < onDay.length ? 'This day' : 'Other days'}`);

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
		void placement;
		const right = card?.getBoundingClientRect().right ?? 0;
		onfocus(q && ask ? { lat: q.latitude, lon: q.longitude, reachM: ask.reachM, shown: ask.shown, cardRight: right } : null);
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
			default:
				return 'Saved';
		}
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
			idx = Math.min(at, Math.max(0, questions.length - 1));
			clearTimeout(undoTimer);
			undo = a.kind === 'reopen' ? null : { id, said: said(a) };
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
		await answer(u.id, { kind: 'reopen' });
		const i = questions.findIndex((q) => q.id === u.id);
		if (i >= 0) {
			idx = i;
			mode = 'questions';
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
	/** An answer's icon in the Maps layout's action row. */
	const iconOf = (a: Answer) =>
		a.kind === 'home' || a.kind === 'work' ? 'ri:check-line' : a.kind === 'not_home' || a.kind === 'not_work' ? 'ri:close-line' : 'ri:map-pin-line';
</script>

<div class="review" class:popover={placement === 'popover'}>
	<button class="pill tile" type="button" aria-expanded={open} onclick={toggle}>
		<span class="ic" aria-hidden="true">◎</span>Help Virtues out
		{#if review?.questions.length}<span class="badge">{review.questions.length}</span>{/if}
	</button>

	{#if open && placement === 'popover'}<span class="arrow" aria-hidden="true"></span>{/if}
	{#if open}
		<div class="card tile" class:maps={layout === 'maps'} role="dialog" aria-label="Help Virtues out" bind:this={card} style="max-height: {room}px">
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
					<h3 class="subject">{plural(confirmed.length, 'named place')}</h3>
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
					<span class="counter">{counter}</span>
					<IconButton icon="ri:close-line" label="Close" size="sm" onclick={toggle} />
				</div>
				<div class="pad">
					<h3 class="subject">{current.title}</h3>
					<p class="sub">{plural(current.stop_count, 'visit')} · {duration(current.hours)}</p>
					{#if current.recall}<p class="sub">{current.recall}</p>{/if}

					<p class="q">{ask.text}</p>

					{#if ask.picks.length}
						<ul class="checklist">
							{#each ask.picks as c (c.name)}
								<li>
									<button type="button" role="checkbox" aria-checked={picked.includes(c.name)} disabled={saving} onclick={() => togglePick(c.name)}>
										<span class="check" aria-hidden="true">{picked.includes(c.name) ? '✓' : ''}</span>
										<span class="pick-name">{c.name}</span>
										<span class="pick-m">{Math.round(c.meters)} m</span>
									</button>
								</li>
							{/each}
						</ul>
						<button class="btn primary wide" type="button" disabled={!picked.length || saving} onclick={savePicks}>
							{picked.length > 1 ? `Save These ${picked.length}` : 'Save'}
						</button>
					{:else if layout === 'maps'}
						<div class="actions">
							{#each ask.chips as c, i (c.label)}
								<button class="action" class:primary={i === 0 && ask.chips.length <= 2} type="button" disabled={saving} onclick={() => void answer(current.id, c.answer)}>
									<span class="disc"><Icon icon={iconOf(c.answer)} width="20" /></span>
									<span class="action-label">{c.label}</span>
								</button>
							{/each}
						</div>
					{:else}
						<div class="buttons" class:grid={ask.chips.length > 2}>
							{#each ask.chips as c, i (c.label)}
								<button class="btn" class:primary={i === 0 && ask.chips.length <= 2} type="button" disabled={saving} onclick={() => void answer(current.id, c.answer)}>
									{c.label}
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
					{#if current.snippet}<p class="snippet">“{current.snippet}”</p>{/if}
					{#if !current.naming.street_detail}
						<p class="snippet">Your server's map of this area doesn't list its places, so it can't suggest names here.</p>
					{/if}
				</div>
				<div class="foot">
					<span class="nav">
						<IconButton icon="ri:arrow-left-s-line" label="Previous place" size="sm" disabled={at <= 0} onclick={() => page(-1)} />
						<IconButton icon="ri:arrow-right-s-line" label="Next place" size="sm" disabled={at >= questions.length - 1} onclick={() => page(1)} />
					</span>
					{#if current.best_day}
						{@const day = current.best_day}
						<TextAction onclick={() => seeDay(day)}>See That Day</TextAction>
					{/if}
					<TextAction quiet onclick={() => (mode = 'confirmed')}>{confirmed.length} Named</TextAction>
				</div>
			{:else}
				<div class="top">
					<span class="counter"></span>
					<IconButton icon="ri:close-line" label="Close" size="sm" onclick={toggle} />
				</div>
				<div class="pad">
					<h3 class="subject">All set</h3>
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

			{#if DEV}
				<!-- Dev only, never shipped: compare the layouts and placements. -->
				<div class="dev">
					<span>Dev:</span>
					{#each ['photos', 'maps'] as const as l (l)}
						<button type="button" aria-pressed={layout === l} onclick={() => remember('tl-review-layout', (layout = l))}>{l === 'photos' ? 'Photos' : 'Maps'}</button>
					{/each}
					<span class="dev-gap"></span>
					{#each ['below', 'popover'] as const as p (p)}
						<button type="button" aria-pressed={placement === p} onclick={() => remember('tl-review-placement', (placement = p))}>{p === 'below' ? 'Under pill' : 'Popover'}</button>
					{/each}
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

	/* Under the pill, or a popover beside it, its arrow on the pill. */
	.card {
		position: absolute;
		top: calc(100% + 8px);
		left: 0;
		z-index: 13;
		width: 336px;
		max-width: calc(100vw - 32px);
		display: flex;
		flex-direction: column;
		border-radius: var(--tile-radius);
		overflow-y: auto;
		font-family: var(--font-sans);
		color: var(--color-foreground);
	}
	.popover .card {
		top: -8px;
		left: calc(100% + 12px);
	}
	/* The popover's arrow, on the pill, outside the card so its scrolling
	   never clips it. */
	.arrow {
		position: absolute;
		top: 10px;
		left: calc(100% + 6px);
		z-index: 14;
		width: 12px;
		height: 12px;
		background: var(--color-surface);
		border-left: 1px solid var(--color-border);
		border-bottom: 1px solid var(--color-border);
		transform: rotate(45deg);
	}

	.top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		padding: 8px 8px 0 16px;
		min-height: 36px;
	}
	.counter {
		font-size: 12px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.pad {
		padding: 4px 16px 12px;
	}
	.subject {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 22px;
		font-weight: 400;
		line-height: 1.2;
	}
	.sub {
		margin: 4px 0 0;
		font-size: 13px;
		line-height: 1.35;
		color: var(--color-foreground-muted);
	}
	.q {
		margin: 16px 0 12px;
		font-size: 15px;
		font-weight: 500;
		line-height: 1.35;
	}

	/* Apple's alert buttons: equal widths, the likely answer filled. */
	.buttons {
		display: flex;
		gap: 8px;
	}
	.buttons.grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
	}
	.btn {
		flex: 1;
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
		margin: 0;
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
	.pick-m {
		font-size: 12px;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	/* Apple Maps' action row: round buttons, a label under each. */
	.actions {
		display: flex;
		gap: 8px;
	}
	.action {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 8px 4px;
		border: 0;
		border-radius: 12px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-primary);
		font-family: var(--font-sans);
		cursor: pointer;
	}
	.action.primary {
		background: var(--color-primary);
		color: var(--color-background);
	}
	.disc {
		display: inline-flex;
	}
	.action-label {
		font-size: 12px;
		font-weight: 500;
		text-align: center;
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
	.snippet,
	.note {
		margin: 12px 0 0;
		font-size: 12px;
		line-height: 1.4;
		color: var(--color-foreground-subtle);
	}
	.snippet {
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.note {
		color: var(--color-foreground-muted);
	}

	.foot {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 16px 12px 8px;
		border-top: 1px solid var(--color-border);
		font-size: 13px;
	}
	.nav {
		display: inline-flex;
		margin-right: auto;
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

	/* Dev only, never shipped. */
	.dev {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 4px;
		padding: 8px 12px;
		border-top: 1px dashed var(--color-border);
		font-size: 11px;
		color: var(--color-foreground-subtle);
	}
	.dev button {
		padding: 2px 8px;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		background: none;
		color: var(--color-foreground-muted);
		font-size: 11px;
		cursor: pointer;
	}
	.dev button[aria-pressed='true'] {
		color: var(--color-foreground);
		border-color: var(--color-foreground-muted);
	}
	.dev-gap {
		width: 8px;
	}
</style>
