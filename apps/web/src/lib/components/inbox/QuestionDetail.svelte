<script lang="ts">
	/**
	 * One question from your server, with what you need to answer it: the
	 * place on a map, a fact about your time there, and the answers. A tap
	 * on an answer saves it. Where the answer can be a name, one line takes
	 * it.
	 */
	import { Button, Input, TextAction } from '$lib';
	import MovementMap from '$lib/components/timeline/MovementMap.svelte';
	import type { PlaceAnswer, Question } from '$lib/inbox/questions';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	let { question, onAnswer }: { question: Question; onAnswer: (answer: PlaceAnswer) => void } = $props();

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
</script>

<div class="ask">
	<h2 class="text">{question.text}</h2>
	<p class="fact">
		{question.about}. {question.context}.
		{#if place.best_day}<TextAction inline onclick={openDay}>See the day you were there most</TextAction>{/if}
	</p>
	{#key question.key}
		<MovementMap track={spot} stops={spot} height={220} zoom={15} />
	{/key}
	{#if question.replies.length > 0}
		<div class="replies">
			{#each question.replies as r, i (r.label)}
				<Button variant={i === 0 && question.replies.length === 2 ? 'primary' : 'secondary'} onclick={() => onAnswer(r.answer)}>{r.label}</Button>
			{/each}
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
		gap: 8px;
	}
	.typed {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
</style>
