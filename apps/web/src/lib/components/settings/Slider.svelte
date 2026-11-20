<script lang="ts">
	/**
	 * A percent slider for a setting row, with the value beside it. The line
	 * is drawn the same way as the Inbox's signal slider (inbox/SignalSlider).
	 */
	let {
		value,
		label,
		min = 0,
		max = 100,
		step = 5,
		onchange,
	}: {
		value: number;
		label: string;
		min?: number;
		max?: number;
		step?: number;
		onchange: (value: number) => void;
	} = $props();
</script>

<span class="slider">
	<input
		type="range"
		{min}
		{max}
		{step}
		{value}
		aria-label={label}
		aria-valuetext="{value}%"
		oninput={(e) => onchange(Number(e.currentTarget.value))}
	/>
	<output>{value}%</output>
</span>

<style>
	.slider {
		display: inline-flex;
		align-items: center;
		gap: 10px;
		height: var(--control-h);
		font-size: var(--text-caption);
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}

	output {
		min-width: 4ch;
		text-align: right;
	}

	/* The line stops 8px short of each end, where the middle of the 16px
	   thumb comes to rest. */
	input {
		appearance: none;
		width: 160px;
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
