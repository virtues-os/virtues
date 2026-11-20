<script lang="ts">
	/**
	 * A row that opens to show more: a chevron, a label, and one line saying
	 * what is inside. Settings folds what most people never need on a visit,
	 * such as the command that adds a drive, or links that no longer open.
	 */
	import type { Snippet } from 'svelte';

	let {
		label,
		sub,
		children,
	}: {
		label: string;
		/** One line under the label saying what opening it shows. */
		sub?: string;
		children: Snippet;
	} = $props();

	let open = $state(false);
	const id = $props.id();
</script>

<div class="fold">
	<button type="button" class="head" aria-expanded={open} aria-controls={id} onclick={() => (open = !open)}>
		<svg class="chevron" class:open viewBox="0 0 24 24" aria-hidden="true" width="15" height="15">
			<path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="1.8" d="m9 18l6-6l-6-6" />
		</svg>
		<span class="words">
			<span class="label">{label}</span>
			{#if sub}<span class="sub">{sub}</span>{/if}
		</span>
	</button>
	{#if open}
		<div class="body" {id}>{@render children()}</div>
	{/if}
</div>

<style>
	.head {
		display: grid;
		grid-template-columns: 16px minmax(0, 1fr);
		align-items: start;
		gap: 8px;
		width: 100%;
		min-height: 52px;
		padding: 8px 0;
		border: none;
		background: none;
		font: inherit;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
	}

	/* Level with the label's first line, however far the label wraps. */
	.chevron {
		margin-top: 3px;
		color: var(--color-foreground-muted);
		transition: transform var(--motion-state) var(--ease-premium);
	}

	.chevron.open {
		transform: rotate(90deg);
	}

	.label {
		display: block;
		font-size: 15px;
		line-height: 1.4;
	}

	.sub {
		display: block;
		margin-top: 4px;
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.body {
		padding: 4px 0 12px 24px;
	}

	@media (prefers-reduced-motion: reduce) {
		.chevron {
			transition: none;
		}
	}
</style>
