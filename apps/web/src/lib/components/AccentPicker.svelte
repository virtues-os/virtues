<script lang="ts">
	/**
	 * The accent swatches (design.md, "Accent"), then one for a color you
	 * choose. Each swatch carries its own `data-accent`, so themes.css hands
	 * it that swatch's color in the theme on screen and no value is copied
	 * here. The last one opens the system color picker: the app takes the
	 * color live as you move through it and saves it when the picker
	 * closes, fitted to both themes (tuneAccent). It shows a spectrum until
	 * you have chosen one, then your color. The chosen swatch wears a ring
	 * in its own color; the names are in the tooltips and said by each radio.
	 */
	import { ACCENTS, applyAccent, isCustomAccent, setAccent, type Accent } from '$lib/utils/theme';

	let { value, onchange }: { value: Accent; onchange?: (accent: Accent) => void } = $props();

	const custom = $derived(isCustomAccent(value));
	let input: HTMLInputElement | undefined = $state();

	async function select(accent: Accent) {
		await setAccent(accent);
		onchange?.(accent);
	}

	/** Open the picker on the color you have now. */
	function seed() {
		if (!input) return;
		const now = getComputedStyle(document.documentElement).getPropertyValue('--accent-light').trim();
		if (custom) input.value = value;
		else if (now) input.value = now;
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
	<label class="swatch" class:selected={custom} data-tip="Choose your own" onpointerdown={seed}>
		<span class="dot" class:spectrum={!custom}></span>
		<input
			bind:this={input}
			type="color"
			role="radio"
			aria-checked={custom}
			aria-label="Choose your own accent"
			oninput={(e) => applyAccent(e.currentTarget.value as Accent)}
			onchange={(e) => select(e.currentTarget.value as Accent)}
		/>
	</label>
</div>

<style>
	.swatches {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.swatch {
		position: relative;
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

	/* Not chosen yet: the whole wheel, so it reads as "any color". */
	.dot.spectrum,
	:global([data-theme="asgard"]) .dot.spectrum {
		background: var(--accent-spectrum);
	}

	/* The system picker opens from the whole swatch; the field itself is
	   not drawn. */
	input[type="color"] {
		position: absolute;
		inset: 0;
		width: 100%;
		height: 100%;
		opacity: 0;
		cursor: pointer;
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
