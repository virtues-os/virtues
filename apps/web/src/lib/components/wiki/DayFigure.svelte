<!--
	DayFigure: one figure set into a day's Article. The writer, or the server
	for a route, emits a fenced `figure` block; DayArticleBody parses it into
	`fields` and places this in the text column.

	- quote: words from the record, set large, with who said them and when.
	- route: where the phone was (lib/wiki/dayRoute.ts), one map of the day's
	  city when it has anything to frame, and a second of the long move when
	  the day crossed one.
	- thread: messages from one thread set as dialogue.
	- picture: a painting of a place the page names (api/day_picture.rs), with
	  its caption. The caption veils the place as the page does.

	A quote, a thread, or a figure with a `ref` opens its record through
	`oncite`. With the veil on, the record's words and the names in them are
	hidden, as they are in the evidence card.
-->
<script lang="ts">
	import type { Snippet } from "svelte";
	import { getRecord, type OntologyRecord } from "$lib/api/client";
	import MovementMap from "$lib/components/timeline/MovementMap.svelte";
	import { backendUrl } from "$lib/config/backend";
	import { veiled } from "$lib/actions/veil";
	import { veil } from "$lib/stores/veil.svelte";
	import { getDayTimeline } from "$lib/wiki/api";
	import { figureRefs, veilMarks } from "$lib/wiki/dayArticle";
	import { clock, dayRoute, type DayRoute } from "$lib/wiki/dayRoute";
	import { messageBody } from "$lib/wiki/recordWords";

	interface Props {
		fields: Record<string, string>;
		date: string;
		timezone: string | null;
		/** `focus`: opened from the keyboard, so the card takes focus. */
		oncite?: (refs: string[], anchor: HTMLElement, focus: boolean) => void;
	}

	let { fields, date, timezone, oncite }: Props = $props();

	const kind = $derived(fields.kind ?? "");
	const refs = $derived(figureRefs(fields));
	const citable = $derived(!!oncite && refs.length > 0);

	/** A click with no pointer count (`detail` 0) came from Enter or Space. */
	function cite(e: MouseEvent) {
		if (citable) oncite?.(refs, e.currentTarget as HTMLElement, e.detail === 0);
	}

	// ── quote ──
	/** Attributions that name nobody; any other `who` is a person's name. */
	const UNNAMED = new Set(["You", "In a recording", "In a message"]);
	const quoteText = $derived(veilMarks(fields.text ?? "").markdown);

	// ── picture ──
	/** The caption's words, and the place in it the veil hides. */
	const caption = $derived(veilMarks(fields.caption ?? ""));
	const made = $derived(fields.style === "pencil" ? "Drawn from the record" : "Painted from the record");

	// ── route ──
	let route = $state<DayRoute | null>(null);
	$effect(() => {
		if (kind !== "route") return;
		const zone = timezone;
		let live = true;
		route = null;
		getDayTimeline(date)
			.then((view) => {
				if (live) route = dayRoute(view, zone);
			})
			.catch(() => {
				// No location to draw: the figure stays empty rather than claiming a route.
				if (live) route = null;
			});
		return () => {
			live = false;
		};
	});

	// ── thread ──
	type Line = { ref: string; who: string; words: string; time: string };
	let lines = $state<Line[]>([]);

	/** A dialogue's speaker: "You", else the sender's first name. A sender
	 *  with no name, or whose name is a number or an address, is "Someone". */
	function speaker(row: Record<string, unknown>): string {
		const meta = (row.metadata ?? {}) as Record<string, unknown>;
		if (meta.is_from_me === true || meta.is_from_me === "true") return "You";
		const name = String(row.from_name ?? "").trim();
		if (!name || name.includes("@") || /^[+\d\s().-]+$/.test(name)) return "Someone";
		return name.split(/\s+/)[0];
	}

	function toLine(ref: string, r: OntologyRecord): Line {
		const at = Date.parse(String(r.row.occurred_at ?? ""));
		return {
			ref,
			who: speaker(r.row),
			words: messageBody(r.row) || "(no text)",
			time: Number.isFinite(at) ? clock(at, timezone) : "",
		};
	}

	$effect(() => {
		if (kind !== "thread") return;
		const list = refs;
		let live = true;
		lines = [];
		Promise.all(
			list.map((ref) => {
				const [table, ...rest] = ref.split(":");
				return getRecord(table, rest.join(":")).then((r) => toLine(ref, r));
			}),
		)
			.then((loaded) => {
				if (live) lines = loaded;
			})
			.catch(() => {
				// A message that won't load leaves the exchange out; the
				// sentences around it still open their evidence.
				if (live) lines = [];
			});
		return () => {
			live = false;
		};
	});
</script>

