<!--
	ChapterLifeline.svelte

	The lifeline as a plate: one wire from α to Ω, a life's chapters as boxes
	on it, their names in the dimension lane above, the ruler below, now
	marked. It draws whatever `life` it is handed (see life.ts) — the
	interview's opening hands it the repo's fictional example, its close
	hands it the person's own chapters (ChapterLifelineLive).

	MEASURED, NOT SCALED (2026-09-15). The sheet used to be a fixed 1280-unit
	viewBox rendered into at most 1112 CSS px and as little as 700 in a
	narrow column, so every declared size shrank with the container: 13.5px
	names set at 7px, an 11.5px ruler at 6px. The plate was not badly typeset,
	it was photographed from far away. The viewBox is the element's own width
	now — one unit is one pixel at every size — so the numbers below are real,
	and a narrow plate REFLOWS instead of shrinking.

	Two ways to name the chapters, chosen by how much room the names need:

	  the lane     names over their own spans, three rows, and a leader
	               elbow drawn to any name that had to move to fit. This is
	               the drafting convention for a dimension whose text will
	               not sit on its own line — and it replaced shrinking the
	               name, which made the four hardest names the smallest.
	  numerals     past the lane's capacity: the boxes carry 1..n and the
	               names flow in a legend under the plate. The only thing
	               that survives someone naming twenty chapters, or a
	               700px column.
-->

