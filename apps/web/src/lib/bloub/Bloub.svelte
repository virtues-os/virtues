<script lang="ts">
	// Svelte port of bloub's BloubBot.vue (vendored under ./bot — see README.md
	// there for origin and license). The engine is clock-free; this component
	// owns the clock and re-renders one sampled frame per animation tick.
	import { untrack } from "svelte";
	import { NOTIF_BLUE, type DotRender } from "./bot/decor";
	import { BotEngine, type BotFrame } from "./bot/engine";
	import { DEFAULT_EXPRESSION, EXPRESSION_BY_ID } from "./bot/expressions";
	import { COLOR_BY_ID, DEFAULT_COLOR, DEFAULT_SHAPE, SHAPE_BY_ID, mixHex } from "./bot/skins";
	import { DEMI_VIEWBOX, RAYON } from "./bot/repere";
	import type { StateId } from "./bot/states";

	interface Props {
		size?: number;
		/** Engine state to hold — the caller drives mood ('idle', 'thinking', …). */
		state?: StateId;
		/** Body shape id from the bloub customizer set. */
		shape?: string;
		/** Ink color id from the bloub customizer set. */
		color?: string;
		/**
		 * Direct CSS color for the body ink; overrides `color` when set. Accepts
		 * theme vars (`var(--color-foreground)`) so the bot flips with dark mode
		 * instead of going black-on-black. Depth-misted burst particles need a
		 * hex to blend and fall back to this plain ink otherwise.
		 */
		ink?: string;
		/** Resting expression id from the bloub customizer set. */
		expression?: string;
		/**
		 * Page background behind the bot. The eyes are holes in the body, backed
		 * by a paper-filled copy of the silhouette so orbit arcs passing behind
		 * the body never show through them — so this must match the page. Any CSS
		 * color works for that backing; depth-misted burst particles need a hex
		 * and fall back to plain ink otherwise.
		 */
		paper?: string;
	}

	let {
		size = 96,
		state: stateId = "idle",
		shape = DEFAULT_SHAPE,
		color = DEFAULT_COLOR,
		ink: inkOverride = undefined,
		expression = DEFAULT_EXPRESSION,
		paper = "#f9f9f9",
	}: Props = $props();

	const R = RAYON;
	const VB = DEMI_VIEWBOX;
	const uid = Math.random().toString(36).slice(2, 8);
	const maskId = `bloub-mask-${uid}`;

	const ink = $derived(inkOverride ?? COLOR_BY_ID.get(color)?.hex ?? "#0a0a0c");

	// The constructor takes the initial values only; the $effects below morph
	// the engine when the props change later.
	const engine = untrack(
		() =>
			new BotEngine(
				R,
				stateId,
				SHAPE_BY_ID.get(shape)?.radii ?? null,
				EXPRESSION_BY_ID.get(expression) ?? null,
			),
	);

	let frame: BotFrame = $state.raw(engine.sample(0));

	// One clock for the component's lifetime; the engine is a pure function of it.
	let clock = 0;
	let raf = 0;
	$effect(() => {
		const t0 = performance.now();
		const tick = (now: number) => {
			clock = (now - t0) / 1000;
			frame = engine.sample(clock);
			raf = requestAnimationFrame(tick);
		};
		raf = requestAnimationFrame(tick);
		return () => cancelAnimationFrame(raf);
	});

	$effect(() => {
		engine.setState(stateId, clock);
	});
	$effect(() => {
		engine.setShape(SHAPE_BY_ID.get(shape)?.radii ?? null, clock);
	});
	$effect(() => {
		engine.setExpression(EXPRESSION_BY_ID.get(expression) ?? null, clock);
	});

	function dotFill(dot: DotRender): string {
		if (dot.color) return dot.color;
		if (dot.depth === undefined) return ink;
		// Depth mist blends toward the page color, which needs two hexes to mix.
		return paper.startsWith("#") && ink.startsWith("#") ? mixHex(paper, ink, dot.depth) : ink;
	}
