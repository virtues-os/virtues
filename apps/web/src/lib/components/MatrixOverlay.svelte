<script lang="ts">
	import { onMount } from "svelte";

	interface Props {
		layout?: "bottom" | "left" | "right" | "inline";
		cols?: number;
		rows?: number;
		cellSize?: number;
		cellColor?: string;
		fillColor?: string;
		padding?: number;
		message?: string; // Optional message to render as dot-letters (e.g., "SENT")
	}

	let {
		layout = "right",
		cols = 4,
		rows = 24,
		cellSize = 6,
		cellColor = "white",
		fillColor = "var(--primary)",
		padding = 48,
		message,
	}: Props = $props();

	// 3-pixel wide font with 1-pixel gap (4 cols per letter slot)
	const FONT: Record<string, number[][]> = {
		S: [
			[1, 1, 1, 0],
			[1, 0, 0, 0],
			[1, 1, 1, 0],
			[0, 0, 1, 0],
			[1, 1, 1, 0],
		],
		E: [
			[1, 1, 1, 0],
			[1, 0, 0, 0],
			[1, 1, 0, 0],
			[1, 0, 0, 0],
			[1, 1, 1, 0],
		],
		N: [
			[1, 0, 1, 0],
			[1, 1, 1, 0],
			[1, 0, 1, 0],
			[1, 0, 1, 0],
			[1, 0, 1, 0],
		],
		T: [
			[1, 1, 1, 0],
			[0, 1, 0, 0],
			[0, 1, 0, 0],
			[0, 1, 0, 0],
			[0, 1, 0, 0],
		],
		" ": [
			[0, 0, 0, 0],
			[0, 0, 0, 0],
			[0, 0, 0, 0],
			[0, 0, 0, 0],
			[0, 0, 0, 0],
		],
	};

	// Each cell is either null (invisible) or a random opacity between 0.2 and 1
	let matrixCells = $state<(number | null)[]>([]);

	let totalCells = $derived(cols * rows);

	// Position styles based on layout
	let positionStyle = $derived(
		layout === "inline"
			? "" // No absolute positioning for inline
			: layout === "bottom"
				? "position: absolute; bottom: 0; left: 0; width: 100%; height: auto;"
				: layout === "left"
					? "position: absolute; top: 0; left: 0; width: auto; height: 100%;"
					: "position: absolute; top: 0; right: 0; width: auto; height: 100%;",
	);

	// Check if a cell should be lit for the message
	function isMessageCell(index: number): boolean {
		if (!message) return false;
		const col = index % cols;
		const row = Math.floor(index / cols);
		const charWidth = 4;
		const charIndex = Math.floor(col / charWidth);
		const charCol = col % charWidth;

		if (charIndex >= message.length) return false;

		const char = message[charIndex].toUpperCase();
		const glyph = FONT[char];
		if (!glyph || row >= glyph.length) return false;

		return glyph[row][charCol] === 1;
	}

	// Generate matrix cells with 50% chance of being visible, random opacity 0.2-1
	function generateMatrixCells(count: number): (number | null)[] {
		const cells: (number | null)[] = [];
		for (let i = 0; i < count; i++) {
			if (Math.random() < 0.5) {
				// Random opacity between 0.2 and 0.8
				cells.push(0.2 + Math.random() * 0.6);
			} else {
				cells.push(null);
			}
		}
		return cells;
	}

	// Calculate delay based on diagonal position (top-left to bottom-right swoosh)
	function calculateDelay(index: number): number {
		const row = Math.floor(index / cols);
		const col = index % cols;
		// Diagonal distance from top-left (0,0)
		// Cells at same diagonal distance animate together
		const diagonalIndex = row + col;
		return diagonalIndex * 30 + 100; // 30ms between diagonals, 100ms initial delay
	}

	onMount(() => {
		matrixCells = generateMatrixCells(totalCells);
	});

	// Regenerate cells when size changes
	let prevTotalCells = totalCells;
	$effect(() => {
		if (totalCells > 0 && totalCells !== prevTotalCells) {
			matrixCells = generateMatrixCells(totalCells);
			prevTotalCells = totalCells;
		}
	});
</script>

{#key message}
	<div
		class="matrix-overlay"
		style="{positionStyle} grid-template-columns: repeat({cols}, {cellSize}px); grid-template-rows: repeat({rows}, {cellSize}px); gap: {cellSize}px; padding: {padding}px;"
		role="presentation"
	>
		{#each matrixCells as cellOpacity, i}
			{@const isMessage = isMessageCell(i)}
			{@const showRandomDot = !message && cellOpacity !== null}
			{@const showMessageDot = message && isMessage}
			<div
				class="matrix-cell"
				class:transparent={!showRandomDot && !showMessageDot}
				style="
					width: {cellSize}px;
					height: {cellSize}px;
					animation-delay: {calculateDelay(i)}ms;
					--cell-opacity: {showMessageDot ? 1 : showRandomDot ? cellOpacity : 0};
					{showMessageDot
					? `background-color: ${fillColor};`
					: showRandomDot
						? `background-color: ${cellColor};`
						: ''}
				"
			></div>
		{/each}
	</div>
{/key}

<style>
	.matrix-overlay {
		z-index: 10;
		display: grid;
		pointer-events: none;
	}

	.matrix-cell {
		border-radius: 9999px;
		opacity: 0;
		flex-shrink: 0;
		animation: matrixSwoosh 300ms ease-in-out forwards;
	}

	.matrix-cell.transparent {
		background-color: transparent;
		animation: none;
	}

	@keyframes matrixSwoosh {
		from {
			transform: scale(0.5);
			opacity: 0;
		}
		to {
			transform: scale(1);
			opacity: var(--cell-opacity);
		}
	}
</style>
