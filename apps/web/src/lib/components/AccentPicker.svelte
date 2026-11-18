<script lang="ts">
	/**
	 * The accent swatches (design.md, "Accent"). Each swatch carries its own
	 * `data-accent`, so themes.css hands it that swatch's color in the theme
	 * on screen and no value is copied here. The chosen one wears a ring in
	 * its own color; the names are in the tooltips and said by each radio.
	 */
	import { ACCENTS, setAccent, type Accent } from '$lib/utils/theme';

	let { value, onchange }: { value: Accent; onchange?: (accent: Accent) => void } = $props();

	async function select(accent: Accent) {
		await setAccent(accent);
		onchange?.(accent);
	}
</script>

<div class="swatches" role="radiogroup" aria-label="Accent">
	{#each ACCENTS as accent (accent.id)}
		<button
			type="button"
			class="swatch"
			class:selected={value === accent.id}
			role="radio"
			aria-checked={value === accent.id}
			aria-label={accent.name}
			data-tip={accent.name}
			data-accent={accent.id}
			onclick={() => select(accent.id)}
		>
			<span class="dot"></span>
		</button>
	{/each}
</div>

<style>
	.swatches {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.swatch {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: var(--control-h);
		height: var(--control-h);
		padding: 0;
		border: none;
		border-radius: 50%;
		background: transparent;
		cursor: pointer;
	}

	.dot {
		width: 24px;
		height: 24px;
		border-radius: 50%;
		background: var(--accent-light);
		transition: transform var(--motion-release) var(--ease-premium);
	}

	:global([data-theme="asgard"]) .dot {
		background: var(--accent-dark);
	}

	.swatch:active .dot {
		transform: scale(0.94);
		transition-duration: var(--motion-press);
	}

	/* The chosen swatch: a ring in its own color, a step out from the dot. */
	.swatch.selected .dot {
		outline: 2px solid var(--accent-light);
		outline-offset: 3px;
	}

	:global([data-theme="asgard"]) .swatch.selected .dot {
		outline-color: var(--accent-dark);
	}

	@media (prefers-reduced-motion: reduce) {
		.dot {
			transition: none;
		}
	}
</style>
