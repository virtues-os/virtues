<!--
	ChapterLifeline.svelte

	The lifeline as a plate, cropped to its life-level view: one wire from α
	to Ω, a life's chapters as boxes on it, their names in the dimension lane
	above, the age ruler below, now marked. Wider than the column it sits in.

	It draws whatever `life` it is handed (see life.ts). The interview opening
	hands it nothing and gets the repo's fictional example — enough to show
	what a partition of a life LOOKS like before someone writes theirs. The
	interview's close hands it the person's own chapters (ChapterLifelineLive),
	so the shape they saw as an example comes back drawn from their answers.

	Ported from the "Alpha to Omega" prototype (the full zoomable plate lives
	there; this is the static crop).
-->

<script lang="ts">
	import { FICTIONAL_LIFE, YR, type Life } from "./life";

	interface Props {
		life?: Life;
	}
	let { life = FICTIONAL_LIFE }: Props = $props();

	/* The plate's palette — one muted color per chapter, cycled. */
	const CH_COLORS = ["#B07514", "#2E6B43", "#1E4E8C", "#1E3159", "#6C7185", "#7E5A2E"];

	// ── geometry: the prototype's sheet, cropped to one static window ──
	const PX0 = 60, PX1 = 1220;
	/* Vertical geometry, top to bottom: the name lane (two rows of names
	   above its hairline), the boxes on the wire, the age ruler, and the
	   now-word below it. Kept tight: the lane sits just above the boxes it
	   measures, and the sheet's height is the content's, no dead band. */
	const BH = 16;              // box half-height
	const BASE = 90;            // the wire
	const DIM = BASE - BH - 20; // the dimension lane's hairline
	const SHEET_H = BASE + BH + 60;
	const ROW_H = 14;

	interface Label { cx: number; text: string; tight: boolean; row: number; planned: boolean; unnamed: boolean }
	interface Tick { t: number; year: number; age: number | null; major: boolean }
	/* Everything the template draws, laid out from the life at hand. The
	   origin of the scale is birth when known, else the first chapter; now
	   sits at 72% of the sheet, as the prototype boots. */
	const L = $derived.by(() => {
		const { now, chapters, planned, stories } = life;
		const origin = life.birth ?? chapters[0]?.t0 ?? now - 30 * YR;
		const LO = origin;
		const HI = origin + Math.max(now - origin, YR) / 0.72;
		const X = (t: number) => PX0 + ((t - LO) / (HI - LO)) * (PX1 - PX0);

		/* The ruler. With a birth known, birthday ticks read in both
		   coordinate systems — the world's calendar above, the life's age
		   below, every fifth labeled. Without one there is no honest zero
		   for an age, so the ruler is the calendar alone: a tick each New
		   Year, the years divisible by five labeled. */
		const ticks: Tick[] = [];
		if (life.birth !== null) {
			for (let age = 1; age * YR < HI - origin; age++) {
				const d = new Date(origin);
				d.setFullYear(d.getFullYear() + age);
				ticks.push({ t: d.getTime(), year: d.getFullYear(), age, major: age % 5 === 0 });
			}
		} else {
			for (let y = new Date(origin).getFullYear() + 1; ; y++) {
				const t = new Date(y, 0, 1).getTime();
				if (t > HI) break;
				ticks.push({ t, year: y, age: null, major: y % 5 === 0 });
			}
		}
		const ages = ticks.filter((a) => X(a.t) >= PX0 && X(a.t) <= PX1);

		// The planned chapter's span, clamped to the sheet.
		const plannedX0 = planned ? X(planned.t0) : 0;
		const plannedX1 = planned ? Math.min(X(planned.t1), PX1) : 0;

		/* The dimension lane's labels, laid out so none collide. A name longer
		   than its box is set tight; a name that would still run into the one
		   before it on the lane steps up a row. Widths are estimated from the
		   serif's average advance (about 0.5em at these sizes, in sentence
		   case) plus a little slack, in viewBox px. */
		const spans = [
			...chapters.map((c) => ({
				x0: X(c.t0),
				x1: X(c.t1 ?? now),
				text: c.label ?? "unnamed",
				planned: false,
				unnamed: c.label === null,
			})),
			...(planned ? [{ x0: plannedX0, x1: plannedX1, text: planned.label, planned: true, unnamed: false }] : []),
		];
		const width = (text: string, tight: boolean) =>
			text.length * (tight ? 11 * 0.5 * 1.08 : 13.5 * 0.5 * 1.1);
		const ends = [-Infinity, -Infinity];
		const labels: Label[] = spans.map(({ x0, x1, text, planned, unnamed }) => {
			const tight = width(text, false) > x1 - x0;
			const w = width(text, tight);
			const cx = (x0 + x1) / 2;
			const row = cx - w / 2 < ends[0] + 10 ? 1 : 0;
			ends[row] = cx + w / 2;
			return { cx, text, tight, row, planned, unnamed };
		});

		const boxes = chapters.map((c, ci) => ({
			c,
			x0: X(c.t0),
			x1: X(c.t1 ?? now),
			color: CH_COLORS[ci % CH_COLORS.length],
		}));

		// The stories: a dot on the wire, named beneath it inside the box.
		const marks = stories.filter((st) => st.t <= now).map((st) => ({ x: X(st.t), label: st.label }));

		return { X, now, ages, plannedX0, plannedX1, labels, boxes, marks, planned };
	});

	let readout = $state<string | null>(null);
