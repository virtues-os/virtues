<!--
	ReviewTile.svelte - "Help Virtues out", the prototype's pill under the date
	card with the count of open questions (dayback/index.html:266-282,
	1177). It goes between days: from the pill, the question with the most
	time first; ‹ › to the one before or after; each opens its day and the
	card on its stay in the day's list, so every question is answered beside
	the stay it's about. With nothing left, "All caught up" opens the names
	Virtues chose without asking.
-->
<script lang="ts">
	import IconButton from '$lib/components/IconButton.svelte';

	let {
		count,
		at,
		onstep,
		onnames,
		onclose,
	}: {
		/** The questions the tile steps through. */
		count: number;
		/** Which of them is open; `null` before the first step. */
		at: number | null;
		/** Open question `i`, on its day. */
		onstep: (i: number) => void;
		/** Open the names Virtues chose. */
		onnames: () => void;
		/** Stop stepping. */
		onclose: () => void;
	} = $props();
</script>

<div class="review-tile tile">
	{#if !count}
		<button class="label" type="button" onclick={onnames}><span class="ic" aria-hidden="true">✓</span>All caught up</button>
	{:else if at === null}
		<button class="label" type="button" onclick={() => onstep(0)}>
			<span class="ic" aria-hidden="true">◎</span>Help Virtues out<span class="badge">{count}</span>
		</button>
	{:else}
		<button class="label" type="button" title="Open this question again" onclick={() => onstep(at)}>
			<span class="ic" aria-hidden="true">◎</span>Help Virtues out
		</button>
		<span class="steps">
			<IconButton icon="ri:arrow-left-s-line" label="Previous question" size="xs" disabled={at <= 0} onclick={() => onstep(at - 1)} />
			<span class="counter">{at + 1} of {count}</span>
			<IconButton icon="ri:arrow-right-s-line" label="Next question" size="xs" disabled={at >= count - 1} onclick={() => onstep(at + 1)} />
			<IconButton icon="ri:close-line" label="Stop" size="xs" onclick={onclose} />
		</span>
	{/if}
</div>

<style>
	/* The Timeline's one material: the surface in a hairline, never a
	   shadow. */
	.tile {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
	}
	.review-tile {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		height: 32px;
		margin-top: 8px;
		padding: 0 4px 0 12px;
		border-radius: 999px;
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	.label {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		height: 100%;
		padding: 0 8px 0 0;
		border: 0;
		background: none;
		color: inherit;
		font: inherit;
		cursor: pointer;
	}
	.label:hover {
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
		font-size: 11px;
		font-weight: 600;
		line-height: 16px;
		text-align: center;
	}
	.steps {
		display: inline-flex;
		align-items: center;
		gap: 4px;
	}
	.counter {
		padding: 0 4px;
		font-size: 12px;
		font-weight: 400;
		font-variant-numeric: tabular-nums;
	}
</style>