</script>

<svg
	width={size}
	height={size}
	viewBox={`${-VB} ${-VB} ${VB * 2} ${VB * 2}`}
	role="img"
	aria-label="Interview companion"
>
	<defs>
		<!-- The eyes are holes punched through the body, so the silhouette crops
		     them automatically when they slide toward the edge. -->
		<mask id={maskId} maskUnits="userSpaceOnUse" x={-VB} y={-VB} width={VB * 2} height={VB * 2}>
			<path d={frame.bodyPath} fill="#fff" />
			{#each frame.eyes as eye, i (i)}
				<path d={eye.d} transform={eye.matrix} opacity={eye.alpha} fill="#000" />
			{/each}
			{#if frame.notch}
				<circle cx={frame.notch.x} cy={frame.notch.y} r={frame.notch.r} fill="#000" />
			{/if}
		</mask>

		{#each frame.arcs as arc (arc.id)}
			<linearGradient
				id={`${uid}-${arc.id}`}
				gradientUnits="userSpaceOnUse"
				x1={arc.grad.x1}
				y1={arc.grad.y1}
				x2={arc.grad.x2}
				y2={arc.grad.y2}
			>
				{#each arc.grad.stops as c, i (i)}
					<stop offset={i / (arc.grad.stops.length - 1)} stop-color={c} />
				{/each}
			</linearGradient>
		{/each}
	</defs>

	<!-- Back half of the orbit rings: drawn before the body, so occluded by it. -->
	<g fill="none" stroke-linecap="round">
		{#each frame.arcs as arc (`b${arc.id}`)}
			<path
				d={arc.back}
				stroke={`url(#${uid}-${arc.id})`}
				stroke-width={arc.width}
				opacity={arc.opacity}
			/>
		{/each}
	</g>

	<!-- Burst particles pass behind the core. -->
	{#if frame.dotsBehind}
		<g>
			{#each frame.dots as dot, i (i)}
				{#if dot.d}
					<path
						d={dot.d}
						transform={`translate(${dot.x} ${dot.y}) rotate(${dot.rot ?? 0}) scale(${R})`}
						fill={dotFill(dot)}
						opacity={dot.opacity}
					/>
				{:else}
					<circle cx={dot.x} cy={dot.y} r={dot.r} fill={dotFill(dot)} opacity={dot.opacity} />
				{/if}
			{/each}
		</g>
	{/if}

	<g opacity={frame.bodyAlpha}>
		<!-- Paper-filled backing in the body's exact shape: the eye holes would
		     otherwise reveal the rings and particles drawn behind the body. -->
		<path d={frame.bodyPath} fill={paper} />
		<g mask={`url(#${maskId})`}>
			<rect x={-VB} y={-VB} width={VB * 2} height={VB * 2} fill={ink} />
		</g>
	</g>

	{#if !frame.dotsBehind}
		<g>
			{#each frame.dots as dot, i (i)}
				{#if dot.d}
					<path
						d={dot.d}
						transform={`translate(${dot.x} ${dot.y}) rotate(${dot.rot ?? 0}) scale(${R})`}
						fill={dotFill(dot)}
						opacity={dot.opacity}
					/>
				{:else}
					<circle cx={dot.x} cy={dot.y} r={dot.r} fill={dotFill(dot)} opacity={dot.opacity} />
				{/if}
			{/each}
		</g>
	{/if}

	{#if frame.notif}
		<circle cx={frame.notif.x} cy={frame.notif.y} r={frame.notif.r} fill={NOTIF_BLUE} />
	{/if}

	<!-- Front half of the orbit rings. -->
	<g fill="none" stroke-linecap="round">
		{#each frame.arcs as arc (`f${arc.id}`)}
			<path
				d={arc.front}
				stroke={`url(#${uid}-${arc.id})`}
				stroke-width={arc.width}
				opacity={arc.opacity}
			/>
		{/each}
	</g>
</svg>