</script>

<figure class="lifeline">
	<svg viewBox={`0 0 1280 ${SHEET_H}`} role="img" aria-label={life.ariaLabel}>
		<!-- the wire IS the life: it begins at α and runs toward Ω -->
		<line x1={PX0} y1={BASE} x2={PX1} y2={BASE} class="wire" />
		<circle cx={PX0} cy={BASE} r="3" class="alpha-dot" />
		<text x={PX0 - 14} y={BASE + 5} text-anchor="end" class="t-omega">α</text>
		<text x={PX1 + 14} y={BASE + 5} text-anchor="start" class="t-omega">Ω</text>

		<!-- the age ruler: years above the ticks, the life's age below, the
		     age row named at its left so "5, 10, 15" is not a puzzle -->
		{#if life.birth !== null}
			<text x={PX0 - 14} y={BASE + BH + 30} text-anchor="end" class="t-age t-age-name">age</text>
		{/if}
		{#each L.ages as a (a.t)}
			<line x1={L.X(a.t)} y1={BASE + BH + 14} x2={L.X(a.t)} y2={BASE + BH + 19} class="tick" class:fut={a.t > L.now} />
			{#if a.major}
				<text x={L.X(a.t)} y={BASE + BH + 10} text-anchor="middle" class="t-age" class:fut={a.t > L.now}>{a.year}</text>
				{#if a.age !== null}
					<text x={L.X(a.t)} y={BASE + BH + 30} text-anchor="middle" class="t-age" class:fut={a.t > L.now}>{a.age}</text>
				{/if}
			{/if}
		{/each}

		<!-- chapters: boxes on the wire, coverage inside, names in the lane above -->
		{#each L.boxes as b, ci (b.c.t0)}
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<g
				class="blk"
				class:unnamed={b.c.label === null}
				onmouseenter={() => (readout = `${b.c.label ?? "An unnamed stretch"} — ${b.c.ep}`)}
				onmouseleave={() => (readout = null)}
			>
				<rect x={b.x0} y={BASE - BH} width={b.x1 - b.x0} height={BH * 2} class="box" style={`--ch:${b.color}`} />
				<!-- the dimension lane: a hairline with end ticks, the drafted name above -->
				<line x1={b.x0 + 1} y1={DIM} x2={b.c.t1 === null ? b.x1 : b.x1 - 1} y2={DIM} class="dim" />
				<line x1={b.x0 + 1} y1={DIM} x2={b.x0 + 1} y2={DIM + 5} class="dim" />
				{#if b.c.t1 !== null}
					<line x1={b.x1 - 1} y1={DIM} x2={b.x1 - 1} y2={DIM + 5} class="dim" />
				{/if}
			</g>
		{/each}

		<!-- the stories: a dot on the wire, named beneath it inside its chapter -->
		{#each L.marks as m (m.x)}
			<circle cx={m.x} cy={BASE} r="2.4" class="story-dot" />
			<text x={m.x} y={BASE + BH - 4} text-anchor="middle" class="t-story">{m.label}</text>
		{/each}

		<!-- the planned chapter: drafted, not lived — dashed and quiet. The
		     one red on the plate is now. -->
		{#if L.planned}
			<line x1={L.plannedX0} y1={DIM} x2={L.plannedX1} y2={DIM} class="dim planned" />
		{/if}

		<!-- the names, on the lane or one row up when they would collide -->
		{#each L.labels as l, li (li)}
			<text
				x={l.cx}
				y={DIM - 7 - l.row * ROW_H}
				text-anchor="middle"
				class="t-dim"
				class:t-dim-tight={l.tight}
				class:planned-t={l.planned}
				class:unnamed-t={l.unnamed}
			>
				{l.text}
			</text>
		{/each}

		<!-- now: the one claret vertical. It runs from the boxes down through
		     the ruler and is named below it, so it never cuts through a
		     chapter's name in the lane above. -->
		<line x1={L.X(L.now)} y1={BASE - BH - 8} x2={L.X(L.now)} y2={BASE + BH + 38} class="now" />
		<text x={L.X(L.now)} y={BASE + BH + 50} text-anchor="middle" class="t-now">now</text>
	</svg>
	<figcaption class="readout" class:idle={!readout}>
		{readout ?? life.caption}
	</figcaption>
</figure>

<style>
	.lifeline {
		/* A plate wants a full measure of air from the prose on both sides;
		   at the paragraph's own spacing it read as part of the paragraph. */
		margin: 2.5rem 0 2.75rem;
		/* Wider than the column it sits in: the chat scroller is the size
		   container (ChatView sets container-type on it), so the plate takes
		   up to 72rem of it, less a gutter, and centers on the column. The
		   column's paint containment is relaxed for this one message so the
		   overhang is drawn, not clipped. On a narrow scroller this collapses
		   to the column width. */
		width: min(72rem, calc(100cqw - 3rem));
		max-width: none;
		position: relative;
		left: 50%;
		transform: translateX(-50%);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 1.5rem 1.25rem 0.875rem;
		background: var(--color-background);
	}


	svg {
		display: block;
		width: 100%;
		height: auto;
	}

	.wire {
		stroke: var(--color-foreground);
		stroke-width: 1.2;
	}

	.alpha-dot {
		fill: var(--color-foreground);
	}

	.t-omega {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 20px;
		fill: var(--color-foreground);
	}

	.tick {
		stroke: var(--color-foreground);
		stroke-opacity: 0.45;
		stroke-width: 0.6;
	}

	.tick.fut {
		stroke-opacity: 0.22;
	}

	/* Every word on the plate is set in the book's serif, roman, never
	   bold — an engraving's labels, not a chart's. The mono it wore made
	   the one picture in the room look like a dashboard pasted into it. */
	.t-age {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 11.5px;
		fill: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}
	.t-age-name {
		font-size: 10.5px;
		letter-spacing: 0.06em;
	}

	.t-age.fut {
		opacity: 0.5;
	}

	.box {
		fill: var(--ch);
		fill-opacity: 0.07;
		stroke: var(--color-foreground);
		stroke-opacity: 0.35;
		stroke-width: 0.8;
	}

	.blk.unnamed .box {
		stroke-dasharray: 3 3;
		fill-opacity: 0.03;
	}

	.unnamed-t {
		fill: var(--color-foreground-subtle);
	}

	.blk:hover .box {
		fill-opacity: 0.14;
		stroke-opacity: 0.6;
	}

	.dim {
		stroke: var(--color-foreground-subtle);
		stroke-width: 0.7;
	}

	.dim.planned {
		stroke: var(--color-foreground-subtle);
		stroke-dasharray: 3 4;
	}

	.t-dim {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 13.5px;
		letter-spacing: 0.01em;
		fill: var(--color-foreground-muted);
	}

	.t-dim-tight {
		font-size: 11px;
	}

	.planned-t {
		fill: var(--color-foreground-subtle);
	}

	.story-dot {
		fill: var(--color-foreground);
	}

	.t-story {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 10.5px;
		fill: var(--color-foreground-muted);
	}

	.now {
		stroke: #9a2b2e;
		stroke-width: 1;
	}

	.t-now {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 11px;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		fill: #9a2b2e;
	}

	.readout {
		margin: 0.75rem 0.25rem 0;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		min-height: 1.2em;
	}

	.readout.idle {
		color: var(--color-foreground-subtle);
	}
</style>
