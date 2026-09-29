<script lang="ts">
	/**
	 * One section of a project's page: a small, quiet head with a count, then
	 * its contents. Told apart from its neighbours by the head and the air
	 * above it, the sidebar's grammar for a group; no rules.
	 *
	 * `empty` replaces the contents with one plain line when there is nothing
	 * to show and nothing to do about it here.
	 */
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		count?: number;
		empty?: string | null;
		children?: Snippet;
	}

	let { title, count = 0, empty = null, children }: Props = $props();

	const id = `project-section-${Math.random().toString(36).slice(2, 8)}`;
</script>

<section class="section" aria-labelledby={id}>
	<div class="section-head">
		<h2 {id}>{title}</h2>
		{#if count > 0}<span class="section-count">{count}</span>{/if}
	</div>
	{#if empty}
		<p class="section-empty">{empty}</p>
	{:else}
		{@render children?.()}
	{/if}
</section>

<style>
	.section {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.section-head {
		display: flex;
		align-items: baseline;
		gap: 8px;
	}
	/* The sidebar group head's voice, not a heading's: the global h2 is the
	   serif, and these are labels. */
	.section-head h2 {
		margin: 0;
		font-family: inherit;
		letter-spacing: normal;
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
	}
	.section-count {
		font-size: 0.75rem;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
	}
	.section-empty {
		margin: 0;
		font-size: 0.875rem;
		color: var(--color-foreground-subtle);
	}
</style>
