<script lang="ts">
	import type { DayState } from '$lib/applets/days';

	/**
	 * One day of an applet's runs, as a dot. The Applets table, an applet's
	 * Info and its Run history all draw the same states, so they draw them
	 * here and can't drift apart.
	 *
	 * Color marks the states a person can act on (failed, stopped, missed);
	 * a day that ran is the success hue, and a day with nothing is a speck.
	 */
	let {
		state,
		size = 'sm',
		title
	}: {
		state: DayState;
		/** 8px in a table row, 12px where the dots are the subject. */
		size?: 'sm' | 'md';
		title?: string;
	} = $props();
</script>

<span class="day-dot {state}" data-size={size} {title}></span>

<style>
	.day-dot {
		display: inline-block;
		box-sizing: border-box;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		flex: none;
	}
	.day-dot[data-size='md'] {
		width: 12px;
		height: 12px;
	}
	.ran {
		background: var(--color-success);
	}
	.failed {
		background: var(--color-error);
	}
	.stopped {
		background: var(--color-warning);
	}
	.quiet {
		background: var(--color-border-strong);
	}
	/* Hollow: it should have run and didn't. */
	.missed {
		border: 2px solid var(--color-error);
	}
	/* A speck, so a run of empty days doesn't read as a row of results. */
	.none {
		width: 4px;
		height: 4px;
		margin: 0 2px;
		background: var(--color-border-strong);
	}
	.none[data-size='md'] {
		width: 4px;
		height: 4px;
		margin: 0 4px;
	}
</style>
