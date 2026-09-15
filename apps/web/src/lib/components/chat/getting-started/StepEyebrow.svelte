<!--
	The number over a beat of the room: "2 of 4 · Introductions".

	This is where progress lives now — in the thread, on the reading axis,
	the way a conversational form numbers each question. The line the room
	asks a step with carries it; if a step was settled before it was ever
	asked, the settling line carries it instead. Scrolling the thread IS
	reading the progress, and the last eyebrow is where you are.

	Ink while the step is the one being asked; quiet once it has settled.
	No check, no glyph: the number and the weight of the ink are the state.
-->
<script lang="ts">
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	let { stepId }: { stepId: string } = $props();

	const steps = $derived(gettingStarted.steps);
	const index = $derived(steps.findIndex((s) => s.id === stepId));
	const step = $derived(index >= 0 ? steps[index] : null);
	const current = $derived(gettingStarted.firstOpen === stepId);
</script>

{#if step}
	<p class="eyebrow" class:current class:skipped={step.status === "skipped"}>
		<span class="n">{index + 1} of {steps.length}</span>
		<span class="sep" aria-hidden="true">·</span>
		<span class="t">{step.title}</span>
	</p>
{/if}

<style>
	.eyebrow {
		display: flex;
		align-items: baseline;
		gap: 0.4rem;
		margin: 0 0 0.35rem;
		font-size: 0.8125rem;
		line-height: 1.4;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
		user-select: none;
	}
	.eyebrow.current {
		color: var(--color-foreground);
	}
	.eyebrow.current .t {
		font-weight: 500;
	}
	.eyebrow.skipped .t {
		text-decoration: line-through;
		text-decoration-thickness: 1px;
	}
	.sep {
		opacity: 0.6;
	}
</style>
