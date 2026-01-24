<script lang="ts">
	import type { Tab } from '$lib/tabs/types';
	import { onMount, onDestroy } from 'svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Game dimensions
	const VIEWPORT_WIDTH = 400;
	const VIEWPORT_HEIGHT = 220;
	const GROUND_Y = 50;
	const DOG_WIDTH = 28;
	const DOG_HEIGHT = 22;

	// Physics
	const GRAVITY = 0.5;
	const JUMP_FORCE = -9.5;
	const RUN_SPEED = 3;

	// Game state
	let gameState = $state<'ready' | 'running' | 'lost'>('ready');
	let dogX = $state(60);
	let dogY = $state(0);
	let dogVelocity = 0;
	let cameraX = $state(0);
	let score = $state(0);
	let bestScore = $state<number | null>(null);

	// Animation
	let animationId: number | null = null;

	// Procedural hurdles
	let hurdles = $state<{ x: number; height: number }[]>([]);
	let lastHurdleX = 0;

	function spawnInitialHurdles() {
		hurdles = [];
		lastHurdleX = 400; // First hurdle starts well ahead of dog
		for (let i = 0; i < 5; i++) {
			spawnHurdle();
		}
	}

	function spawnHurdle() {
		const gap = 150 + Math.random() * 100; // 150-250px between hurdles
		const height = 26 + Math.random() * 12; // 26-38px height
		lastHurdleX += gap;
		hurdles.push({ x: lastHurdleX, height });
	}

	// Parallax elements
	const clouds = [
		{ x: 50, y: 20, size: 1 },
		{ x: 200, y: 35, size: 0.7 },
		{ x: 400, y: 15, size: 0.9 },
		{ x: 600, y: 40, size: 0.6 },
		{ x: 800, y: 25, size: 0.8 },
		{ x: 1000, y: 30, size: 0.7 },
		{ x: 1200, y: 18, size: 0.9 }
	];

	const silhouettes = [
		{ x: 30, type: 'tree', height: 45 },
		{ x: 100, type: 'house', height: 40 },
		{ x: 180, type: 'tree', height: 35 },
		{ x: 220, type: 'tree', height: 50 },
		{ x: 320, type: 'house', height: 45 },
		{ x: 420, type: 'tree', height: 40 },
		{ x: 500, type: 'tree', height: 55 },
		{ x: 580, type: 'house', height: 38 },
		{ x: 680, type: 'tree', height: 42 },
		{ x: 750, type: 'tree', height: 48 },
		{ x: 850, type: 'house', height: 44 },
		{ x: 950, type: 'tree', height: 38 },
		{ x: 1050, type: 'tree', height: 52 },
		{ x: 1150, type: 'house', height: 42 },
		{ x: 1250, type: 'tree', height: 46 }
	];

	function jump() {
		if (!active) return;

		if (gameState === 'ready') {
			startGame();
			return;
		}
		if (gameState === 'lost') {
			resetGame();
			return;
		}
		// Only jump if on ground
		if (dogY <= 0) {
			dogVelocity = JUMP_FORCE;
		}
	}

	function startGame() {
		gameState = 'running';
		dogX = 60;
		dogY = 0;
		dogVelocity = 0;
		cameraX = 0;
		score = 0;
		spawnInitialHurdles();
		gameLoop();
	}

	function resetGame() {
		gameState = 'ready';
		dogX = 60;
		dogY = 0;
		dogVelocity = 0;
		cameraX = 0;
		score = 0;
		hurdles = [];
	}

	function updateGame() {
		if (gameState !== 'running') return;

		// Move dog forward
		dogX += RUN_SPEED;

		// Update score (distance traveled)
		score = Math.floor(dogX / 10);

		// Apply gravity
		dogVelocity += GRAVITY;
		dogY += dogVelocity;

		// Ground collision
		if (dogY >= 0) {
			dogY = 0;
			dogVelocity = 0;
		}

		// Update camera (follow dog with offset)
		cameraX = Math.max(0, dogX - 80);

		// Spawn new hurdles ahead
		while (lastHurdleX < dogX + VIEWPORT_WIDTH + 200) {
			spawnHurdle();
		}

		// Remove hurdles that are far behind
		hurdles = hurdles.filter((h) => h.x > dogX - 100);

		// Check hurdle collisions
		const dogLeft = dogX;
		const dogRight = dogX + DOG_WIDTH - 6;
		const dogBottom = GROUND_Y - dogY;
		const dogTop = dogBottom + DOG_HEIGHT - 4;

		for (const hurdle of hurdles) {
			const hurdleLeft = hurdle.x;
			const hurdleRight = hurdle.x + 34;
			const hurdleTop = GROUND_Y + hurdle.height;

			const horizontalOverlap = dogRight > hurdleLeft && dogLeft < hurdleRight;
			const verticalOverlap = dogBottom < hurdleTop && dogTop > GROUND_Y;

			if (horizontalOverlap && verticalOverlap) {
				gameState = 'lost';
				// Save best score
				if (bestScore === null || score > bestScore) {
					bestScore = score;
					if (typeof window !== 'undefined') {
						localStorage.setItem('virtues-dog-agility-best', String(score));
					}
				}
				return;
			}
		}
	}

	function gameLoop() {
		if (gameState !== 'running') return;

		updateGame();

		if (gameState === 'running') {
			animationId = requestAnimationFrame(gameLoop);
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (!active) return;
		if (e.metaKey || e.ctrlKey || e.altKey) return;

		if (e.code === 'Space' || e.code === 'ArrowUp') {
			e.preventDefault();
			jump();
		}
	}

	onMount(() => {
		if (typeof window !== 'undefined') {
			const saved = localStorage.getItem('virtues-dog-agility-best');
			if (saved) bestScore = parseInt(saved, 10);
		}

		window.addEventListener('keydown', handleKeydown);
		return () => window.removeEventListener('keydown', handleKeydown);
	});

	onDestroy(() => {
		if (animationId) cancelAnimationFrame(animationId);
	});

	// Derived values
	const dogBottom = $derived(GROUND_Y - dogY);
	const isJumping = $derived(dogY < 0);
</script>

<div class="game-wrapper">
	<div
		class="game-container"
		style="width: {VIEWPORT_WIDTH}px; height: {VIEWPORT_HEIGHT}px;"
		onclick={jump}
		onkeydown={(e) => e.code === 'Space' && jump()}
		role="button"
		tabindex="0"
	>
		<!-- Sky gradient -->
		<div class="sky"></div>

		<!-- Clouds layer (slowest parallax) -->
		<div class="parallax-layer clouds" style="transform: translateX({-cameraX * 0.05}px)">
			{#each clouds as cloud}
				<svg
					class="cloud"
					style="left: {cloud.x}px; top: {cloud.y}px; transform: scale({cloud.size})"
					width="40"
					height="20"
					viewBox="0 0 40 20"
				>
					<ellipse cx="12" cy="14" rx="10" ry="6" />
					<ellipse cx="24" cy="12" rx="12" ry="8" />
					<ellipse cx="34" cy="14" rx="6" ry="5" />
				</svg>
			{/each}
		</div>

		<!-- Silhouettes layer (slow parallax) -->
		<div class="parallax-layer silhouettes" style="transform: translateX({-cameraX * 0.2}px)">
			{#each silhouettes as item}
				{#if item.type === 'tree'}
					<svg
						class="silhouette"
						style="left: {item.x}px; bottom: {GROUND_Y}px;"
						width="24"
						height={item.height}
						viewBox="0 0 24 {item.height}"
					>
						<polygon points="12,0 24,{item.height - 8} 0,{item.height - 8}" />
						<rect x="10" y={item.height - 10} width="4" height="10" />
					</svg>
				{:else}
					<svg
						class="silhouette"
						style="left: {item.x}px; bottom: {GROUND_Y}px;"
						width="32"
						height={item.height}
						viewBox="0 0 32 {item.height}"
					>
						<rect x="0" y="12" width="32" height={item.height - 12} />
						<polygon points="0,12 16,0 32,12" />
						<rect x="6" y={item.height - 14} width="6" height="10" fill="var(--color-surface-elevated)" />
						<rect x="18" y={item.height - 20} width="5" height="6" fill="var(--color-surface-elevated)" />
					</svg>
				{/if}
			{/each}
		</div>

		<!-- Ground -->
		<div class="ground" style="bottom: 0; height: {GROUND_Y}px;">
			<div class="ground-line" style="top: 0;"></div>
			<div class="ground-texture" style="transform: translateX({-cameraX % 20}px)"></div>
		</div>

		<!-- Game layer (hurdles, dog, finish) -->
		<div class="game-layer" style="transform: translateX({-cameraX}px)">
			<!-- Hurdles -->
			{#each hurdles as hurdle}
				<svg
					class="hurdle"
					style="left: {hurdle.x}px; bottom: {GROUND_Y}px;"
					width="34"
					height={hurdle.height}
					viewBox="0 0 34 {hurdle.height}"
				>
					<!-- Left post with stripes -->
					<rect x="0" y="0" width="5" height={hurdle.height} class="post" />
					<rect x="0" y="0" width="5" height="5" class="stripe" />
					<rect x="0" y="10" width="5" height="5" class="stripe" />
					<rect x="0" y="20" width="5" height="5" class="stripe" />
					{#if hurdle.height > 28}
						<rect x="0" y="30" width="5" height="5" class="stripe" />
					{/if}

					<!-- Right post with stripes -->
					<rect x="29" y="0" width="5" height={hurdle.height} class="post" />
					<rect x="29" y="0" width="5" height="5" class="stripe" />
					<rect x="29" y="10" width="5" height="5" class="stripe" />
					<rect x="29" y="20" width="5" height="5" class="stripe" />
					{#if hurdle.height > 28}
						<rect x="29" y="30" width="5" height="5" class="stripe" />
					{/if}

					<!-- Bar -->
					<rect x="0" y="2" width="34" height="4" rx="2" class="bar" />
				</svg>
			{/each}

			<!-- Dog -->
			<svg
				class="dog"
				class:jumping={isJumping}
				style="left: {dogX}px; bottom: {dogBottom}px;"
				width="28"
				height="22"
				viewBox="0 0 28 22"
			>
				<!-- Body -->
				<ellipse cx="11" cy="13" rx="9" ry="6" class="dog-body" />
				<!-- Head -->
				<circle cx="21" cy="9" r="6" class="dog-body" />
				<!-- Ear -->
				<ellipse cx="24" cy="4" rx="2.5" ry="4" class="dog-body" />
				<!-- Snout -->
				<ellipse cx="26" cy="10" rx="2" ry="1.5" class="dog-body" />
				<!-- Tail -->
				<path d="M2 11 Q-1 6 3 4" class="dog-tail" />
				<!-- Legs -->
				<rect x="5" y="17" width="3" height="5" rx="1" class="dog-body" />
				<rect x="13" y="17" width="3" height="5" rx="1" class="dog-body" />
				<!-- Eye -->
				<circle cx="22" cy="8" r="1.5" class="dog-eye" />
				<!-- Nose -->
				<circle cx="27" cy="10" r="1" class="dog-nose" />
			</svg>
		</div>

		<!-- UI Overlay -->
		{#if gameState === 'running'}
			<div class="score-display">{score}</div>
		{/if}

		<!-- Ready state -->
		{#if gameState === 'ready'}
			<div class="overlay">
				<div class="overlay-content">
					<span class="title">Agility Run</span>
					<span class="hint">space or click to start</span>
				</div>
			</div>
		{/if}

		<!-- Lose state -->
		{#if gameState === 'lost'}
			<div class="overlay">
				<div class="overlay-content">
					<span class="title">Knocked a bar!</span>
					<span class="final-score">{score}</span>
					{#if bestScore !== null && score >= bestScore}
						<span class="best-badge">new best!</span>
					{/if}
					<span class="hint">click to retry</span>
				</div>
			</div>
		{/if}
	</div>

	<!-- Best score display -->
	{#if bestScore !== null}
		<div class="best-score">best: {bestScore}</div>
	{/if}
</div>

<style>
	.game-wrapper {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 100%;
		gap: 0.5rem;
	}

	.game-container {
		position: relative;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		overflow: hidden;
		cursor: pointer;
		outline: none;
	}

	.game-container:focus {
		border-color: var(--color-primary);
	}

	/* Sky */
	.sky {
		position: absolute;
		inset: 0;
		background: linear-gradient(
			to bottom,
			var(--color-background) 0%,
			var(--color-surface-elevated) 100%
		);
	}

	/* Parallax layers */
	.parallax-layer {
		position: absolute;
		inset: 0;
		pointer-events: none;
	}

	/* Clouds */
	.cloud {
		position: absolute;
		fill: var(--color-foreground-subtle);
		opacity: 0.15;
	}

	/* Silhouettes */
	.silhouette {
		position: absolute;
		fill: var(--color-foreground-subtle);
		opacity: 0.25;
	}

	/* Ground */
	.ground {
		position: absolute;
		left: 0;
		right: 0;
		background: var(--color-surface-elevated);
		overflow: hidden;
	}

	.ground-line {
		position: absolute;
		left: 0;
		right: 0;
		height: 1px;
		background: var(--color-border);
	}

	.ground-texture {
		position: absolute;
		inset: 1px 0 0 0;
		background-image: repeating-linear-gradient(
			90deg,
			var(--color-border-subtle) 0px,
			var(--color-border-subtle) 1px,
			transparent 1px,
			transparent 20px
		);
		opacity: 0.5;
	}

	/* Game layer */
	.game-layer {
		position: absolute;
		inset: 0;
	}

	/* Hurdles */
	.hurdle {
		position: absolute;
	}

	.hurdle .post {
		fill: var(--color-foreground-subtle);
	}

	.hurdle .stripe {
		fill: var(--color-primary);
	}

	.hurdle .bar {
		fill: var(--color-primary);
	}

	/* Dog */
	.dog {
		position: absolute;
		transition: transform 0.05s ease-out;
	}

	.dog.jumping {
		transform: rotate(-12deg);
	}

	.dog-body {
		fill: var(--color-foreground-muted);
	}

	.dog-tail {
		stroke: var(--color-foreground-muted);
		stroke-width: 2.5;
		fill: none;
		stroke-linecap: round;
	}

	.dog-eye {
		fill: var(--color-background);
	}

	.dog-nose {
		fill: var(--color-foreground);
	}

	/* Score */
	.score-display {
		position: absolute;
		top: 8px;
		right: 10px;
		font-family: var(--font-mono, monospace);
		font-size: 0.85rem;
		color: var(--color-foreground-muted);
	}

	.best-score {
		font-family: var(--font-mono, monospace);
		font-size: 0.7rem;
		color: var(--color-foreground-subtle);
	}

	/* Overlays */
	.overlay {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(in srgb, var(--color-surface) 85%, transparent);
	}

	.overlay-content {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.35rem;
	}

	.title {
		font-family: var(--font-serif);
		font-size: 1.1rem;
		color: var(--color-foreground);
	}

	.hint {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}

	.final-score {
		font-family: var(--font-mono, monospace);
		font-size: 1.5rem;
		color: var(--color-foreground);
	}

	.best-badge {
		font-size: 0.7rem;
		color: var(--color-primary);
		padding: 0.15rem 0.5rem;
		border: 1px solid var(--color-primary);
		border-radius: 4px;
	}
</style>
