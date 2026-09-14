<!--
	Where you are in setting up, under the door: the four steps as a short
	vertical list, right-aligned, one type size. Each state answers one
	thing the person is feeling on the walk from the letter to here:

	  done      KEPT. A green check, the app's own glyph for it — what you
	            gave is banked, not faded into the past.
	  current   HERE. Ink, and a little weight. Never green: green means
	            done everywhere else in the product.
	  upcoming  NOT YET. Faded, so it reads as the future and not as
	            something already missed.
	  set aside CHOSEN. Struck through, not faded — a decision, not a gap,
	            and never mistakable for done.

	The checks stack in a column on the right edge and grow as you go; that
	column is the feeling of accumulation. Click a step and the thread
	scrolls to where the room spoke about it.
-->
<script lang="ts">
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	let { onjump }: { onjump?: (stepId: string) => void } = $props();

	const steps = $derived(gettingStarted.steps);
	const currentId = $derived(gettingStarted.firstOpen);
	const currentIndex = $derived(steps.findIndex((s) => s.id === currentId));
</script>

{#if !gettingStarted.graduated && steps.length > 0}
	<ol class="steps" aria-label="Setting up">
		{#each steps as step, i (step.id)}
			{@const done = step.status === "done"}
			{@const skipped = step.status === "skipped"}
			{@const current = step.id === currentId}
			{@const upcoming = !done && !skipped && !current && (currentIndex === -1 || i > currentIndex)}
			<li>
				<button
					type="button"
					class:done
					class:skipped
					class:current
					class:upcoming
					aria-current={current ? "step" : undefined}
					title={skipped ? "Set aside — say the word to come back to it" : undefined}
					onclick={() => onjump?.(step.id)}
				>
					<span class="label">{step.title}</span>
					<span class="mark" aria-hidden="true">{done ? "✓" : ""}</span>
				</button>
			</li>
		{/each}
	</ol>
{/if}

<style>
	.steps {
		list-style: none;
		margin: 0;
		padding: 0 0.4rem 0 0;
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 0.1rem;
		user-select: none;
	}
	button {
		display: flex;
		align-items: baseline;
		gap: 0.35rem;
		font: inherit;
		font-size: 0.75rem;
		line-height: 1.4;
		border: 0;
		background: none;
		padding: 0;
		margin: 0;
		color: var(--color-foreground-subtle);
		cursor: pointer;
		white-space: nowrap;
		transition: color 0.18s ease, opacity 0.18s ease;
	}
	button:hover .label {
		color: var(--color-foreground);
	}
	/* A fixed column for the check, present on every row, so the labels stay
	   aligned against it whether or not a check is there yet. */
	.mark {
		width: 0.85em;
		text-align: left;
		color: var(--color-success);
		font-size: 0.7rem;
	}

	.current .label {
		color: var(--color-foreground);
		font-weight: 500;
	}
	.done .label {
		color: var(--color-foreground-muted);
	}
	.upcoming {
		opacity: 0.55;
	}
	.skipped .label {
		text-decoration: line-through;
		text-decoration-thickness: 1px;
	}
</style>
