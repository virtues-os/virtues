<!--
	Where you are in setting up, under the door: the four steps as a short
	vertical list, right-aligned, the one you are on in ink and the rest
	quiet. Vertical, not a rail — a horizontal bar named one step at a time
	and had to be hovered to say the others, and a list of four short titles
	says all of it at a glance.

	Small and tight, because the earlier list in this corner was not: it sat
	at the door's label size with a loose leading and the corner read as
	prose. The door is icon-only now and this is the only type here, at one
	size. No count, no marks, no rules — the weight of the ink is the state.

	Click a step and the thread scrolls to where the room spoke about it.
-->
<script lang="ts">
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	let { onjump }: { onjump?: (stepId: string) => void } = $props();

	const steps = $derived(gettingStarted.steps);
	const currentId = $derived(gettingStarted.firstOpen);
</script>

{#if !gettingStarted.graduated && steps.length > 0}
	<ol class="steps" aria-label="Setting up">
		{#each steps as step (step.id)}
			<li>
				<button
					type="button"
					class:current={step.id === currentId}
					class:skipped={step.status === "skipped"}
					aria-current={step.id === currentId ? "step" : undefined}
					title={step.status === "skipped" ? "Set aside — say the word to come back to it" : undefined}
					onclick={() => onjump?.(step.id)}
				>
					{step.title}
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
		transition: color 0.18s ease;
	}
	button:hover {
		color: var(--color-foreground);
	}
	/* The one thing being asked of you, and the only line in ink. */
	button.current {
		color: var(--color-foreground);
	}
	/* Set aside is not done, and must not read as done. */
	button.skipped {
		text-decoration: line-through;
		text-decoration-thickness: 1px;
		opacity: 0.75;
	}
</style>
