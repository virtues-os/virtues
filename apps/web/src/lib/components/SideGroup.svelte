<script lang="ts">
	/**
	 * One named group of properties in a thing's side column (PaneColumns):
	 * a small heading, then rows of an icon and a value (PreviewRow). A row
	 * that opens a picker can sit in a Popover; its trigger fills the row.
	 */
	import type { Snippet } from 'svelte';

	let { label, children }: { label: string; children: Snippet } = $props();
</script>

<section class="group" aria-label={label}>
	<h3 class="label">{label}</h3>
	<div class="rows">{@render children()}</div>
</section>

<style>
	.group {
		margin-top: 24px;
	}
	.group:first-child {
		margin-top: 0;
	}
	.label {
		margin: 8px 0 4px;
		font-family: var(--font-sans);
		font-size: var(--text-caption);
		font-weight: 400;
		line-height: 16px;
		color: var(--color-foreground-muted);
	}
	.rows {
		display: flex;
		flex-direction: column;
	}
	.rows :global(.popover-trigger) {
		display: block;
	}
</style>
