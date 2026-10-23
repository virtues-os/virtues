<!--
	RecordCard.svelte

	The card a link to your data opens, the same everywhere it is cited: a
	citation in a chat answer, a mention or a link in a page, a sentence of a
	day article (or its table, photo or figure). Each source shows its own
	words (RecordCardBody): a message as it was sent, a recording as the few
	turns nearest the citing sentence, a purchase as its amount, with the
	words the sentence shares with it marked (lib/wiki/recordWords.ts).
	Nothing is summarized. With the veil on, no words and no names show.
	A chat answer also cites a query (its first rows, and its own tab for
	the rest) and a page a web search returned (the page as the search read
	it).

	Where it sits is the caller's call (`mode`): in the margin beside the
	sentence when the page has a margin, a popover under the link when it
	doesn't, a sheet from the bottom on a phone (`cardMode`). The caller owns
	the order too: it says where this stop falls (`position`), and ← and →
	ask it for the next one (`onstep`). Its anatomy is the person card's
	(DayGloss): a kicker line, the body, one row of actions.
-->
<script lang="ts" module>
	import { getRecord, type OntologyRecord } from "$lib/api/client";
	import type { QueryResult, WebSource } from "$lib/types/Citation";

	export type CardMode = "margin" | "popover" | "sheet";

	/** One thing the card shows. */
	export interface CardSource {
		/**
		 * `/record/<ontology>/<id>`, `table:id`, a query's
		 * `/chat/<chat>/tool/<call>`, or a web page's address. Null for a
		 * citation with no record: on a day page, a question asked in chat
		 * that day.
		 */
		ref: string | null;
		/** The kicker the citation carries ("Message · 4:52 PM"). Without one, the card names the kind and the time. */
		label?: string;
		/** The passage a search matched, shown before the record arrives. */
		passage?: string;
		title?: string;
		/** A query's result, from the turn's tool output. */
		query?: QueryResult;
		/** A web page, as the turn's search returned it. */
		web?: WebSource;
	}

	/** On a surface with no margin: a popover, or a sheet on a phone. */
	export function cardMode(): "popover" | "sheet" {
		return matchMedia("(max-width: 40rem)").matches ? "sheet" : "popover";
	}

	/** Records fetched this session, by ontology (or table) and id. */
	const records = new Map<string, OntologyRecord>();
	const loading = new Map<string, Promise<OntologyRecord>>();

	function fetchRecord(ontology: string, id: string): Promise<OntologyRecord> {
		const key = `${ontology}/${id}`;
		let p = loading.get(key);
		if (!p) {
			p = getRecord(ontology, id).then((r) => {
				records.set(key, r);
				return r;
			});
			loading.set(key, p);
			p.catch(() => loading.delete(key));
		}
		return p;
	}
</script>

