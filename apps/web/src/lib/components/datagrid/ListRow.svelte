<!--
	ListRow.svelte

	The one row anatomy for the data grid's List view: a 16px glyph, the name
	in serif, one line under it, and a short note on the right (a date, "Off",
	"You"). A view renders it from its `listRow` snippet and passes only its
	content, so Pages, Applets and the wiki index stay alike.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';

	let {
		icon,
		lead,
		title,
		line = null,
		problem = false,
		note = null,
	}: {
		/** An icon name for the glyph. */
		icon?: string;
		/** A glyph the icon name cannot express, drawn in place of `icon`. */
		lead?: Snippet;
		title: string;
		/** The line under the name; nothing is drawn without one. */
		line?: string | null;
		/** The line says what went wrong, so it takes the error color. */
		problem?: boolean;
		note?: string | null;
	} = $props();
</script>

<span class="row">
	<span class="row-icon" aria-hidden="true">
		{#if lead}{@render lead()}{:else if icon}<Icon {icon} width="16" />{/if}
	</span>
	<span class="row-text">
		<span class="row-title">{title}</span>
		{#if line}
			<span class="row-line" class:problem>{line}</span>
		{/if}
	</span>
	{#if note}
		<span class="row-note">{note}</span>
	{/if}
</span>

<style>
	.row {
		display: flex;
		align-items: flex-start;
		gap: 0.875rem;
	}
	.row-icon {
		flex: 0 0 16px;
		margin-top: 0.25rem;
		color: var(--color-foreground-muted);
	}
	.row-text {
		flex: 1 1 auto;
		min-width: 0;
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}
	.row-title,
	.row-line {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.row-title {
		font-family: var(--font-serif);
		font-size: 1.125rem;
		line-height: 1.3;
		color: var(--color-foreground);
	}
	:global(.list-row:hover) .row-title {
		color: var(--color-foreground-muted);
	}
	.row-line {
		font-family: var(--font-serif);
		font-size: 1rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}
	.row-line.problem {
		color: var(--color-error);
	}
	.row-note {
		flex: 0 0 auto;
		margin-top: 0.25rem;
		font-size: 0.8125rem;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-muted);
	}
</style>
