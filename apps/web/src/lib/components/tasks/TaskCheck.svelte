<script lang="ts">
	/**
	 * The round check a to-do wears, wherever it is listed: empty while open,
	 * filled once it is done.
	 */
	let {
		done,
		label,
		onclick,
	}: {
		done: boolean;
		/** What pressing it does, for a screen reader: "Complete Renew passport". */
		label: string;
		onclick: () => void;
	} = $props();
</script>

<button type="button" class="check" class:checked={done} role="checkbox" aria-checked={done} aria-label={label} {onclick}>
	{#if done}
		<svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true"
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
	.check:hover {
		border-color: var(--color-foreground-muted);
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
