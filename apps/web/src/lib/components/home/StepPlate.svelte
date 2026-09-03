<!--
	StepPlate.svelte — the folio's right page.

	One letterpress-style plate per getting-started step: a fine-line figure
	in the ∴ register above, a lowercase mono caption below, numbered like a
	series ("no. 05 / in your own words / …"). The walk on the left tells you
	where you are; the plate says what this step IS, the way a cloud atlas
	pairs the photograph with the hand-drawn diagram.

	Line work only — strokes in ink tokens, one claret accent at most — so it
	holds in both themes and never pretends to be a photograph. The figures
	are diagrams of the product's own constructs (streams into the server,
	the lifeline wire, a narrated page at dawn), not decoration.
-->

<script lang="ts">
	let { step, census = null }: {
		/** The active step id, or "settled" when everything is done. */
		step: string;
		/** For the settled plate's caption. */
		census?: { total: number; earliest: string | null } | null;
	} = $props();

	const PLATES: Record<string, { no: string; name: string; gloss: string }> = {
		letter: { no: "01", name: "the founder's letter", gloss: "why any of this exists" },
		introductions: { no: "02", name: "introductions", gloss: "two names, thirty seconds" },
		connect: { no: "03", name: "connect your world", gloss: "the record begins" },
		signin: { no: "04", name: "your virtues account", gloss: "the models it writes with" },
		interview: { no: "05", name: "in your own words", gloss: "chapters · beliefs · days" },
		first_day: { no: "06", name: "your first day", gloss: "tomorrow morning" },
		further: { no: "07", name: "go further", gloss: "applets · the manual" },
		settled: { no: "∴", name: "the record so far", gloss: "" },
	};
	const plate = $derived(PLATES[step] ?? PLATES.letter);

	const settledGloss = $derived.by(() => {
		if (step !== "settled" || !census) return "";
		const traces = census.total.toLocaleString();
		const since = census.earliest
			? new Date(census.earliest).toLocaleDateString(undefined, { month: "long", year: "numeric" })
			: null;
		return since ? `${traces} traces, since ${since}` : `${traces} traces`;
	});
</script>

