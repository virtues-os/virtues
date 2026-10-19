<script lang="ts">
	/**
	 * Setup's glyph: a ring cut into one arc per step, each arc inked when
	 * its step is done. The count is the drawing, so the label can be the
	 * room's name alone, the same length as every other label on the rail.
	 * Arcs sit in step order, clockwise from twelve, so a step done out of
	 * order shows as the gap it leaves rather than as a tally. Skipped is not
	 * done and stays faint, as it does in the flow.
	 */
	import { setup } from '$lib/components/setup/setup.svelte';

	let { size = 20 }: { size?: number } = $props();

	const R = 5.6;
	// Degrees between arcs. Butt caps: round ones would close a ten-step
	// ring into a solid circle at 20px.
	const GAP = 9;

	function point(deg: number): string {
		const a = ((deg - 90) * Math.PI) / 180;
		return `${(8 + R * Math.cos(a)).toFixed(3)} ${(8 + R * Math.sin(a)).toFixed(3)}`;
	}

	const arcs = $derived.by(() => {
		const steps = setup.steps;
		const span = 360 / steps.length;
		return steps.map((s, i) => {
			const from = i * span + GAP / 2;
			const to = (i + 1) * span - GAP / 2;
			return {
				id: s.id,
				done: s.status === 'done',
				d: `M${point(from)}A${R} ${R} 0 ${to - from > 180 ? 1 : 0} 1 ${point(to)}`,
			};
		});
	});
</script>

<svg
	viewBox="0 0 16 16"
	width={size}
	height={size}
	fill="none"
	stroke="currentColor"
	stroke-linecap="butt"
	aria-hidden="true"
>
	{#each arcs as arc (arc.id)}
		<path d={arc.d} class:todo={!arc.done} />
	{/each}
</svg>

<style>
	/* Done has to outweigh to-do at a glance, or the ring reads as a
	   loading spinner: heavier ink against a thin, faint track. */
	path {
		stroke-width: 1.7;
	}
	.todo {
		stroke-width: 1;
		opacity: 0.4;
	}
</style>
