<!--
	DayHand.svelte

	One of your notes in a day page's margin, in your hand. Handwriting on the
	page is only ever yours: what the record computed is typeset, and what you
	wrote beside it is not. A note written just now draws itself once, at the
	pace of a pen, then holds still.

	Removing a note waits a few seconds before it happens, with an Undo, because
	a removed note doesn't come back to the page.
-->
<script lang="ts">
	interface Props {
		text: string;
		/** When you wrote it, e.g. "Oct 6". */
		written: string;
		/** The words it was about, when they're no longer on the page. */
		lost?: string | null;
		/** The sentence it sits beside, for screen readers (the bracket only shows it). */
		about?: string | null;
		drawing?: boolean;
		/** Removed, and waiting out its few seconds of Undo. */
		removing?: boolean;
		/** The last removal failed. */
		failed?: boolean;
		onremove?: () => void;
		onundo?: () => void;
	}

	let { text, written, lost = null, about = null, drawing = false, removing = false, failed = false, onremove, onundo }: Props = $props();

	const short = $derived(text.length > 40 ? `${text.slice(0, 40)}…` : text);
</script>

<div class="hand" class:removing>
	{#if lost}<span class="about">On “{lost}”</span>{:else if about}<span class="sr-only">On “{about}”</span>{/if}
	<span class="words" class:drawing>{text}</span>
	<span class="meta">
		{#if removing}
			Removed<span aria-hidden="true">{" · "}</span><button type="button" class="act" onclick={onundo}>Undo</button>
		{:else}
			{written}{#if onremove}<span aria-hidden="true">{" · "}</span><button type="button" class="act" aria-label="Remove note: {short}" onclick={onremove}>Remove note</button>{/if}
		{/if}
	</span>
	{#if failed}<span class="err" role="status">Your server couldn't remove that note. Try again.</span>{/if}
</div>

<style>
	.hand {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		color: var(--color-foreground-muted);
	}

	.about,
	.err {
		font-family: var(--font-sans);
		font-size: 0.75rem;
		line-height: 1.35;
		color: var(--color-foreground-subtle);
	}

	.words {
		font-family: var(--font-hand);
		font-size: 1.125rem;
		line-height: 1.3;
		transform: rotate(-2.5deg);
		transform-origin: left top;
		overflow-wrap: anywhere;
		transition: opacity 0.2s ease;
	}

	.removing .words {
		opacity: 0.35;
		text-decoration: line-through;
	}

	.words.drawing {
		animation: write 0.85s cubic-bezier(0.3, 0.7, 0.4, 1) both;
	}

	@keyframes write {
		from {
			clip-path: inset(-20% 100% -20% 0);
		}
		to {
			clip-path: inset(-20% -5% -20% 0);
		}
	}

	.meta {
		font-family: var(--font-sans);
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.act {
		font: inherit;
		color: inherit;
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.act:hover {
		color: var(--color-foreground);
	}

	@media (prefers-reduced-motion: reduce) {
		.words,
		.words.drawing {
			animation: none;
			transition: none;
		}
	}
</style>
