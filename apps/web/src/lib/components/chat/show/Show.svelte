<!--
	A `show` call, drawn where the model made it in the reply.

	The kinds are a fixed set, each its own component; the model chose one
	and wrote a query, and the box ran the query and sent the rows
	(virtues-core tools/show.rs). So this never renders anything the model
	wrote as markup, and every figure on it came out of the record.

	Unframed, like the rest of the box's turn (design.md, "Framed and unframed blocks"): no border
	and no ground of its own, a chart at the full width of the column.
-->
<script lang="ts">
	import { isKitMark, isShowOutput, type ShowOutput } from "./show";
	import ShowChart from "./ShowChart.svelte";
	import ShowMark from "./ShowMark.svelte";
	import ShowNumbers from "./ShowNumbers.svelte";
	import ShowTable from "./ShowTable.svelte";
	import ShowMap from "./ShowMap.svelte";
	import ShowTimeline from "./ShowTimeline.svelte";
	import ShowChoices from "./ShowChoices.svelte";
	import ShowSeries from "./ShowSeries.svelte";

	interface Props {
		part: { state?: string; input?: Record<string, unknown>; output?: unknown };
		/** The newest turn, finished: its reply buttons can be pressed. */
		active: boolean;
		/** The turn is still running, so a call without a result is in progress. */
		working: boolean;
		/** Send a reply button's words; false when the chat could not take them. */
		onChoose?: (text: string) => Promise<boolean>;
		/** Open the rows a figure was drawn from, where the chat can. */
		onOpenRows?: (ref: string) => void;
	}
	let { part, active, working, onChoose, onOpenRows }: Props = $props();

	const out = $derived(part.state === "output-available" && isShowOutput(part.output) ? (part.output as ShowOutput) : null);
	// A call the turn ended without answering (stopped, cut off) draws
	// nothing, rather than a shimmer that never resolves.
	const drawing = $derived(working && (part.state === "input-streaming" || part.state === "input-available"));
	const title = $derived(out ? out.title : typeof part.input?.title === "string" ? part.input.title : null);
	const columns = $derived(out?.columns ?? []);
	const rows = $derived(out?.rows ?? []);
	// While a call runs, its kind comes from the call: `chart` (the field the
	// tool offers, virtues_registry::chart_marks), or `kind` and `purpose` from
	// calls written before it. Numbers hold the short placeholder, so the
	// reply doesn't jump, and choices draw no placeholder at all.
	function kindOfCall(input: Record<string, unknown> | undefined): string | undefined {
		const chart = input?.chart ?? input?.purpose;
		// Figures with series stay in the sentence, so nothing holds space here.
		if (input?.series && typeof input.series === "object") return "series";
		if (typeof input?.kind === "string") return input.kind;
		if (chart === "choices") return "choices";
		if (chart === "in_sentence" || chart === "numbers") return "numbers";
		return undefined;
	}
	const kind = $derived(out?.kind ?? kindOfCall(part.input as Record<string, unknown> | undefined));
	// Figures that carry their rows open from the sentence; when none of them
	// can be found there, the plain row of figures is drawn here instead.
	let unplaced = $state(false);
	const inSentence = $derived(!!out && out.kind === "numbers" && !!out.series && Object.keys(out.series).length > 0 && !unplaced);
</script>

{#if out && inSentence}
	<ShowSeries {out} {working} {onOpenRows} onUnplaced={() => (unplaced = true)} />
{:else if out || (drawing && kind !== "choices" && kind !== "series")}
	<figure class="show" class:choices={kind === "choices"} aria-busy={drawing}>
		{#if title && kind !== "choices"}
			<figcaption class="title">{title}</figcaption>
		{/if}

		{#if !out}
			<div class="drawing" class:short={kind === "numbers"}></div>
		{:else if isKitMark(out.mark)}
			<ShowMark out={{ ...out, mark: out.mark }} {onOpenRows} />
		{:else if out.kind === "chart"}
			<ShowChart {columns} {rows} mark={out.mark === "line" ? "line" : "bar"} />
		{:else if out.kind === "numbers" && rows[0]}
			<ShowNumbers {columns} row={rows[0]} />
		{:else if out.kind === "table"}
			<ShowTable {columns} {rows} more={!!out.more} />
		{:else if out.kind === "map"}
			<ShowMap {rows} />
		{:else if out.kind === "timeline"}
			<ShowTimeline {rows} />
		{:else if out.kind === "choices"}
			<ShowChoices options={out.options ?? []} {active} {onChoose} />
		{/if}

		{#if out && out.kind !== "choices"}
			<div class="source">
				<span>{out.kind === "numbers" ? "From your records" : `From ${rows.length === 1 ? "1 row" : `${rows.length} rows`} of your records`}</span>
				{#if out.ref && onOpenRows && out.kind !== "table"}
					<button type="button" onclick={() => onOpenRows(out.ref!)}>See the rows</button>
				{/if}
			</div>
		{/if}
	</figure>
{/if}

<style>
	.show {
		margin: 1.25rem 0 1.5rem;
		min-width: 0;
	}
	.show.choices {
		margin: 0.5rem 0 1rem;
	}
	.title {
		font-family: var(--font-serif);
		font-weight: 400;
		font-size: 1.125rem;
		line-height: 1.35;
		color: var(--color-foreground);
		margin-bottom: 0.5rem;
	}
	.drawing {
		height: 200px;
		border-radius: var(--radius-xl);
		background: linear-gradient(
			90deg,
			var(--color-surface) 0%,
			var(--color-surface-elevated) 50%,
			var(--color-surface) 100%
		);
		background-size: 200% 100%;
		animation: drawing 1.4s ease-in-out infinite;
	}
	.drawing.short {
		height: 64px;
	}
	@keyframes drawing {
		from {
			background-position: 100% 0;
		}
		to {
			background-position: -100% 0;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.drawing {
			animation: none;
		}
	}
	.source {
		display: flex;
		flex-wrap: wrap;
		gap: 0.25rem 0.75rem;
		margin-top: 0.5rem;
		font-family: var(--font-sans);
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}
	.source button {
		all: unset;
		cursor: pointer;
		color: var(--color-primary);
	}
</style>
