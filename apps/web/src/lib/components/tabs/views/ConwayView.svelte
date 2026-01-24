<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { onMount, onDestroy } from 'svelte';
	import 'iconify-icon';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let canvasRef = $state<HTMLCanvasElement | null>(null);
	let containerRef = $state<HTMLElement | null>(null);
	let animationId: number | null = null;
	let isRunning = $state(true);
	let generation = $state(0);
	let population = $state(0);
	let tickRate = $state(100); // milliseconds between generations (lower = faster)

	// Grid settings
	const CELL_SIZE = 8;
	let cols = 0;
	let rows = 0;
	let grid: boolean[][] = [];

	// Colors from CSS variables
	let cellColor = '#3b82f6';
	let bgColor = '#0a0a0a';

	function initGrid(randomize = true) {
		grid = [];
		for (let i = 0; i < cols; i++) {
			grid[i] = [];
			for (let j = 0; j < rows; j++) {
				grid[i][j] = randomize ? Math.random() < 0.15 : false;
			}
		}
		generation = 0;
		countPopulation();
	}

	function countPopulation() {
		let count = 0;
		for (let i = 0; i < cols; i++) {
			for (let j = 0; j < rows; j++) {
				if (grid[i][j]) count++;
			}
		}
		population = count;
	}

	function countNeighbors(x: number, y: number): number {
		let count = 0;
		for (let i = -1; i <= 1; i++) {
			for (let j = -1; j <= 1; j++) {
				if (i === 0 && j === 0) continue;
				const nx = (x + i + cols) % cols;
				const ny = (y + j + rows) % rows;
				if (grid[nx][ny]) count++;
			}
		}
		return count;
	}

	function nextGeneration() {
		const newGrid: boolean[][] = [];
		for (let i = 0; i < cols; i++) {
			newGrid[i] = [];
			for (let j = 0; j < rows; j++) {
				const neighbors = countNeighbors(i, j);
				const alive = grid[i][j];

				if (alive) {
					// Cell survives with 2 or 3 neighbors
					newGrid[i][j] = neighbors === 2 || neighbors === 3;
				} else {
					// Cell is born with exactly 3 neighbors
					newGrid[i][j] = neighbors === 3;
				}
			}
		}
		grid = newGrid;
		generation++;
		countPopulation();
	}

	function draw() {
		if (!canvasRef) return;
		const ctx = canvasRef.getContext('2d');
		if (!ctx) return;

		// Clear with slight trail effect for zen feeling
		ctx.fillStyle = bgColor;
		ctx.globalAlpha = 0.2;
		ctx.fillRect(0, 0, canvasRef.width, canvasRef.height);
		ctx.globalAlpha = 1;

		// Draw cells
		ctx.fillStyle = cellColor;
		for (let i = 0; i < cols; i++) {
			for (let j = 0; j < rows; j++) {
				if (grid[i][j]) {
					ctx.beginPath();
					ctx.arc(
						i * CELL_SIZE + CELL_SIZE / 2,
						j * CELL_SIZE + CELL_SIZE / 2,
						CELL_SIZE / 2 - 1,
						0,
						Math.PI * 2
					);
					ctx.fill();
				}
			}
		}
	}

	function gameLoop() {
		if (isRunning) {
			nextGeneration();
		}
		draw();
		animationId = requestAnimationFrame(() => {
			setTimeout(gameLoop, tickRate);
		});
	}

	function handleCanvasClick(e: MouseEvent) {
		if (!canvasRef) return;
		const rect = canvasRef.getBoundingClientRect();
		const x = Math.floor((e.clientX - rect.left) / CELL_SIZE);
		const y = Math.floor((e.clientY - rect.top) / CELL_SIZE);
		
		// Spawn a glider pattern
		if (x >= 1 && x < cols - 1 && y >= 1 && y < rows - 1) {
			grid[x][y] = true;
			grid[x + 1][y] = true;
			grid[x - 1][y] = true;
			grid[x][y - 1] = true;
			grid[x + 1][y + 1] = true;
			countPopulation();
		}
	}

	function handleCanvasDrag(e: MouseEvent) {
		if (e.buttons !== 1) return;
		if (!canvasRef) return;
		const rect = canvasRef.getBoundingClientRect();
		const x = Math.floor((e.clientX - rect.left) / CELL_SIZE);
		const y = Math.floor((e.clientY - rect.top) / CELL_SIZE);
		
		if (x >= 0 && x < cols && y >= 0 && y < rows) {
			grid[x][y] = true;
			countPopulation();
		}
	}

	function resize() {
		if (!containerRef || !canvasRef) return;
		
		const width = containerRef.clientWidth;
		const height = containerRef.clientHeight;
		
		canvasRef.width = width;
		canvasRef.height = height;
		
		const newCols = Math.floor(width / CELL_SIZE);
		const newRows = Math.floor(height / CELL_SIZE);
		
		if (newCols !== cols || newRows !== rows) {
			cols = newCols;
			rows = newRows;
			initGrid();
		}
	}

	function getColors() {
		if (typeof window === 'undefined') return;
		const style = getComputedStyle(document.documentElement);
		cellColor = style.getPropertyValue('--color-primary').trim() || '#3b82f6';
		bgColor = style.getPropertyValue('--color-background').trim() || '#0a0a0a';
	}

	onMount(() => {
		getColors();
		resize();
		gameLoop();

		// Watch for theme changes
		const observer = new MutationObserver(() => {
			getColors();
		});
		observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class', 'data-theme'] });

		window.addEventListener('resize', resize);

		return () => {
			observer.disconnect();
			window.removeEventListener('resize', resize);
		};
	});

	onDestroy(() => {
		if (animationId) {
			cancelAnimationFrame(animationId);
		}
	});