<div class="plate" aria-hidden="true">
	{#key step}
		<div class="plate-in">
			<svg viewBox="0 0 220 200" class="figure">
				{#if step === "letter"}
					<!-- the mark itself, set like a colophon: therefore -->
					<circle cx="110" cy="72" r="9" class="ink-fill" />
					<circle cx="82" cy="118" r="9" class="ink-fill" />
					<circle cx="138" cy="118" r="9" class="ink-fill" />
					<line x1="66" y1="156" x2="154" y2="156" class="hair" />
				{:else if step === "introductions"}
					<!-- two names, one line between them -->
					<circle cx="66" cy="96" r="16" class="ink" />
					<circle cx="154" cy="96" r="16" class="ink" />
					<line x1="84" y1="96" x2="136" y2="96" class="hair" />
					<line x1="46" y1="136" x2="86" y2="136" class="hair" />
					<line x1="134" y1="136" x2="174" y2="136" class="hair" />
				{:else if step === "connect"}
					<!-- the streams of a life, flowing into the server -->
					{#each [52, 68, 84, 116, 132, 148] as y, i (y)}
						<path d={`M 24 ${y} C 80 ${y}, 96 ${100 + (i - 2.5) * 4}, 138 ${100 + (i - 2.5) * 4}`} class="hair" />
					{/each}
					<rect x="138" y="76" width="42" height="48" class="ink" />
					<line x1="146" y1="112" x2="172" y2="112" class="hair" />
				{:else if step === "signin"}
					<!-- the seal: the mark, impressed -->
					<circle cx="110" cy="96" r="38" class="ink" />
					<circle cx="110" cy="86" r="4.5" class="ink-fill" />
					<circle cx="97" cy="107" r="4.5" class="ink-fill" />
					<circle cx="123" cy="107" r="4.5" class="ink-fill" />
					<path d="M 84 124 C 96 138, 124 138, 136 124" class="hair" />
				{:else if step === "interview"}
					<!-- the lifeline, miniature: your chapters will hang on this wire -->
					<line x1="30" y1="100" x2="190" y2="100" class="ink" />
					<circle cx="30" cy="100" r="2.5" class="ink-fill" />
					<text x="18" y="104" class="glyph">α</text>
					<text x="196" y="104" class="glyph">Ω</text>
					<rect x="42" y="90" width="34" height="20" class="ink" />
					<rect x="76" y="90" width="26" height="20" class="ink" />
					<rect x="102" y="90" width="38" height="20" class="ink" />
					<rect x="140" y="90" width="18" height="20" class="ink" />
					<line x1="158" y1="72" x2="158" y2="122" class="claret" />
					<line x1="42" y1="132" x2="158" y2="132" class="hair" />
				{:else if step === "first_day"}
					<!-- a narrated page, at dawn -->
					<path d="M 60 66 A 50 50 0 0 1 160 66" class="hair" />
					<line x1="110" y1="30" x2="110" y2="40" class="hair" />
					<line x1="66" y1="44" x2="73" y2="51" class="hair" />
					<line x1="154" y1="44" x2="147" y2="51" class="hair" />
					<rect x="74" y="66" width="72" height="96" class="ink" />
					{#each [84, 96, 108, 120, 132] as y (y)}
						<line x1="84" y1={y} x2={y === 132 ? 116 : 136} y2={y} class="hair" />
					{/each}
				{:else if step === "further"}
					<!-- the door, ajar -->
					<rect x="76" y="48" width="60" height="108" class="ink" />
					<path d="M 76 48 L 118 62 L 118 174 L 76 156 Z" class="ink paper-fill" />
					<circle cx="110" cy="118" r="2.5" class="ink-fill" />
					<line x1="146" y1="156" x2="176" y2="156" class="hair" />
				{:else}
					<!-- settled: the wire again, now carrying a record -->
					<line x1="30" y1="100" x2="190" y2="100" class="ink" />
					<circle cx="30" cy="100" r="2.5" class="ink-fill" />
					<text x="18" y="104" class="glyph">α</text>
					<text x="196" y="104" class="glyph">Ω</text>
					{#each [44, 62, 80, 104, 122, 140, 158] as x, i (x)}
						<rect x={x} y={94 - (i % 3) * 2} width="10" height={12 + (i % 3) * 4} class="ink-soft-fill" />
					{/each}
					<line x1="166" y1="76" x2="166" y2="118" class="claret" />
				{/if}
			</svg>
			<div class="caption mono">
				<span class="no">no. {plate.no}</span>
				<span>{plate.name}</span>
				{#if step === "settled"}
					{#if settledGloss}<span class="gloss">{settledGloss}</span>{/if}
				{:else if plate.gloss}
					<span class="gloss">{plate.gloss}</span>
				{/if}
			</div>
		</div>
	{/key}
</div>

<style>
	.plate {
		margin: 0;
		border: 1px solid var(--color-border);
		background: var(--color-surface-elevated);
		padding: 10px;
	}

	.plate-in {
		border: 1px solid color-mix(in srgb, var(--color-border) 60%, transparent);
		padding: 26px 18px 18px;
		display: flex;
		flex-direction: column;
		gap: 14px;
		animation: plate-in 0.45s ease both;
	}

	/* from-only keyframes, per the house rule */
	@keyframes plate-in {
		from {
			opacity: 0;
			transform: translateY(4px);
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.plate-in {
			animation: none;
		}
	}

	.figure {
		display: block;
		width: 100%;
		height: auto;
	}

	.ink {
		fill: none;
		stroke: var(--color-foreground);
		stroke-width: 1.1;
	}

	.paper-fill {
		fill: var(--color-surface-elevated);
	}

	.ink-fill {
		fill: var(--color-foreground);
	}

	.ink-soft-fill {
		fill: var(--color-foreground);
		fill-opacity: 0.25;
	}

	.hair {
		fill: none;
		stroke: var(--color-foreground);
		stroke-opacity: 0.45;
		stroke-width: 0.8;
	}

	.claret {
		stroke: #9a2b2e;
		stroke-width: 1;
	}

	.glyph {
		font-family: var(--font-serif, Georgia, serif);
		font-size: 11px;
		fill: var(--color-foreground);
	}

	.caption {
		display: flex;
		flex-direction: column;
		gap: 3px;
		font-size: 11px;
		letter-spacing: 0.02em;
		color: var(--color-foreground-muted);
	}

	.caption .no {
		color: var(--color-foreground-subtle);
	}

	.caption .gloss {
		color: var(--color-foreground-subtle);
	}
</style>