<script lang="ts">
	import { FICTIONAL_LIFE, YR, type Life } from "./life";

	interface Props {
		life?: Life;
	}
	let { life = FICTIONAL_LIFE }: Props = $props();

	// ── the sheet ──────────────────────────────────────────────────────────
	/** The element's own width, so one viewBox unit is one CSS pixel. */
	let w = $state(0);
	/** Bumped once the serif has loaded: text measured in a fallback face
	 *  lays the lane out against widths that are about to change. */
	let fontsReady = $state(0);

	$effect(() => {
		if (typeof document === "undefined" || !document.fonts) return;
		let alive = true;
		void document.fonts.ready.then(() => {
			if (alive) fontsReady += 1;
		});
		return () => {
			alive = false;
		};
	});

	const SIDE = 56; // the margin α and Ω hang in
	const NAME_PX = 15;
	const GAP = 14; // clear air between two names on one row
	const ROWS = 3;
	const ROW_H = 24;
	const BH = 18; // box half-height

	/** Real text width, from the face actually in use. */
	let ctx: CanvasRenderingContext2D | null = null;
	function textWidth(text: string, px: number): number {
		if (typeof document === "undefined") return text.length * px * 0.5;
		if (!ctx) ctx = document.createElement("canvas").getContext("2d");
		if (!ctx) return text.length * px * 0.5;
		const face = getComputedStyle(document.documentElement).getPropertyValue("--font-serif").trim();
		ctx.font = `${px}px ${face || "Georgia, serif"}`;
		return ctx.measureText(text).width;
	}

	interface Label {
		cx: number;
		mid: number;
		text: string;
		row: number;
		leader: boolean;
		planned: boolean;
		unnamed: boolean;
	}
	interface Tick { t: number; year: number; age: number | null; major: boolean }
	interface Mark { x: number; label: string; row: number }

	const L = $derived.by(() => {
		fontsReady; // re-lay out when the real face arrives
		const { now, chapters, planned, stories } = life;
		const PX0 = SIDE;
		const PX1 = Math.max(SIDE + 120, w - SIDE);
		const origin = life.birth ?? chapters[0]?.t0 ?? now - 30 * YR;
		const LO = origin;
		/* How much future to draw. A planned chapter needs room ahead of now;
		   nothing else does, and a real life has none — it used to spend 28%
		   of the plate on an empty line while the recent chapters, which are
		   always the shortest, fought over what was left. */
		const HI = origin + Math.max(now - origin, YR) / (planned ? 0.9 : 0.96);
		const X = (t: number) => PX0 + ((t - LO) / (HI - LO)) * (PX1 - PX0);

		/* The ruler. With a birth known, birthday ticks read in both
		   coordinate systems — the world's calendar above, the life's age
		   below, every fifth labeled. Without one there is no honest zero for
		   an age, so the ruler is the calendar alone. */
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

		const plannedX0 = planned ? X(planned.t0) : 0;
		const plannedX1 = planned ? Math.min(X(planned.t1), PX1) : 0;

		const boxes = chapters.map((c, i) => ({
			c,
			n: i + 1,
			x0: X(c.t0),
			x1: X(c.t1 ?? now),
		}));

		const spans = [
			...chapters.map((c) => ({
				mid: (X(c.t0) + X(c.t1 ?? now)) / 2,
				text: c.label ?? "unnamed",
				planned: false,
				unnamed: c.label === null,
			})),
			...(planned
				? [{ mid: (plannedX0 + plannedX1) / 2, text: planned.label, planned: true, unnamed: false }]
				: []),
		];

		/* Numeral mode, decided before the lane is laid out: if every name
		   laid end to end with its air will not fit across three rows, no
		   arrangement of them will. */
		const widths = spans.map((s) => textWidth(s.text, NAME_PX));
		const need = widths.reduce((a, b) => a + b, 0) + spans.length * GAP;
		const numerals = w > 0 && need > ROWS * (PX1 - PX0);

		/* The lane. Each name wants its own midpoint; it takes the first row
		   where it clears what is already there, and is nudged along that row
		   if no row is free. A name that ends up anywhere but over its own
		   span gets a leader drawn back to it. The old rule tested every row
		   against `ends[0]`, so a name pushed to the second row was placed
		   blind and cleared its neighbour only by luck. */
		const ends = Array<number>(ROWS).fill(-Infinity);
		const laid: Label[] = numerals
			? []
			: spans.map((s, i) => {
					const tw = widths[i];
					const ideal = Math.min(Math.max(s.mid, PX0 + tw / 2), PX1 - tw / 2);
					let row = ends.findIndex((e) => ideal - tw / 2 >= e + GAP);
					let cx = ideal;
					if (row === -1) {
						row = ends.indexOf(Math.min(...ends));
						cx = ends[row] + GAP + tw / 2;
					}
					ends[row] = cx + tw / 2;
					return {
						cx,
						mid: s.mid,
						text: s.text,
						row,
						leader: Math.abs(cx - s.mid) > 3 || row > 0,
						planned: s.planned,
						unnamed: s.unnamed,
					};
				});
		/* The nudge can push the last name off the end of the plate, which the
		   sum above does not catch: it knows the names FIT across three rows,
		   not that they fit in the order the spans demand. If anything ended
		   up outside the sheet, the lane has failed and the whole plate falls
		   back to numerals — a name half off the page is worse than a legend. */
		const spilled = laid.some((l, i) => l.cx - widths[i] / 2 < PX0 - 0.5 || l.cx + widths[i] / 2 > PX1 + 0.5);
		const useNumerals = numerals || spilled;
		const labels: Label[] = useNumerals ? [] : laid;

		/* The stories: a dot on the wire, named BELOW the boxes rather than
		   inside them, on two rows so two close together do not overlap. */
		const storyEnds = [-Infinity, -Infinity];
		const marks: Mark[] = stories
			.filter((st) => st.t <= now)
			.map((st) => {
				const x = X(st.t);
				const tw = textWidth(st.label, STORY_PX);
				const row = x - tw / 2 >= storyEnds[0] + GAP ? 0 : 1;
				storyEnds[row] = x + tw / 2;
				return { x, label: st.label, row };
			});

		// ── the vertical, derived from what is actually drawn ──
		const laneH = useNumerals ? 0 : ROWS * ROW_H;
		const DIM = laneH + (useNumerals ? 14 : 26);
		const BASE = DIM + 24;
		const boxBottom = BASE + BH;
		const storyH = marks.length ? (marks.some((m) => m.row === 1) ? 44 : 26) : 4;
		const rulerTop = boxBottom + storyH;
		const yearsY = rulerTop + 16;
		const tickY0 = rulerTop + 22;
		const tickY1 = rulerTop + 30;
		const agesY = rulerTop + 46;
		const rulerBottom = life.birth !== null ? agesY : tickY1;
		const H = rulerBottom + 30;

		const legend = useNumerals
			? spans.map((s, i) => ({
					n: i + 1,
					text: s.text,
					years: yearsOf(i, s.planned),
					planned: s.planned,
				}))
			: [];

		return {
			X, now, ages, plannedX0, plannedX1, labels, boxes, marks, planned,
			numerals: useNumerals, legend,
			PX0, PX1, DIM, BASE, boxBottom, yearsY, tickY0, tickY1, agesY, rulerBottom, H,
		};
	});

	const STORY_PX = 13;

	/** "1997 – 2003" for the legend, from the chapter at that index. */
	function yearsOf(i: number, isPlanned: boolean): string {
		const src = isPlanned ? life.planned : life.chapters[i];
		if (!src) return "";
		const y0 = new Date(src.t0).getFullYear();
		const end = "t1" in src ? src.t1 : null;
		const y1 = end === null || end === undefined ? "now" : new Date(end).getFullYear();
		return `${y0} – ${y1}`;
	}
