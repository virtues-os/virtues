<!--
	Where you are in setting up, under the door.

	A segmented rail with the step you are on named above it — the shape
	every product uses for "part way through something short", which is
	exactly what this is. Four earlier attempts failed for the same reason
	in different clothes: a checklist in the column competed with the
	conversation saying the same thing, a check in the prose margin was an
	artifact glued to the page, and four stacked labels (and then four
	stacked rules) floated in the corner's whitespace with nothing holding
	them. This is one small block, ~10rem, with one line of words.

	It names ONE step, so there is no list to read. Hover a segment and the
	line names that one instead; click it and the thread scrolls to where
	the room spoke about it. So it is a contents as well as a gauge, without
	ever drawing a contents.
-->
<script lang="ts">
	import { gettingStarted } from "$lib/stores/gettingStarted.svelte";

	let { onjump }: { onjump?: (stepId: string) => void } = $props();

	const steps = $derived(gettingStarted.steps);
	const currentId = $derived(gettingStarted.firstOpen);
	const settled = $derived(steps.filter((s) => s.status !== "open").length);

	let hoveredId = $state<string | null>(null);
	const named = $derived(steps.find((s) => s.id === (hoveredId ?? currentId)) ?? steps[0]);
</script>

{#if !gettingStarted.graduated && steps.length > 0}
	<div class="progress" aria-label="Setting up">
		<p class="named">
			<span class="title">{named?.title ?? ""}</span>
			<span class="count">{settled} of {steps.length}</span>
		</p>
		<div class="rail">
			{#each steps as step (step.id)}
				<button
					type="button"
					class="seg"
					class:done={step.status === "done"}
					class:skipped={step.status === "skipped"}
					class:current={step.id === currentId}
					aria-current={step.id === currentId ? "step" : undefined}
					aria-label={step.title}
					onmouseenter={() => (hoveredId = step.id)}
					onmouseleave={() => (hoveredId = null)}
					onfocus={() => (hoveredId = step.id)}
					onblur={() => (hoveredId = null)}
					onclick={() => onjump?.(step.id)}
				></button>
			{/each}
		</div>
	</div>
{/if}

<style>
	.progress {
		width: 10rem;
		user-select: none;
	}
	.named {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 0.5rem;
		margin: 0 0 0.3rem;
		font-size: 0.75rem;
		line-height: 1.3;
		white-space: nowrap;
	}
	.title {
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.count {
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
		flex: none;
	}

	.rail {
		display: flex;
		gap: 3px;
	}
	.seg {
		flex: 1;
		height: 2px;
		padding: 0;
		border: 0;
		border-radius: 1px;
		cursor: pointer;
		background: color-mix(in srgb, var(--color-foreground) 12%, transparent);
		transition: background 0.18s ease;
		/* The bar is 2px; the reach is a finger's worth either side of it. */
		position: relative;
	}
	.seg::after {
		content: "";
		position: absolute;
		inset: -7px -1px;
	}
	.seg.done {
		background: color-mix(in srgb, var(--color-foreground) 42%, transparent);
	}
	/* Set aside is not handled: broken, not filled. */
	.seg.skipped {
		background: repeating-linear-gradient(
			to right,
			color-mix(in srgb, var(--color-foreground) 38%, transparent) 0 3px,
			transparent 3px 6px
		);
	}
	.seg.current {
		background: var(--color-foreground);
	}
	.seg:hover {
		background: color-mix(in srgb, var(--color-foreground) 70%, transparent);
	}
</style>
