<!--
	ReviewAsk.svelte - one "Help Virtues out" question and its answers
	(dayback/src/main.js:2156-2182): compact under its stay in the day's list,
	or full size on the Review screen. Which of several close places (a
	checklist, any number saved together); whether it was a named place just
	out of reach, or the hotel beside a night; whose home; what it was; and
	"Is this home?" / "Is this where you work?", where "No" hands over to
	pointing on the map. Other and Just passing through on every place
	question.
-->
<script lang="ts">
	import Icon from '$lib/components/Icon.svelte';
	import TextAction from '$lib/components/TextAction.svelte';
	import { listPeople } from '$lib/wiki/api';
	import { askFor, homeOf, pickAnswer, weekly, type Answer, type PlaceQuestion } from '$lib/timeline/review';
	import type { ReviewState } from '$lib/timeline/reviewState.svelte';

	let {
		q,
		review,
		compact = false,
	}: {
		q: PlaceQuestion;
		review: ReviewState;
		/** Under a stay in the day's list: small type, no notes. */
		compact?: boolean;
	} = $props();

	const ask = $derived(askFor(q));
	let picked = $state<string[]>([]);
	let otherOpen = $state(false);
	let otherText = $state('');
	let otherInput = $state<HTMLInputElement | null>(null);
	let people = $state<string[] | null>(null);
	const suggestions = $derived([...new Set(ask.other === 'person' ? (people ?? []) : q.naming.nearby)]);
	const sub = $derived(ask.role ? weekly(q) : q.recall);

	// Another question in this spot: start it clean.
	$effect(() => {
		void q.id;
		picked = [];
		otherOpen = false;
		otherText = '';
	});

	// "Whose home" suggests the people Virtues knows, fetched the first time.
	$effect(() => {
		if (ask.other !== 'person' || people) return;
		listPeople()
			.then((ps) => (people = ps.map((p) => p.name).filter(Boolean)))
			.catch((e) => {
				console.warn('[Timeline] people for "whose place" unavailable', e);
				people = [];
			});
	});

	/** A button's answer: "No" to Home or Work points to the map instead. */
	function choose(a: Answer) {
		if (ask.role && (a.kind === 'not_home' || a.kind === 'not_work')) review.startPointing(q.id, ask.role);
		else void review.answer(q.id, a);
	}

	function togglePick(name: string) {
		picked = picked.includes(name) ? picked.filter((n) => n !== name) : [...picked, name];
	}

	function savePicks() {
		const chosen = ask.picks.filter((c) => picked.includes(c.name));
		if (chosen.length) void review.answer(q.id, pickAnswer(chosen));
	}

	function submitOther() {
		const text = otherText.trim();
		if (text) void review.answer(q.id, { kind: 'name', name: ask.other === 'person' ? homeOf(text) : text });
	}

	async function openOther() {
		otherOpen = true;
		await Promise.resolve();
		otherInput?.focus();
	}

	/** Yes and No carry a check and a cross. */
	const iconOf = (a: Answer) =>
		a.kind === 'home' || a.kind === 'work' ? 'ri:check-line' : a.kind === 'not_home' || a.kind === 'not_work' ? 'ri:close-line' : null;
</script>

<div class="ask" class:compact>
	<p class="q">{ask.text}</p>
	{#if sub}<p class="sub">{sub}</p>{/if}

	{#if ask.picks.length}
		<ul class="checklist">
			{#each ask.picks as c (c.name)}
				<li>
					<button type="button" role="checkbox" aria-checked={picked.includes(c.name)} disabled={review.saving} onclick={() => togglePick(c.name)}>
						<span class="check" aria-hidden="true">{picked.includes(c.name) ? '✓' : ''}</span>
						<span class="pick-name">{c.name}</span>
					</button>
				</li>
			{/each}
		</ul>
		<button class="btn primary wide" type="button" disabled={!picked.length || review.saving} onclick={savePicks}>
			{picked.length > 1 ? `Save These ${picked.length}` : 'Save'}
		</button>
	{:else}
		<div class="buttons" class:grid={ask.chips.length > 2}>
			{#each ask.chips as c, i (c.label)}
				{@const icon = iconOf(c.answer)}
				<button class="btn" class:primary={i === 0 && ask.chips.length <= 2} type="button" disabled={review.saving} onclick={() => choose(c.answer)}>
					{#if icon}<Icon {icon} width={compact ? '14' : '16'} />{/if}{c.label}
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
					list="tl-ask-suggest-{q.id}"
					placeholder={ask.other === 'person' ? 'Whose place?' : 'Type its name'}
					aria-label={ask.other === 'person' ? 'Whose place?' : 'What was it?'}
					onkeydown={(e) => e.key === 'Enter' && submitOther()}
				/>
				<button class="btn primary" type="button" disabled={!otherText.trim() || review.saving} onclick={submitOther}>Save</button>
			</div>
			<datalist id="tl-ask-suggest-{q.id}">
				{#each suggestions as s (s)}<option value={s}></option>{/each}
			</datalist>
		{/if}
		<div class="quiet">
			{#if !otherOpen}<TextAction onclick={() => void openOther()}>Other…</TextAction>{/if}
			<TextAction quiet disabled={review.saving} onclick={() => void review.answer(q.id, { kind: 'passing' })}>Just Passing Through</TextAction>
		</div>
	{/if}

	{#if review.saveFailed === q.id}<p class="note">Error: your server couldn't save that. Try again.</p>{/if}
	{#if !compact && !q.naming.street_detail && ask.other}
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
	.sub {
		margin: 4px 0 0;
		font-size: 13px;
		line-height: 1.35;
		color: var(--color-foreground-muted);
	}
	/* Under a stay: a question in the list's own type, on the tint Apple uses
	   for a suggestion inside a list. */
	.compact {
		padding: 8px 12px 12px;
		border-radius: 12px;
		background: color-mix(in srgb, var(--color-primary) 6%, transparent);
	}
	.compact .q {
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
	}
	.compact .sub {
		margin-top: 2px;
		font-size: 12px;
	}

	/* Apple's alert buttons: equal widths, the likely answer filled. */
	.buttons {
		display: flex;
		gap: 8px;
		margin-top: 16px;
	}
	.compact .buttons {
		margin-top: 8px;
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
	.compact .btn {
		min-height: 28px;
		padding: 4px 12px;
		border-radius: 6px;
		background: var(--color-surface);
		font-size: 13px;
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
	.compact .btn.wide {
		margin-top: 8px;
	}

	/* Apple's settings list: a checkmark on each row picked. */
	.checklist {
		margin: 12px 0 0;
		padding: 0;
		list-style: none;
		border-top: 1px solid var(--color-border);
	}
	.compact .checklist {
		margin-top: 8px;
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
	.compact .checklist button {
		min-height: 32px;
		padding: 4px 0;
		font-size: 13px;
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
	.compact .field {
		padding: 4px 8px;
		border-radius: 6px;
		font-size: 13px;
	}
	.quiet {
		display: flex;
		gap: 16px;
		margin-top: 12px;
		font-size: 13px;
	}
	.compact .quiet {
		margin-top: 8px;
		font-size: 12px;
	}
	.note {
		margin: 12px 0 0;
		font-size: 12px;
		line-height: 1.4;
		color: var(--color-foreground-muted);
	}
</style>
