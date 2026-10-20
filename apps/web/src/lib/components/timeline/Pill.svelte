<!--
	Pill.svelte - the Timeline's button: Virtues' Button "secondary" shape
	(Button.svelte: a pill, the surface in a hairline, the foreground's text)
	at the list's own sizes, with the likely answer and a save filled in the
	Timeline's blue, as the prototype's card fills them (the owner's call,
	2026-10-08: "hybrid" over Button's claret primary, which would be the only
	claret on a map of blue stays, orange drives and red events). `quiet` is
	Button's "ghost": text only, for Cancel and the way back. No underlined
	word is a button in the Timeline.
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		filled = false,
		quiet = false,
		size = 'row',
		wide = false,
		disabled = false,
		type = 'button',
		title,
		label,
		onclick,
		children,
	}: {
		/** The likely answer, or a save. */
		filled?: boolean;
		/** Cancel, the way back: text only. */
		quiet?: boolean;
		/** `row`: on a line of the day's list (28 px); `card`: in the card (32 px). */
		size?: 'row' | 'card';
		wide?: boolean;
		disabled?: boolean;
		type?: 'button' | 'submit';
		title?: string;
		label?: string;
		onclick?: (e: MouseEvent) => void;
		children: Snippet;
	} = $props();
</script>

<button class="pill" data-size={size} class:filled class:quiet class:wide {type} {title} aria-label={label} {disabled} {onclick}>
	{@render children()}
</button>

<style>
	.pill {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		height: 28px;
		padding: 0 12px;
		border: 1px solid var(--color-border);
		border-radius: 999px;
		background: var(--color-surface);
		color: var(--color-foreground);
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 500;
		white-space: nowrap;
		cursor: pointer;
		transition:
			background-color 120ms ease,
			border-color 120ms ease,
			color 120ms ease;
	}
	.pill[data-size='card'] {
		height: 32px;
		padding: 0 16px;
	}
	.pill:hover:not(:disabled) {
		background: var(--hover-bg);
		border-color: var(--color-border-strong);
	}
	.pill.filled {
		border-color: transparent;
		background: var(--color-primary);
		color: var(--color-background);
	}
	.pill.filled:hover:not(:disabled) {
		border-color: transparent;
		background: var(--color-primary-hover);
	}
	.pill.quiet {
		border-color: transparent;
		background: transparent;
		color: var(--color-foreground-muted);
	}
	.pill.quiet:hover:not(:disabled) {
		border-color: transparent;
		background: var(--hover-bg);
		color: var(--color-foreground);
	}
	.pill.wide {
		width: 100%;
	}
	.pill:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.pill:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 2px;
	}
	@media (prefers-reduced-motion: reduce) {
		.pill {
			transition: none;
		}
	}
</style>
