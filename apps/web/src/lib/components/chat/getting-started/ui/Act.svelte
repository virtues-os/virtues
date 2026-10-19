<!--
	The room's one button. Two weights — the thing to do, and everything
	else — and a single shape, size and rhythm, so a control drawn in one
	corner of the setup cannot look like a different product from the next.
	Four components used to define this independently, in two radii and five
	paddings (2026-09-14).
-->
<script lang="ts">
	import type { Snippet } from "svelte";

	interface Props {
		variant?: "primary" | "quiet" | "plain";
		type?: "button" | "submit";
		disabled?: boolean;
		title?: string;
		onclick?: () => void;
		children: Snippet;
	}
	let { variant = "quiet", type = "button", disabled = false, title, onclick, children }: Props = $props();
</script>

<button {type} class="act {variant}" {disabled} {title} {onclick}>
	{@render children()}
</button>

<style>
	.act {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font: inherit;
		font-size: 0.875rem;
		line-height: 1.2;
		padding: 0.45rem 0.95rem;
		border-radius: 999px;
		border: 1px solid var(--color-border);
		background: transparent;
		color: var(--color-foreground);
		cursor: pointer;
		white-space: nowrap;
		transition:
			border-color 0.18s ease,
			background-color 0.18s ease,
			color 0.18s ease;
	}
	.act:hover:not(:disabled) {
		border-color: var(--color-foreground);
	}
	.act:disabled {
		opacity: 0.35;
		cursor: default;
	}

	.primary {
		border-color: var(--color-foreground);
		background: var(--color-foreground);
		color: var(--color-background);
	}
	.primary:hover:not(:disabled) {
		opacity: 0.88;
	}

	/* No chrome at all: the way back, the second thought. */
	.plain {
		border-color: transparent;
		padding-left: 0.25rem;
		padding-right: 0.25rem;
		color: var(--color-foreground-muted);
	}
	.plain:hover:not(:disabled) {
		border-color: transparent;
		color: var(--color-foreground);
	}
</style>