{#snippet hit(body: Snippet)}
	{#if citable}
		<button type="button" class="hit" onclick={cite}>{@render body()}</button>
	{:else}
		<div class="hit">{@render body()}</div>
	{/if}
{/snippet}

{#if kind === "quote" && quoteText}
	{@const who = fields.who ?? ""}
	<figure class="day-figure fig-quote">
		{#snippet quote()}
			<!-- The veil hides the words and leaves the quote marks. -->
			<q use:veiled={{ hiding: veil.hiding, whole: true }}>{quoteText}</q>
			{#if who || fields.time}
				<span class="who" use:veiled={{ hiding: veil.hiding && !!who && !UNNAMED.has(who), whole: true }}
					>{[who, fields.time].filter(Boolean).join(" · ")}</span
				>
			{/if}
		{/snippet}
		{@render hit(quote)}
	</figure>
{:else if kind === "route" && route}
	<figure class="day-figure fig-route">
		<div class="panels" class:two={!!route.city && !!route.long}>
			{#if route.city}
				<MovementMap track={route.city.track} stops={route.city.stops} height={220} interactive={false} />
			{/if}
			{#if route.long}
				<MovementMap track={route.long.track} stops={route.long.stops} height={220} interactive={false} />
			{/if}
		</div>
		<figcaption>Where your phone was.{#if route.quiet} Dashed where it went quiet.{/if}</figcaption>
	</figure>
{:else if kind === "thread" && lines.length}
	<figure class="day-figure fig-thread">
		{#snippet dialogue()}
			<span class="dialogue" use:veiled={{ hiding: veil.hiding, whole: true }}>
				{#each lines as line (line.ref)}
					<span class="msg">
						<span class="sender">{line.who}</span>
						<span class="words">{line.words}</span>
						<time>{line.time}</time>
					</span>
				{/each}
			</span>
		{/snippet}
		{@render hit(dialogue)}
	</figure>
{:else if kind === "picture" && fields.src}
	<figure class="day-figure fig-picture">
		{#snippet picture()}
			<!-- With names hidden, the alt names nothing either. -->
			<img src={backendUrl(fields.src)} alt={veil.hiding ? made : caption.markdown} loading="lazy" />
		{/snippet}
		{@render hit(picture)}
		<figcaption>
			<span class="kicker">{made}</span>
			{#if caption.markdown}
				<span class="caption" use:veiled={{ hiding: veil.hiding, phrases: caption.phrases }}>{caption.markdown}</span>
			{/if}
		</figcaption>
	</figure>
{/if}

<style>
	.day-figure {
		margin: 1.5rem 0 2rem;
	}

	/* The figure is one control: the whole of it opens its record. */
	.hit {
		display: block;
		width: 100%;
		margin: 0;
		padding: 0;
		border: none;
		background: none;
		font: inherit;
		color: inherit;
		text-align: left;
	}

	button.hit {
		cursor: pointer;
	}

	button.hit:focus-visible {
		outline: 2px solid var(--color-border-focus);
		outline-offset: 4px;
		border-radius: 6px;
	}

	/* ── quote ── */
	.fig-quote q {
		display: block;
		font-family: var(--font-serif);
		font-size: 1.875rem;
		line-height: 1.25;
		color: var(--color-foreground);
		quotes: "\201C" "\201D";
		/* The opening mark hangs in the margin so the words keep the text's left edge. */
		text-indent: -0.42em;
	}

	.fig-quote q::before,
	.fig-quote q::after {
		color: var(--color-secondary);
	}

	.who {
		display: block;
		margin-top: 0.625rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		color: var(--color-foreground-subtle);
	}

	/* ── route ── */
	.panels {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: 0.75rem;
	}

	.panels.two {
		grid-template-columns: minmax(0, 2fr) minmax(0, 3fr);
	}

	.fig-route figcaption,
	.fig-picture figcaption {
		margin-top: 0.5rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle);
	}

	/* ── thread ── */
	.fig-thread .hit {
		padding: 0.125rem 0 0.125rem 1rem;
		border-left: 2px solid var(--color-border-strong);
	}

	.dialogue {
		display: block;
	}

	.msg {
		display: grid;
		grid-template-columns: 3.25rem minmax(0, 1fr) auto;
		align-items: baseline;
		column-gap: 0.75rem;
		margin: 0.125rem 0;
	}

	.sender,
	.msg time {
		font-family: var(--font-sans);
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
	}

	.sender {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.words {
		font-family: var(--font-serif);
		font-size: 1.1875rem;
		line-height: 1.4;
		color: var(--color-foreground);
	}

	/* ── picture ── */
	/* The painter makes 3:2; the box is that shape before the file arrives,
	   so the prose under it never jumps. */
	.fig-picture img {
		display: block;
		width: 100%;
		aspect-ratio: 3 / 2;
		max-height: 19rem;
		object-fit: cover;
		border-radius: 6px;
		background: var(--color-surface-elevated);
	}

	.kicker,
	.caption {
		display: block;
	}

	.kicker {
		color: var(--color-foreground-muted);
	}

	@media (max-width: 560px) {
		.panels.two {
			grid-template-columns: minmax(0, 1fr);
		}

		.fig-quote q {
			font-size: 1.5rem;
		}
	}
</style>
