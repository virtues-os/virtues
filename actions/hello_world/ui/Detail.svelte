<!--
	hello_world — Detail.svelte

	A deliberately unhinged showcase of what a view-runtime action can do.
	No design system, no restraint. If you can make Svelte mount it and a
	browser render it, it belongs in a view.
-->

<script lang="ts">
	import { spaceStore } from '$lib/stores/space.svelte';

	function back() {
		spaceStore.openTabFromRoute('/actions');
	}

	const sparkles = Array.from({ length: 24 }, (_, i) => ({
		left: Math.random() * 100,
		top: Math.random() * 100,
		delay: (i * 0.13) % 3,
		size: 8 + Math.random() * 18
	}));
</script>

<svelte:head>
	<link rel="preconnect" href="https://fonts.googleapis.com" />
	<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
	<link
		href="https://fonts.googleapis.com/css2?family=Bungee+Shade&family=Rubik+Glitch&family=Press+Start+2P&display=swap"
		rel="stylesheet"
	/>
</svelte:head>

<section class="party">
	<button class="back" type="button" onclick={back}>← Actions</button>

	{#each sparkles as s}
		<span
			class="sparkle"
			style:left="{s.left}%"
			style:top="{s.top}%"
			style:--size="{s.size}px"
			style:--delay="{s.delay}s"
		>✦</span>
	{/each}

	<h1 class="title">HELLO<br />WORLD</h1>

	<p class="tagline">
		<span class="blink">▶</span>
		view-runtime is just a svelte component, baby
		<span class="blink">◀</span>
	</p>

	<div class="marquee">
		<div class="marquee-track">
			<span>★ NO BACKEND ★ NO CRON ★ NO SUBPROCESS ★ JUST VIBES ★</span>
			<span>★ NO BACKEND ★ NO CRON ★ NO SUBPROCESS ★ JUST VIBES ★</span>
		</div>
	</div>

	<div class="grid">
		<div class="cell gif-cell">
			<iframe
				src="https://giphy.com/embed/3oz8xAFtqoOUUrsh7W"
				title="party parrot"
				width="100%"
				height="240"
				frameborder="0"
				allowfullscreen
			></iframe>
		</div>

		<div class="cell yt-cell">
			<iframe
				width="100%"
				height="240"
				src="https://www.youtube.com/embed/dQw4w9WgXcQ?autoplay=0&rel=0"
				title="YouTube video player"
				frameborder="0"
				allow="accelerometer; clipboard-write; encrypted-media; gyroscope; picture-in-picture"
				allowfullscreen
			></iframe>
		</div>

		<div class="cell gif-cell">
			<iframe
				src="https://giphy.com/embed/26gsspfbu0PZNqGZG"
				title="dancing cat"
				width="100%"
				height="240"
				frameborder="0"
				allowfullscreen
			></iframe>
		</div>

		<div class="cell info-cell">
			<h2>WHAT IS THIS</h2>
			<p>
				This entire page is one Svelte file at
				<code>actions/hello_world/ui/Detail.svelte</code>. It mounts when you
				click into the action; unmounts when you close the tab. No process
				supervised, no row in <code>app_action_runs</code>, no cron tick.
			</p>
			<p class="big-emoji">🪩 🦄 🌈 ✨</p>
		</div>
	</div>

	<div class="footer">
		<span class="footer-line">made with svelte • zero backend • all vibes</span>
	</div>
</section>

<style>
	.party {
		--pink-1: #ff8ce8;
		--pink-2: #ff3ec9;
		--pink-3: #ffd6f5;
		--ink: #2d0a3a;

		position: relative;
		min-height: 100%;
		padding: 2rem 1.5rem 4rem;
		background:
			radial-gradient(circle at 20% 10%, #fff 0%, transparent 30%),
			radial-gradient(circle at 80% 90%, #fff 0%, transparent 25%),
			linear-gradient(135deg, var(--pink-1) 0%, var(--pink-2) 50%, #ff66dd 100%);
		color: var(--ink);
		overflow: hidden;
		isolation: isolate;
	}

	.back {
		position: relative;
		z-index: 5;
		font-family: 'Press Start 2P', monospace;
		font-size: 0.625rem;
		padding: 0.5rem 0.75rem;
		border: 2px solid var(--ink);
		border-radius: 4px;
		background: var(--pink-3);
		color: var(--ink);
		cursor: pointer;
		box-shadow: 4px 4px 0 var(--ink);
		transition: transform 0.1s, box-shadow 0.1s;
	}
	.back:hover {
		transform: translate(2px, 2px);
		box-shadow: 2px 2px 0 var(--ink);
	}

	.sparkle {
		position: absolute;
		font-size: var(--size, 14px);
		color: #fff;
		text-shadow: 0 0 8px #fff, 0 0 16px var(--pink-2);
		animation: twinkle 2.4s ease-in-out infinite;
		animation-delay: var(--delay, 0s);
		pointer-events: none;
		z-index: 1;
	}
	@keyframes twinkle {
		0%, 100% { opacity: 0.2; transform: scale(0.6) rotate(0deg); }
		50% { opacity: 1; transform: scale(1.2) rotate(180deg); }
	}

	.title {
		position: relative;
		z-index: 2;
		margin: 1.5rem 0 0.75rem;
		font-family: 'Bungee Shade', 'Rubik Glitch', cursive;
		font-size: clamp(3.5rem, 12vw, 8rem);
		line-height: 0.95;
		text-align: center;
		color: #fff;
		text-shadow:
			6px 6px 0 var(--ink),
			12px 12px 0 var(--pink-2),
			18px 18px 30px rgba(0, 0, 0, 0.3);
		letter-spacing: -0.02em;
		animation: wobble 4s ease-in-out infinite;
	}
	@keyframes wobble {
		0%, 100% { transform: rotate(-2deg) scale(1); }
		50% { transform: rotate(2deg) scale(1.02); }
	}

	.tagline {
		position: relative;
		z-index: 2;
		margin: 0 0 2rem;
		text-align: center;
		font-family: 'Press Start 2P', monospace;
		font-size: 0.75rem;
		color: var(--ink);
		letter-spacing: 0.05em;
	}
	.blink {
		display: inline-block;
		animation: blink 0.8s steps(2) infinite;
	}
	@keyframes blink {
		50% { opacity: 0; }
	}

	.marquee {
		position: relative;
		z-index: 2;
		margin: 0 -1.5rem 2rem;
		padding: 0.75rem 0;
		background: var(--ink);
		color: var(--pink-1);
		font-family: 'Press Start 2P', monospace;
		font-size: 0.875rem;
		overflow: hidden;
		white-space: nowrap;
		border-top: 4px solid var(--pink-3);
		border-bottom: 4px solid var(--pink-3);
	}
	.marquee-track {
		display: inline-flex;
		gap: 3rem;
		animation: scroll 18s linear infinite;
	}
	.marquee-track span {
		flex-shrink: 0;
		padding: 0 1.5rem;
	}
	@keyframes scroll {
		from { transform: translateX(0); }
		to { transform: translateX(-50%); }
	}

	.grid {
		position: relative;
		z-index: 2;
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
		gap: 1rem;
		margin-bottom: 2rem;
	}
	.cell {
		border: 4px solid var(--ink);
		border-radius: 12px;
		background: var(--pink-3);
		box-shadow: 8px 8px 0 var(--ink);
		overflow: hidden;
		transition: transform 0.15s, box-shadow 0.15s;
	}
	.cell:hover {
		transform: translate(-2px, -2px);
		box-shadow: 10px 10px 0 var(--ink);
	}
	.cell iframe {
		display: block;
	}
	.info-cell {
		padding: 1.25rem;
		background: #fff;
	}
	.info-cell h2 {
		margin: 0 0 0.75rem;
		font-family: 'Bungee Shade', cursive;
		font-size: 1.25rem;
		color: var(--pink-2);
		letter-spacing: 0.02em;
	}
	.info-cell p {
		margin: 0 0 0.75rem;
		font-size: 0.875rem;
		line-height: 1.5;
		color: var(--ink);
	}
	.info-cell code {
		font-family: 'Press Start 2P', monospace;
		font-size: 0.625rem;
		padding: 0.125rem 0.375rem;
		background: var(--pink-3);
		border-radius: 3px;
	}
	.big-emoji {
		font-size: 1.75rem;
		text-align: center;
		letter-spacing: 0.4rem;
		margin-bottom: 0 !important;
	}

	.footer {
		position: relative;
		z-index: 2;
		text-align: center;
		font-family: 'Press Start 2P', monospace;
		font-size: 0.625rem;
		color: rgba(45, 10, 58, 0.7);
		letter-spacing: 0.1em;
	}
	.footer-line {
		display: inline-block;
		padding: 0.5rem 1rem;
		background: rgba(255, 255, 255, 0.5);
		border-radius: 99px;
	}
</style>