</script>

<figure class="lifeline">
	<div class="sheet" bind:clientWidth={w}>
		{#if w > 0}
			<svg viewBox={`0 0 ${w} ${L.H}`} role="img" aria-label={life.ariaLabel}>
				<!-- the wire IS the life: it begins at α and runs toward Ω. Without
				     a birth date there is no α to begin at — the scale starts at
				     the first chapter someone named, which is not the same thing. -->
				<line x1={L.PX0} y1={L.BASE} x2={L.PX1} y2={L.BASE} class="wire" />
				{#if life.birth !== null}
					<circle cx={L.PX0} cy={L.BASE} r="3" class="alpha-dot" />
					<text x={L.PX0 - 16} y={L.BASE + 6} text-anchor="end" class="t-omega">α</text>
				{/if}
				<text x={L.PX1 + 16} y={L.BASE + 6} text-anchor="start" class="t-omega">Ω</text>

				<!-- the ruler: calendar above the ticks, the life's age below -->
				{#each L.ages as a (a.t)}
					<line x1={L.X(a.t)} y1={L.tickY0} x2={L.X(a.t)} y2={L.tickY1} class="tick" class:fut={a.t > L.now} />
					{#if a.major}
						<text x={L.X(a.t)} y={L.yearsY} text-anchor="middle" class="t-age" class:fut={a.t > L.now}>{a.year}</text>
						{#if a.age !== null}
							<!-- the first age carries the word, so a column of bare
							     numerals under the years is not a puzzle -->
							<text x={L.X(a.t)} y={L.agesY} text-anchor="middle" class="t-age" class:fut={a.t > L.now}>
								{a.age === 5 ? `age ${a.age}` : a.age}
							</text>
						{/if}
					{/if}
				{/each}

				<!-- chapters: boxes on the wire, the dimension line above each -->
				{#each L.boxes as b (b.c.t0)}
					<rect
						x={b.x0}
						y={L.BASE - BH}
						width={Math.max(b.x1 - b.x0, 1)}
						height={BH * 2}
						class="box"
						class:alt={b.n % 2 === 0}
						class:unnamed={b.c.label === null}
					/>
					{#if !L.numerals}
						<line x1={b.x0 + 1} y1={L.DIM} x2={b.c.t1 === null ? b.x1 : b.x1 - 1} y2={L.DIM} class="dim" />
						<line x1={b.x0 + 1} y1={L.DIM} x2={b.x0 + 1} y2={L.DIM + 5} class="dim" />
						{#if b.c.t1 !== null}
							<line x1={b.x1 - 1} y1={L.DIM} x2={b.x1 - 1} y2={L.DIM + 5} class="dim" />
						{/if}
					{:else}
						<text x={(b.x0 + b.x1) / 2} y={L.BASE + 5} text-anchor="middle" class="t-numeral">{b.n}</text>
					{/if}
				{/each}

				<!-- the planned chapter: drafted, not lived — dashed and quiet -->
				{#if L.planned && !L.numerals}
					<line x1={L.plannedX0} y1={L.DIM} x2={L.plannedX1} y2={L.DIM} class="dim planned" />
				{/if}

				<!-- the stories: a dot on the wire, named beneath the boxes -->
				{#each L.marks as m (m.x)}
					<circle cx={m.x} cy={L.BASE} r="2.6" class="story-dot" />
					<line x1={m.x} y1={L.boxBottom} x2={m.x} y2={L.boxBottom + 6 + m.row * 18} class="story-stem" />
					<text x={m.x} y={L.boxBottom + 18 + m.row * 18} text-anchor="middle" class="t-story">{m.label}</text>
				{/each}

				<!-- the names, and a leader to any that had to move to fit -->
				{#each L.labels as l, li (li)}
					{@const y = L.DIM - 12 - (2 - l.row) * ROW_H}
					{#if l.leader}
						<path
							d={`M ${l.mid} ${L.DIM - 4} L ${l.mid} ${y + 6} L ${l.cx} ${y + 6}`}
							class="leader"
							class:planned={l.planned}
						/>
					{/if}
					<text x={l.cx} y={y} text-anchor="middle" class="t-dim" class:planned-t={l.planned} class:unnamed-t={l.unnamed}>
						{l.text}
					</text>
				{/each}

				<!-- now: the one claret thing on the plate -->
				<line x1={L.X(L.now)} y1={L.BASE - BH - 10} x2={L.X(L.now)} y2={L.rulerBottom + 8} class="now" />
				<text x={L.X(L.now)} y={L.H - 8} text-anchor="middle" class="t-now">now</text>
			</svg>
		{/if}
	</div>

	{#if L.numerals}
		<!-- Past the lane's capacity: the names live here, in reading order. -->
		<ol class="legend">
			{#each L.legend as item (item.n)}
				<li class:planned={item.planned}>
					<span class="n">{item.n}</span>
					<span class="name">{item.text}</span>
					<span class="years">{item.years}</span>
				</li>
			{/each}
		</ol>
	{/if}
</figure>

<style>
	.lifeline {
		margin: 2.5rem 0 2.75rem;
		/* Wider than the column it sits in: the chat scroller is the size
		   container (ChatView sets container-type on it), so the plate takes
		   up to 72rem of it, less a gutter, and centers on the column. */
		width: min(72rem, calc(100cqw - 3rem));
		max-width: none;
		position: relative;
		left: 50%;
		transform: translateX(-50%);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 2rem 1.75rem 1.25rem;
		background: var(--color-background);
		box-sizing: border-box;
	}

	.sheet {
		width: 100%;
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
		font-size: 22px;
		fill: var(--color-foreground);
	}

	.tick {
		stroke: var(--color-foreground);
		stroke-opacity: 0.45;
		stroke-width: 0.8;
	}

	.tick.fut {
		stroke-opacity: 0.22;
	}

	/* Every word on the plate is the book's serif, roman, sentence case —
	   an engraving's labels, not a chart's. */
	.t-age {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 13px;
		fill: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.t-age.fut {
		opacity: 0.5;
	}

	/* One ink. Six hues at 7% opacity were invisible AS COLOR and still
	   spent six values on nothing; alternating two weights of the same ink
	   reads the partition at a glance and survives either theme. */
	.box {
		fill: var(--color-foreground);
		fill-opacity: 0.05;
		stroke: var(--color-foreground);
		stroke-opacity: 0.35;
		stroke-width: 0.8;
	}

	.box.alt {
		fill-opacity: 0.02;
	}

	.box.unnamed {
		stroke-dasharray: 3 3;
		fill-opacity: 0.015;
	}

	.dim {
		stroke: var(--color-foreground-subtle);
		stroke-width: 0.7;
	}

	.dim.planned {
		stroke: var(--color-foreground-subtle);
		stroke-dasharray: 3 4;
	}

	/* The elbow tying a displaced name back to the span it measures. */
	.leader {
		fill: none;
		stroke: var(--color-foreground-subtle);
		stroke-opacity: 0.55;
		stroke-width: 0.7;
	}

	.leader.planned {
		stroke-dasharray: 3 4;
	}

	.t-dim {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 15px;
		fill: var(--color-foreground-muted);
	}

	.planned-t,
	.unnamed-t {
		fill: var(--color-foreground-subtle);
	}

	.t-numeral {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 13px;
		fill: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}

	.story-dot {
		fill: var(--color-foreground);
	}

	.story-stem {
		stroke: var(--color-foreground);
		stroke-opacity: 0.3;
		stroke-width: 0.7;
	}

	.t-story {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 13px;
		fill: var(--color-foreground-muted);
	}

	.now {
		stroke: #9a2b2e;
		stroke-width: 1;
	}

	.t-now {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 12px;
		letter-spacing: 0.14em;
		text-transform: uppercase;
		fill: #9a2b2e;
	}

	.legend {
		list-style: none;
		margin: 1.25rem 0 0;
		padding: 0;
		columns: 18rem;
		column-gap: 2rem;
		font-family: var(--font-serif, Georgia, serif);
		font-size: 14px;
		line-height: 1.7;
		color: var(--color-foreground-muted);
	}

	.legend li {
		break-inside: avoid;
		display: flex;
		gap: 0.6rem;
		align-items: baseline;
	}

	.legend .n {
		min-width: 1.1em;
		text-align: right;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.legend .name {
		flex: 1;
	}

	.legend .years {
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
	}

	.legend li.planned .name {
		color: var(--color-foreground-subtle);
	}
</style>