<script lang="ts">
	import { FloatingContent, useClickOutside, useEscapeKey } from "$lib/floating";
	import { portal } from "$lib/actions/portal";
	import { parseRecordRef } from "$lib/utils/refRoutes";
	import { contentWords, messageSender } from "$lib/wiki/recordWords";
	import { plainText } from "$lib/wiki/inlineMarkdown";
	import { veil } from "$lib/stores/veil.svelte";
	import RecordCardBody, { KIND_LABEL, citedTurns, recordKind, type RecordKind } from "./RecordCardBody.svelte";

	interface Props {
		anchor: HTMLElement;
		sources: CardSource[];
		/** The sentence that cites them, as markdown or text: it places a recording's window and marks the shared words. */
		sentence: string;
		/** What the card is about, for assistive tech. */
		label?: string;
		mode: CardMode;
		/** In the margin: where the card sits inside the page body, in px. */
		place?: { top: number; left: number; width: number } | null;
		/** The zone for times the citation doesn't carry; null reads them in this device's zone. */
		timezone?: string | null;
		/** "time" on a page about one day, where the date goes without saying; "date" anywhere else. */
		when?: "time" | "date";
		/** Where this stop falls among the caller's: "3 of 21". */
		position?: { n: number; total: number } | null;
		/** Take focus on opening: it was opened, or stepped to, from the keyboard. */
		focus?: boolean;
		/** What the open button says for a record. A query's shows its rows. */
		openLabel?: string;
		/** Open a source: the record, or a query's rows. */
		onopen: (ref: string) => void;
		/** Write in the margin beside this sentence. */
		onnote?: () => void;
		/** Go to the previous (-1) or next (1) stop. */
		onstep?: (by: -1 | 1) => void;
		/** Close the card; `returnFocus` when the card was reached from the keyboard. */
		onclose: (returnFocus?: boolean) => void;
	}

	let {
		anchor,
		sources,
		sentence,
		label = "The record behind this link",
		mode,
		place = null,
		timezone = null,
		when = "date",
		position = null,
		focus = false,
		openLabel = "Open beside",
		onopen,
		onnote,
		onstep,
		onclose,
	}: Props = $props();

	let card: HTMLElement | null = $state(null);

	useClickOutside(() => [card, anchor], () => onclose());
	// Focus goes back only where it came from the keyboard, or is now inside
	// the card (it would otherwise fall to the page when the card goes).
	useEscapeKey(() => onclose(focus || !!card?.contains(document.activeElement)));
	$effect(() => {
		if (card && focus) card.focus({ preventScroll: true });
	});

	// ← and → step, unless the keys belong to something being typed in.
	$effect(() => {
		const step = onstep;
		if (!step) return;
		const onKey = (e: KeyboardEvent) => {
			if ((e.key !== "ArrowLeft" && e.key !== "ArrowRight") || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
			const at = e.target instanceof Element ? e.target : null;
			if (at?.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])')) return;
			e.preventDefault();
			step(e.key === "ArrowRight" ? 1 : -1);
		};
		document.addEventListener("keydown", onKey);
		return () => document.removeEventListener("keydown", onKey);
	});

	// A card that moves to another stop slides there, so the eye can follow it.
	// The first placement does not.
	let gliding = $state(false);
	let lastAnchor: HTMLElement | null = null;
	$effect(() => {
		const a = anchor;
		if (lastAnchor && a !== lastAnchor) {
			gliding = true;
			const t = setTimeout(() => (gliding = false), 320);
			lastAnchor = a;
			return () => clearTimeout(t);
		}
		lastAnchor = a;
	});

	type Loaded = { source: CardSource; kind: RecordKind; record: OntologyRecord | null; failed: boolean };
	let loaded = $state<Loaded[]>([]);
	// Which set of sources a fetch belongs to. `loaded` holds its sources as
	// state proxies, so a fetch can't tell its own source by identity.
	let generation = 0;

	$effect(() => {
		// One source per record: a sentence, or a table's lines, can cite the
		// same moment twice. A citation with no record still shows.
		const list = sources.filter((n, i, all) => all.findIndex((m) => (m.ref ?? m.label) === (n.ref ?? n.label)) === i);
		const parsed = list.map((s) => (s.ref && !s.query && !s.web ? parseRecordRef(s.ref) : null));
		const run = ++generation;
		loaded = list.map((source, i) => {
			const p = parsed[i];
			const kind = source.web ? "web" : p ? recordKind(p.ontology) : "other";
			return { source, kind, record: p ? (records.get(`${p.ontology}/${p.id}`) ?? null) : null, failed: false };
		});
		list.forEach((source, i) => {
			const p = parsed[i];
			if (!p || records.has(`${p.ontology}/${p.id}`)) return;
			fetchRecord(p.ontology, p.id)
				.then((r) => {
					if (run === generation) loaded[i] = { source, kind: recordKind(r.ontology), record: r, failed: false };
				})
				.catch(() => {
					if (run === generation) loaded[i] = { ...loaded[i], failed: true };
				});
		});
	});

	const refs = $derived(loaded.flatMap((s) => (s.source.ref ? [s.source.ref] : [])));
	const many = $derived(refs.length > 1);

	function stamp(v: unknown): string {
		if (typeof v !== "string" && typeof v !== "number") return "";
		const d = new Date(v);
		if (Number.isNaN(d.getTime())) return "";
		const timeZone = timezone ?? undefined;
		return when === "time"
			? d.toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit", timeZone })
			: d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric", timeZone });
	}

	/** The kicker: what the record is, when, and who. The citation's own
	 *  label says the first two when it has one. */
	function kicker(s: Loaded): string[] {
		if (s.source.query) return ["Query", "counted on your server"];
		if (s.source.web) return ["Web", domainOf(s.source.web.url), stamp(s.source.web.published)].filter(Boolean);
		const [kind, time] = (s.source.label ?? "").replace(/\s*·\s*$/, "").split(" · ");
		const at = s.record ? s.record.row[s.record.timestamp_column] : undefined;
		const parts = [kind || KIND_LABEL[s.kind] || s.record?.display_name || "Record", time || stamp(at)];
		if (s.kind === "recording") parts.push("Voices not identified");
		else if (s.kind === "message" && s.record && !veil.hiding) parts.push(messageSender(s.record.row));
		return parts.filter(Boolean);
	}

	const openText = (s: Loaded) => (s.source.query ? "Show all rows" : s.source.web ? "Open site" : openLabel);

	function domainOf(url: string): string {
		try {
			return new URL(url).hostname.replace(/^www\./, "");
		} catch {
			return "";
		}
	}

	/** A web page's words. The search returns its text as markdown: headings, images and links. */
	function pageWords(md: string | undefined): string | undefined {
		if (!md) return undefined;
		const words = plainText(md.replace(/!\[[^\]]*\]\([^)]*\)/g, "").replace(/^[ \t]*(?:#{1,6}|[-*+]|\d+[.)]|>)[ \t]+/gm, ""));
		// Emphasis that runs across a line break is not inline markdown, and stays.
		return words.replace(/\*\*|__/g, "");
	}

	/** A query's first three rows, in its first three columns. An id says nothing a reader can use here. */
	const preview = (q: QueryResult) => ({ rows: q.rows.slice(0, 3), columns: q.columns.filter((c) => c !== "id").slice(0, 3) });

	const words = $derived(contentWords(sentence));

	/** Said only once every record has loaded and none shares a word. */
	const sharesNothing = $derived.by(() => {
		if (veil.hiding || !words.size) return false;
		const cited = loaded.filter((s) => s.source.ref);
		if (!cited.length || cited.some((s) => !s.record)) return false;
		const bodies = cited.map((s) => citedTurns(s.kind, s.record!.row, s.source.passage, sentence));
		if (bodies.some((b) => !b)) return false;
		return bodies.every((b) => b!.every((t) => t.pieces.every((p) => !p.marked)));
	});

	const cell = (v: unknown) => (v === null || v === undefined ? "" : typeof v === "object" ? JSON.stringify(v) : String(v));
</script>

{#snippet content()}
	<div
		class="record-card"
		class:in-margin={mode === "margin"}
		class:sheet={mode === "sheet"}
		class:gliding
		style={mode === "margin" && place ? `top: ${place.top}px; left: ${place.left}px; width: ${place.width}px` : undefined}
		bind:this={card}
		role="dialog"
		aria-label={label}
		tabindex="-1"
	>
		{#each loaded as s, i (s.source.ref ?? s.source.label ?? i)}
			<section class="source">
				<p class="kick">
					{#each kicker(s) as part, k (k)}<span class:kind={k === 0}>{k ? `· ${part}` : part}</span>{/each}
					{#if many && s.source.ref}
						<button type="button" class="act own" onclick={() => onopen(s.source.ref as string)}>{openText(s)}</button>
					{/if}
				</p>
				{#if !s.source.ref}
					<p class="quiet">A question you asked Virtues that day. You'll find the day's chats in Data.</p>
				{:else if veil.hiding}
					<p class="quiet">Hidden. Hold V to show this record.</p>
				{:else if s.source.query}
					{@const q = s.source.query}
					{@const p = preview(q)}
					<p class="count">{q.more && q.count === q.rows.length ? "More than " : ""}{q.count === 1 ? "1 row" : `${q.count.toLocaleString()} rows`}</p>
					{#if p.rows.length && p.columns.length}
						<table class="rows">
							<thead><tr>{#each p.columns as c (c)}<th>{c}</th>{/each}</tr></thead>
							<tbody>
								{#each p.rows as r, j (j)}
									<tr>{#each p.columns as c (c)}<td>{cell(r[c])}</td>{/each}</tr>
								{/each}
							</tbody>
						</table>
					{/if}
					<p class="quiet">Your server ran this query, and the model read {q.more ? "the first part of its rows" : "all of its rows"}.</p>
				{:else if s.source.web}
					<RecordCardBody kind="web" record={null} {sentence} passage={pageWords(s.source.web.text)} title={s.source.web.title} />
				{:else if s.failed && !s.source.passage}
					<p class="quiet">Couldn't load this record. Check that your server is running, then open it again.</p>
				{:else if !s.record && !s.source.passage}
					<p class="quiet">Loading…</p>
				{:else}
					<RecordCardBody kind={s.kind} record={s.record} {sentence} passage={s.source.passage} title={s.source.title} />
				{/if}
			</section>
		{/each}
		{#if sharesNothing}
			<p class="quiet">This sentence shares no words with its source.</p>
		{/if}
		<div class="acts">
			<span class="slot">
				{#if !many && refs.length === 1}
					<button type="button" class="act" onclick={() => onopen(refs[0])}>{openText(loaded.find((s) => s.source.ref) as Loaded)}</button>
				{/if}
			</span>
			{#if position && position.total > 1}
				<span class="steps">
					<button type="button" class="step" aria-label="Previous source" disabled={position.n <= 1} onclick={() => onstep?.(-1)}>←</button>
					<span>{position.n} of {position.total}</span>
					<button type="button" class="step" aria-label="Next source" disabled={position.n >= position.total} onclick={() => onstep?.(1)}>→</button>
				</span>
			{/if}
			<span class="slot end">
				{#if onnote}<button type="button" class="act" onclick={() => onnote?.()}>Write a note</button>{/if}
			</span>
		</div>
	</div>
{/snippet}

{#if mode === "margin"}
	{@render content()}
{:else if mode === "sheet"}
	<div use:portal>{@render content()}</div>
{:else}
	<div use:portal>
		<FloatingContent
			{anchor}
			class={gliding ? "record-float gliding" : "record-float"}
			options={{ placement: "bottom-start", offset: 10, flip: true, shift: true, padding: 12, strategy: "fixed", arrow: true }}
		>
			{@render content()}
		</FloatingContent>
	</div>
{/if}

<style>
	.record-card {
		width: min(20rem, calc(100vw - 32px));
		max-height: min(26rem, 60vh);
		overflow-y: auto;
		box-sizing: border-box;
		padding: 0.8rem 0.95rem 0.7rem;
		font-family: var(--font-sans);
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.record-card:focus {
		outline: none;
	}

	/* A popover that moves to the next stop slides there. */
	:global(.record-float.gliding) {
		transition:
			left var(--motion-layout) var(--ease-premium),
			top var(--motion-layout) var(--ease-premium);
	}

	/* In the margin: the card lifts off the page beside its sentence, and
	   scrolls with it. */
	.record-card.in-margin {
		position: absolute;
		z-index: 5;
		max-height: none;
		overflow: visible;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-xl);
		animation: rise var(--motion-enter) var(--ease-premium);
	}

	.record-card.in-margin.gliding {
		transition: top var(--motion-layout) var(--ease-premium);
	}

	/* On a phone: a sheet from the bottom of the screen. */
	.record-card.sheet {
		position: fixed;
		z-index: var(--z-modal);
		left: 0;
		right: 0;
		bottom: 0;
		width: auto;
		max-height: 60vh;
		padding-bottom: calc(1.25rem + env(safe-area-inset-bottom, 0px));
		background: var(--color-surface);
		border-top: 1px solid var(--color-border);
		border-radius: var(--radius-xl) var(--radius-xl) 0 0;
		animation: sheet-up 0.18s ease-out;
	}

	@keyframes rise {
		from {
			opacity: 0;
			transform: translateY(3px);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	@keyframes sheet-up {
		from {
			transform: translateY(100%);
		}
		to {
			transform: none;
		}
	}

	.source + .source {
		border-top: 1px solid var(--color-border-subtle);
		margin-top: 0.625rem;
		padding-top: 0.625rem;
	}

	.kick {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0 0.35rem;
		margin: 0 0 0.45rem;
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	.kick .kind {
		font-weight: 500;
		color: var(--color-foreground-muted);
	}

	.kick .own {
		margin-left: auto;
	}

	.act {
		font: inherit;
		color: var(--color-primary);
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
	}

	.act:hover {
		text-decoration: underline;
		text-underline-offset: 2px;
	}

	.count {
		margin: 0 0 0.5rem;
		font-size: 0.9375rem;
		color: var(--color-foreground);
	}

	.rows {
		width: 100%;
		border-collapse: collapse;
		table-layout: fixed;
		font-size: 0.75rem;
	}

	.rows th {
		text-align: left;
		font-weight: 400;
		color: var(--color-foreground-subtle);
		padding: 0 0.5rem 0.25rem 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.rows td {
		padding: 0.25rem 0.5rem 0.25rem 0;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.acts {
		display: grid;
		grid-template-columns: 1fr auto 1fr;
		align-items: baseline;
		gap: 0.75rem;
		margin-top: 0.6rem;
		padding-top: 0.55rem;
		border-top: 1px solid var(--color-border-subtle);
		font-size: 0.75rem;
	}

	.slot.end {
		justify-self: end;
	}

	.steps {
		display: inline-flex;
		align-items: baseline;
		gap: 0.4rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	.step {
		font: inherit;
		color: var(--color-foreground-muted);
		background: none;
		border: none;
		padding: 0 0.15rem;
		cursor: pointer;
	}

	.step:hover:not(:disabled) {
		color: var(--color-foreground);
	}

	.step:disabled {
		opacity: 0.35;
		cursor: default;
	}

	.quiet {
		margin: 0.375rem 0 0;
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle);
	}

	@media (prefers-reduced-motion: reduce) {
		.record-card.in-margin,
		.record-card.sheet {
			animation: none;
		}
		.record-card.in-margin.gliding,
		:global(.record-float.gliding) {
			transition: none;
		}
	}
</style>
