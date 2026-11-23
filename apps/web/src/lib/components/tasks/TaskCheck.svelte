<script lang="ts">
	/**
	 * The round check a to-do wears, wherever it is listed: empty while open,
	 * filled once it is done.
	 *
	 * `glyph` draws it as a sidebar icon is drawn, a 16px ring on the
	 * sidebar's 1.25px line, for a row whose other marks are icons (the
	 * Inbox). It still takes clicks across the 18px it used to fill.
	 */
	let {
		done,
		label,
		glyph = false,
		onclick,
	}: {
		done: boolean;
		/** What pressing it does, for a screen reader: "Complete Renew passport". */
		label: string;
		glyph?: boolean;
		onclick: () => void;
	} = $props();
</script>

<button type="button" class="check" class:checked={done} class:glyph role="checkbox" aria-checked={done} aria-label={label} {onclick}>
	{#if done}
		<svg viewBox="0 0 16 16" width={glyph ? 10 : 12} height={glyph ? 10 : 12} aria-hidden="true"
			><path
				d="M4 8.3 6.7 11 12 5.2"
				fill="none"
				stroke="currentColor"
				stroke-width="1.8"
				stroke-linecap="round"
				stroke-linejoin="round"
			/></svg
		>
	{/if}
</button>

<style>
	.check {
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		border: 1.5px solid var(--color-foreground-subtle);
		background: transparent;
		color: var(--color-background);
		cursor: pointer;
		transition:
			background-color 120ms ease,
			border-color 120ms ease;
	}
	.check.glyph {
		position: relative;
		width: 16px;
		height: 16px;
		margin: 1px;
		border-width: 1.25px;
		border-color: var(--color-foreground-muted);
	}
	/* The ring is drawn smaller than the box you can press. */
	.check.glyph::after {
		position: absolute;
		inset: -2px;
		content: '';
	}
	.check:hover {
		border-color: var(--color-foreground-muted);
	}
	.check.glyph:hover {
		border-color: var(--color-foreground);
	}
	.check.checked {
		background: var(--color-foreground-muted);
		border-color: var(--color-foreground-muted);
	}
	@media (prefers-reduced-motion: reduce) {
		.check {
			transition: none;
		}
	}
</style>
