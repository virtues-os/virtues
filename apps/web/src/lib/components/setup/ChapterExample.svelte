<!--
	The example life, small and still: what "See an example" opens in the
	Chapters step. The same five chapters the intro draws, so it is the
	example people have already met. Drawn on a line where there is room,
	and as a list of ages where there isn't (a phone, where the person's
	own chapters are a list too).
-->
<script lang="ts">
	import { SAMPLE, SAMPLE_SPAN } from "./chapterExample";

	let { list = false }: { list?: boolean } = $props();

	const W = 320;
	const PAD_L = 8;
	const PAD_R = 16;
	const LINE_Y = 46;
	const BAND_H = 18;
	const LABEL = 13;
	const inner = W - PAD_L - PAD_R;
	const x = (age: number) => PAD_L + (age / SAMPLE_SPAN) * inner;

	/** Names alternate above and below the line so neighbors never meet; one
	 *  that would run past the end is set against it. */
	const labels = SAMPLE.map((b, i) => {
		const at = x(b.from) + 3;
		const end = at + b.title.length * LABEL * 0.5 > PAD_L + inner;
		return {
			x: end ? PAD_L + inner - 1 : at,
			anchor: end ? "end" : "start",
			y: i % 2 === 0 ? LINE_Y - BAND_H / 2 - 6 : LINE_Y + BAND_H / 2 + LABEL + 2,
		};
	});

	const ages = (b: { from: number; to: number }, last: boolean) =>
		b.from === 0 ? `Birth to ${b.to}` : last ? `${b.from} to now` : `${b.from} to ${b.to}`;
</script>

{#if list}
	<ol class="rows">
		{#each SAMPLE as b, i (b.title)}
			<li>
				<span class="name">{b.title}</span>
				<span class="ages">{ages(b, i === SAMPLE.length - 1)}</span>
			</li>
		{/each}
	</ol>
{:else}
	<svg viewBox="0 0 {W} 92" width="100%" role="img" aria-label="An example life in five chapters: {SAMPLE.map((b) => b.title).join(', ')}">
		{#each SAMPLE as b, i (b.title)}
			<rect
				class="band"
				class:alt={i % 2 === 1}
				x={x(b.from) + 0.5}
				y={LINE_Y - BAND_H / 2}
				width={Math.max(0, x(b.to) - x(b.from) - 1)}
				height={BAND_H}
				rx="2"
			/>
			{#if i > 0}
				<line class="turn" x1={x(b.from)} x2={x(b.from)} y1={LINE_Y - BAND_H / 2 - 4} y2={LINE_Y + BAND_H / 2 + 4} />
			{/if}
			<text class="label" x={labels[i].x} y={labels[i].y} text-anchor={labels[i].anchor}>{b.title}</text>
		{/each}
		<rect class="line" x={PAD_L} y={LINE_Y - 0.5} width={inner} height="1" />
		<circle class="origin" cx={PAD_L} cy={LINE_Y} r="3" />
		<path class="arrow" d="M {PAD_L + inner - 1} {LINE_Y - 4} L {PAD_L + inner + 6} {LINE_Y} L {PAD_L + inner - 1} {LINE_Y + 4}" />
	</svg>
{/if}

<style>
	svg {
		display: block;
		overflow: visible;
	}
	.band {
		fill: color-mix(in srgb, var(--color-primary) 10%, transparent);
	}
	.band.alt {
		fill: color-mix(in srgb, var(--color-primary) 18%, transparent);
	}
	.turn,
	.line,
	.origin,
	.arrow {
		stroke: var(--color-primary);
		fill: none;
	}
	.line,
	.origin {
		fill: var(--color-primary);
		stroke: none;
	}
	.arrow {
		stroke-width: 1.25;
	}
	.label {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 13px;
		fill: var(--color-foreground);
	}

	.rows {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.rows li {
		display: flex;
		justify-content: space-between;
		gap: 16px;
		padding: 8px 0;
		border-bottom: 1px solid var(--color-border);
	}
	.rows li:last-child {
		border-bottom: 0;
	}
	.name {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 16px;
		color: var(--color-foreground);
	}
	.ages {
		font-size: 13px;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}
</style>