</script>

<div class="conway-view" bind:this={containerRef}>
	<canvas
		bind:this={canvasRef}
		onclick={handleCanvasClick}
		onmousemove={handleCanvasDrag}
	></canvas>

	<div class="controls">
		<div class="stats">
			<span class="stat">Gen: {generation}</span>
			<span class="stat">Pop: {population}</span>
		</div>
		<div class="speed-control">
			<iconify-icon icon="ri:speed-line" width="14"></iconify-icon>
			<input
				type="range"
				min="20"
				max="500"
				step="10"
				bind:value={tickRate}
				class="speed-slider"
				title="Speed (lower = faster)"
			/>
		</div>
		<div class="buttons">
			<button
				class="control-btn"
				onclick={() => (isRunning = !isRunning)}
				title={isRunning ? 'Pause' : 'Play'}
			>
				<iconify-icon icon={isRunning ? 'ri:pause-line' : 'ri:play-line'} width="18"></iconify-icon>
			</button>
			<button
				class="control-btn"
				onclick={() => initGrid(true)}
				title="Randomize"
			>
				<iconify-icon icon="ri:refresh-line" width="18"></iconify-icon>
			</button>
			<button
				class="control-btn"
				onclick={() => initGrid(false)}
				title="Clear"
			>
				<iconify-icon icon="ri:delete-bin-line" width="18"></iconify-icon>
			</button>
		</div>
	</div>

	<div class="hint">
		Click to spawn life. Drag to draw.
	</div>
</div>

<style>
	.conway-view {
		position: relative;
		width: 100%;
		height: 100%;
		overflow: hidden;
		background: var(--color-background);
	}

	canvas {
		display: block;
		cursor: crosshair;
	}

	.controls {
		position: absolute;
		bottom: 1rem;
		left: 50%;
		transform: translateX(-50%);
		display: flex;
		align-items: center;
		gap: 1.5rem;
		padding: 0.75rem 1.25rem;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
	}

	.stats {
		display: flex;
		gap: 1rem;
	}

	.stat {
		font-size: 0.8rem;
		font-family: var(--font-mono, monospace);
		color: var(--color-foreground-muted);
	}

	.speed-control {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--color-foreground-muted);
	}

	.speed-slider {
		width: 80px;
		height: 4px;
		-webkit-appearance: none;
		appearance: none;
		background: var(--color-border);
		border-radius: 2px;
		cursor: pointer;
		/* Invert so left = fast, right = slow feels more natural */
		direction: rtl;
	}

	.speed-slider::-webkit-slider-thumb {
		-webkit-appearance: none;
		appearance: none;
		width: 12px;
		height: 12px;
		background: var(--color-foreground-muted);
		border-radius: 50%;
		cursor: pointer;
		transition: background 0.15s ease;
	}

	.speed-slider::-webkit-slider-thumb:hover {
		background: var(--color-primary);
	}

	.speed-slider::-moz-range-thumb {
		width: 12px;
		height: 12px;
		background: var(--color-foreground-muted);
		border: none;
		border-radius: 50%;
		cursor: pointer;
	}

	.buttons {
		display: flex;
		gap: 0.5rem;
	}

	.control-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		background: var(--color-surface-elevated);
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.control-btn:hover {
		border-color: var(--color-primary);
		color: var(--color-primary);
	}

	.hint {
		position: absolute;
		top: 1rem;
		left: 50%;
		transform: translateX(-50%);
		font-size: 0.8rem;
		color: var(--color-foreground-subtle);
		opacity: 0.6;
		pointer-events: none;
	}
</style>
