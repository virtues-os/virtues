<!--
	The heading over a beat of the room: "1 of 4 · Connect AI", as an h2.

	Progress lives in the thread, on the reading axis, the way a
	conversational form numbers each question — and as a section heading
	rather than a caption, so the room has the hierarchy of an authored
	page: one title, four sections, each with its ask, the answer, and the
	settled line under it. The line the room asks a step with carries it;
	a step settled before it was ever asked has its settling line carry it
	instead. The last heading is where you are.

	Serif, like the h1 and the markdown's own h2, but a size under it: four
	of these in a chat at document size would shout. Weight 400 — the
	serif is never bold. The count is the same serif, quiet; the title is
	ink while the step is being asked and quiet once it has settled.
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
	<h2 class="step" class:current class:skipped={step.status === "skipped"}>
		<span class="n">{index + 1} of {steps.length}</span>
		<span class="sep" aria-hidden="true">·</span>
		<span class="t">{step.title}</span>
	</h2>
{/if}

<style>
	.step {
		display: flex;
		align-items: baseline;
		gap: 0.5rem;
		/* A section break wants more air above it than the room's 1rem line
		   rhythm; the container gap supplies 1rem and this adds the rest. As
		   PADDING: the chat zeroes margin-top on every heading inside a
		   message (`.message-wrapper :global(h2)`), and out-specifies this. */
		margin: 0 0 0.5rem;
		padding-top: 0.75rem;
		font-family: var(--md-heading-major-family, var(--font-serif));
		/* The markdown's own h2 token, so the room's two serif headings come
		   off one scale — 1.25rem was a number I invented, and it sat a step
		   below the scale rather than on it. */
		font-size: var(--md-h2-size, 1.375rem);
		font-weight: 400;
		line-height: 1.3;
		font-variant-numeric: tabular-nums;
		/* One color for the whole line. The count used to sit a shade lighter
		   than the title beside it, which read as two things sharing a line
		   rather than one heading. */
		color: var(--color-foreground-muted);
	}
	.step.current {
		color: var(--color-foreground);
	}
	.step.skipped .t {
		text-decoration: line-through;
		text-decoration-thickness: 1px;
	}
	.sep {
		opacity: 0.45;
	}
</style>
