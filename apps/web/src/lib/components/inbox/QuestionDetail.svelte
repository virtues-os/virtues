<script lang="ts">
	/**
	 * One question from your server, with what you need to answer it: the
	 * place on a map, a fact about your time there, and the answers. A tap
	 * on an answer saves it. Where the answer can be a name, one line takes
	 * it. The answers all look alike, so the panel does not lean toward one.
	 *
	 * Under the map are your last four weeks there, a column a day: how long
	 * you were there, and a dot under each night you slept there. That is
	 * what your server is going on. "Not now" sets the question aside for a
	 * week.
	 */
	import { Button, Input, TextAction } from '$lib';
	import MovementMap from '$lib/components/timeline/MovementMap.svelte';
	import type { PlaceAnswer, PlaceDay, Question } from '$lib/inbox/questions';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	let {
		question,
		compact = false,
		onAnswer,
	}: {
		question: Question;
		/** Under the question's own row, which already says the question. */
		compact?: boolean;
		onAnswer: (answer: PlaceAnswer) => void;
	} = $props();

	let name = $state('');
	const place = $derived(question.place);
	const spot = $derived([{ lat: place.latitude, lng: place.longitude, label: question.about, timeMs: 0 }]);

	function submit(e: SubmitEvent) {
		e.preventDefault();
		const typed = name.trim();
		if (typed) onAnswer({ kind: 'name', name: typed });
	}
	function openDay() {
		if (place.best_day) windowShellStore.openTabFromRoute(`/day/${place.best_day}`, { focusExisting: true });
	}

	const days = $derived(place.days ?? []);
	/** The day the strip starts, as "Oct 26". */
	const fromLabel = $derived(days.length ? dayLabel(days[0].date, { month: 'short', day: 'numeric' }) : '');
	function dayLabel(date: string, opts: Intl.DateTimeFormatOptions): string {
		return new Date(`${date}T12:00:00`).toLocaleDateString(undefined, opts);
	}
	function dayTip(d: PlaceDay): string {
		const when = dayLabel(d.date, { weekday: 'short', month: 'short', day: 'numeric' });
		const time = d.hours === 0 ? 'Not there' : `${d.hours} ${d.hours === 1 ? 'hour' : 'hours'}`;
		return d.slept ? `${when}: ${time}, slept there` : `${when}: ${time}`;
	}
</script>

<div class="ask">
	{#if !compact}<h2 class="text">{question.text}</h2>{/if}
	<p class="fact">
		{question.about}. {question.context}.
		{#if place.best_day}<TextAction inline onclick={openDay}>See the day you were there most</TextAction>{/if}
	</p>
	{#key question.key}
		<MovementMap track={spot} stops={spot} height={220} zoom={15} />
	{/key}
	{#if days.length > 0}
		<figure class="weeks">
			<div class="cols">
				{#each days as d (d.date)}
					<span class="col" data-tip={dayTip(d)} aria-label={dayTip(d)} role="img">
						<span class="track"><span class="bar" style:height="{Math.min(d.hours, 24) / 24 * 100}%"></span></span>
						<span class="night" class:slept={d.slept}></span>
					</span>
				{/each}
			</div>
			<figcaption>
				<span>{fromLabel}</span>
				<span>Hours there each day. A dot is a night you slept there.</span>
				<span>Today</span>
			</figcaption>
		</figure>
	{/if}
	{#if question.replies.length > 0 || days.length > 0}
		<div class="replies">
			{#each question.replies as r (r.label)}
				<Button variant="secondary" onclick={() => onAnswer(r.answer)}>{r.label}</Button>
			{/each}
			<!-- A server that sends no days predates Not now. -->
			{#if days.length > 0}
				<span class="later"><TextAction quiet onclick={() => onAnswer({ kind: 'later' })}>Not now</TextAction></span>
			{/if}
		</div>
	{/if}
	{#if question.typed}
		<form class="typed" onsubmit={submit}>
			<Input bind:value={name} label={question.typed.label} />
			<span><Button type="submit" variant="secondary" disabled={!name.trim()}>Save</Button></span>
		</form>
	{/if}
</div>

<style>
	.ask {
		display: flex;
		flex-direction: column;
		gap: 16px;
		max-width: 560px;
	}
	.text {
		margin: 0;
		font-family: var(--font-serif);
		font-size: var(--text-heading);
		font-weight: 400;
		line-height: 28px;
		color: var(--color-foreground);
	}
	.fact {
		margin: 0;
		font-size: var(--text-label);
		line-height: 20px;
		color: var(--color-foreground-muted);
	}
	.replies {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 8px;
	}
	.later {
		margin-left: 8px;
	}
	/* Four weeks, a column a day, as wide as the map above. */
	.weeks {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin: 0;
	}
	.cols {
		display: grid;
		grid-template-columns: repeat(28, 1fr);
		gap: 2px;
	}
	.col {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
	}
	.track {
		display: flex;
		align-items: flex-end;
		width: 100%;
		height: 40px;
		border-radius: var(--radius-sm);
		background: var(--fill-bg);
		overflow: hidden;
	}
	.bar {
		width: 100%;
		background: var(--color-primary);
	}
	.night {
		width: 4px;
		height: 4px;
		border-radius: 50%;
	}
	.night.slept {
		background: var(--color-primary);
	}
	figcaption {
		display: flex;
		justify-content: space-between;
		gap: 16px;
		font-size: var(--text-caption);
		line-height: 16px;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.typed {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
</style>
