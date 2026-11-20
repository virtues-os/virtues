<script lang="ts">
	/**
	 * The control that goes from signal to noise. Its value runs from 0,
	 * which shows only what asks the most of you, to 1, which shows
	 * everything. Each thing's place on that line is in lib/inbox/items.
	 */
	let { value, onchange }: { value: number; onchange: (value: number) => void } = $props();
</script>

<span class="signal">
	<span>Signal</span>
	<input
		type="range"
		min="0"
		max="1"
		step="any"
		{value}
		aria-label="How much to show, from signal to noise"
		oninput={(e) => onchange(Number(e.currentTarget.value))}
	/>
	<span>Noise</span>
</span>

<style>
	.signal {
		display: inline-flex;
		flex: 0 1 auto;
		align-items: center;
		gap: 8px;
		min-width: 0;
		height: var(--control-h);
		font-size: var(--text-caption);
		line-height: 16px;
		color: var(--color-foreground-muted);
	}

	/* The line is drawn on the input itself, so it looks the same in every
	   browser. It stops 8px short of each end, where the middle of the 16px
	   thumb comes to rest. */
	input {
		appearance: none;
		flex: 0 1 120px;
		width: 120px;
		min-width: 48px;
		height: 16px;
		margin: 0;
		background: linear-gradient(var(--color-border-strong), var(--color-border-strong)) center / calc(100% - 16px) 2px no-repeat;
		cursor: pointer;
	}
	input::-webkit-slider-runnable-track {
		height: 16px;
		background: transparent;
	}
	input::-moz-range-track {
		height: 16px;
		background: transparent;
	}
	input::-webkit-slider-thumb {
		appearance: none;
		width: 16px;
		height: 16px;
		border: 4px solid var(--color-page);
		border-radius: 50%;
		background: var(--color-foreground);
	}
	input::-moz-range-thumb {
		box-sizing: border-box;
		width: 16px;
		height: 16px;
		border: 4px solid var(--color-page);
		border-radius: 50%;
		background: var(--color-foreground);
	}
</style>
