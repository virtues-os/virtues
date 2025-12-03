<script lang="ts">
	import { onMount } from "svelte";

	interface Props {
		layout?: "bottom" | "left" | "right";
		cols?: number;
		rows?: number;
		cellSize?: number;
		cellColor?: string;
	}

	let {
		layout = "right",
		cols = 4,
		rows = 24,
		cellSize = 6,
		cellColor = "white",
	}: Props = $props();

	// Each cell is either null (invisible) or a random opacity between 0.2 and 1
	let matrixCells = $state<(number | null)[]>([]);

	let totalCells = $derived(cols * rows);

	// Animation class based on layout
	let animationClass = $derived(
		layout === "bottom"
			? "matrix-anim-bottom"
			: layout === "left"
				? "matrix-anim-left"
				: "matrix-anim-right",
	);

	// Position styles based on layout
	let positionStyle = $derived(
		layout === "bottom"
			? "bottom: 0; left: 0; width: 100%; height: auto;"
			: layout === "left"
				? "top: 0; left: 0; width: auto; height: 100%;"
				: "top: 0; right: 0; width: auto; height: 100%;",
	);

	// Generate matrix cells with 50% chance of being visible, random opacity 0.2-1
	function generateMatrixCells(count: number): (number | null)[] {
		const cells: (number | null)[] = [];
		for (let i = 0; i < count; i++) {
			if (Math.random() < 0.5) {
				// Random opacity between 0.2 and 1
				cells.push(0.2 + Math.random() * 1);
			} else {
				cells.push(null);
			}
		}
		return cells;
	}

	// Calculate delay based on position according to layout
	function calculateDelay(index: number): number {
		if (layout === "bottom") {
			const row = Math.floor(index / cols);
			const col = index % cols;
			return row * 25 + col * 25 + 150;
		} else if (layout === "left") {
			const row = index % rows;
			const col = Math.floor(index / rows);
			return row * 25 + col * 25 + 150;
		} else {
			// right - animate from right edge
			const row = index % rows;
			const col = Math.floor(index / rows);
			return row * 25 + (cols - 1 - col) * 25 + 150;
		}
	}

	onMount(() => {
		matrixCells = generateMatrixCells(totalCells);
	});

	// Regenerate cells when size changes
	$effect(() => {
		if (totalCells > 0) {
			matrixCells = generateMatrixCells(totalCells);
		}
	});
</script>

<div
	class="matrix-overlay"
	style="{positionStyle} grid-template-columns: repeat({cols}, {cellSize}px); grid-template-rows: repeat({rows}, {cellSize}px); gap: {cellSize}px;"
>
	{#each matrixCells as cellOpacity, i}
		<div
			class="matrix-cell {cellOpacity !== null
				? ''
				: 'transparent'} {animationClass}"
			style="width: {cellSize}px; height: {cellSize}px; animation-delay: {calculateDelay(
				i,
			)}ms; --cell-opacity: {cellOpacity ?? 0}; {cellOpacity !== null
				? `background-color: ${cellColor};`
				: ''}"
		></div>
	{/each}
</div>

<style>
	.matrix-overlay {
		position: absolute;
		z-index: 10;
		display: grid;
		pointer-events: none;
		padding: 48px;
	}

	.matrix-cell {
		border-radius: 9999px;
		opacity: 0;
		flex-shrink: 0;
	}

	.matrix-cell.transparent {
		background-color: transparent;
	}

	.matrix-anim-bottom {
		animation: matrixAppearBottom 300ms ease-in-out forwards;
	}

	.matrix-anim-left {
		animation: matrixAppearLeft 300ms ease-in-out forwards;
	}

	.matrix-anim-right {
		animation: matrixAppearRight 300ms ease-in-out forwards;
	}

	@keyframes matrixAppearBottom {
		from {
			transform: translateY(8px);
			opacity: 0;
		}
		to {
			transform: translateY(0);
			opacity: var(--cell-opacity);
		}
	}

	@keyframes matrixAppearLeft {
		from {
			transform: translateX(-8px);
			opacity: 0;
		}
		to {
			transform: translateX(0);
			opacity: var(--cell-opacity);
		}
	}

	@keyframes matrixAppearRight {
		from {
			transform: translateX(8px);
			opacity: 0;
		}
		to {
			transform: translateX(0);
			opacity: var(--cell-opacity);
		}
	}
</style>
