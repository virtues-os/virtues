<script lang="ts" module>
	import type { OntologyRecord } from "$lib/api/client";
	import { contentWords, markRanges, messageBody, nearTurns, sharedRanges } from "$lib/wiki/recordWords";

	export type RecordKind = "message" | "recording" | "purchase" | "event" | "email" | "bookmark" | "page" | "other";

	/** The kind of record an ontology or table name holds. A table name drops its `data_`. */
	export function recordKind(name: string): RecordKind {
		switch (name.replace(/^data_/, "")) {
			case "communication_message":
				return "message";
			case "communication_transcription":
				return "recording";
			case "financial_transaction":
				return "purchase";
			case "calendar_event":
				return "event";
			case "communication_email":
				return "email";
			case "content_bookmark":
				return "bookmark";
			case "app_page":
			case "app_pages":
			case "wiki_article":
				return "page";
			default:
				return "other";
		}
	}

	/** What the card's kicker calls each kind. Any other kind goes by its ontology's display name. */
	export const KIND_LABEL: Record<RecordKind, string | null> = {
		message: "Message",
		recording: "Recording",
		purchase: "Purchase",
		event: "Calendar",
		email: "Mail",
		bookmark: "Bookmark",
		page: "Page",
		other: null,
	};

	export type Piece = { text: string; marked: boolean };
	export type Turn = { pieces: Piece[]; near: boolean };

	/**
	 * A message or a recording as the card quotes it, cut where it shares the
	 * citing sentence's words: a message as it was sent, a recording as the
	 * turns nearest the sentence. A search's passage stands in until the
	 * record arrives, and places a recording's window, since it is the part
	 * the search matched. Null for any other kind.
	 */
	export function citedTurns(
		kind: RecordKind,
		row: Record<string, unknown> | null,
		passage: string | undefined,
		sentence: string,
	): Turn[] | null {
		const words = contentWords(sentence);
		const marked = (text: string) => markRanges(text, sharedRanges(text, words));
		if (kind === "message") {
			const body = row ? messageBody(row) : (passage ?? "");
			return [{ pieces: body ? marked(body) : [{ text: "(no text)", marked: false }], near: true }];
		}
		if (kind === "recording") {
			const text = passage || String(row?.text ?? "");
			return nearTurns(text, sentence).turns.map((t) => ({ pieces: marked(t.text), near: t.near }));
		}
		return null;
	}

	/** About 320 characters of a record's words, from the first one it shares with the sentence, or from the start. */
	function excerpt(text: string, words: Set<string>): Piece[] {
		const clean = text.replace(/\s+/g, " ").trim();
		if (clean.length <= 320) return markRanges(clean, sharedRanges(clean, words));
		const first = sharedRanges(clean, words)[0]?.[0] ?? 0;
		let from = first > 140 ? clean.indexOf(" ", first - 140) + 1 : 0;
		if (from <= 0) from = 0;
		let to = Math.min(clean.length, from + 320);
		if (to < clean.length) to = clean.lastIndexOf(" ", to) > from ? clean.lastIndexOf(" ", to) : to;
		const cut = clean.slice(from, to);
		return [
			...(from > 0 ? [{ text: "…", marked: false }] : []),
			...markRanges(cut, sharedRanges(cut, words)),
			...(to < clean.length ? [{ text: "…", marked: false }] : []),
		];
	}
</script>

<script lang="ts">
	// One source's body in a RecordCard, laid out for its kind: a message or
	// a recording as its own words, a purchase as its amount, an event as its
	// date, and anything else as its title, its words and its first fields.
	// The words the citing sentence shares with it are marked. The card owns
	// the kicker, the loading and the veil; this draws what the record says.
	import { recordFields, recordTitle, renderValue } from "$lib/utils/recordFields";

	let {
		kind,
		record,
		sentence = "",
		passage,
		title: givenTitle,
	} = $props<{
		kind: RecordKind;
		/** The row, once it has loaded. Until then the passage stands in. */
		record: OntologyRecord | null;
		/** The sentence that cites the record, as markdown or text. */
		sentence?: string;
		/** The passage a search matched, from the turn's tool output. */
		passage?: string;
		title?: string;
	}>();

	const row = $derived((record?.row ?? {}) as Record<string, unknown>);
	const str = (k: string): string => (typeof row[k] === "string" ? (row[k] as string).trim() : "");
	const num = (k: string): number | null => {
		const v = row[k];
		const n = typeof v === "number" ? v : typeof v === "string" ? Number(v) : NaN;
		return Number.isFinite(n) ? n : null;
	};

	const words = $derived(contentWords(sentence));
	const turns = $derived(citedTurns(kind, record ? row : null, passage, sentence));

	const heading = $derived(
		kind === "recording" ? str("title") || givenTitle || ""
		: kind === "email" ? str("subject") || givenTitle || ""
		: kind === "message" ? ""
		: record ? recordTitle(row, givenTitle ?? record.display_name)
		: givenTitle ?? "",
	);

	// The words to quote: the passage a search matched when there is one, else
	// the record's own.
	const quote = $derived.by(() => {
		const text =
			passage ||
			(kind === "email" ? str("body_preview") || str("body")
			: kind === "bookmark" ? str("note") || str("description")
			: kind === "page" ? str("content")
			: "");
		return text ? excerpt(text, words) : [];
	});

	// Money is stored in cents.
	const money = $derived.by(() => {
		const cents = num("amount");
		if (cents === null) return null;
		const currency = str("currency") || "USD";
		try {
			const parts = new Intl.NumberFormat(undefined, { style: "currency", currency }).formatToParts(Math.abs(cents) / 100);
			const whole = parts.filter((p) => p.type !== "decimal" && p.type !== "fraction").map((p) => p.value).join("");
			const frac = parts.filter((p) => p.type === "decimal" || p.type === "fraction").map((p) => p.value).join("");
			return { whole, frac, refund: cents < 0 };
		} catch {
			return { whole: (Math.abs(cents) / 100).toFixed(2), frac: "", refund: cents < 0 };
		}
	});
	const category = $derived.by(() => {
		const c = row.category;
		if (Array.isArray(c)) return c.filter((x) => typeof x === "string").join(" · ");
		return str("merchant_category");
	});

	const eventTile = $derived.by(() => {
		const s = str("started_at");
		const d = s ? new Date(s) : null;
		if (!d || Number.isNaN(d.getTime())) return null;
		return { month: d.toLocaleDateString(undefined, { month: "short" }), day: String(d.getDate()) };
	});
	const eventTime = $derived.by(() => {
		if (row.is_all_day === true) return "All day";
		const s = str("started_at"),
			e = str("ended_at");
		const fmt = (v: string) => new Date(v).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
		if (!s) return "";
		return e ? `${fmt(s)} to ${fmt(e)}` : fmt(s);
	});

	const domain = $derived.by(() => {
		try {
			return new URL(str("url")).hostname.replace(/^www\./, "");
		} catch {
			return "";
		}
	});

	const otherFields = $derived(kind === "other" && record ? recordFields(record, heading).slice(0, 3) : []);
</script>

{#snippet pieces(list: Piece[])}{#each list as piece, p (p)}{#if piece.marked}<mark>{piece.text}</mark>{:else}{piece.text}{/if}{/each}{/snippet}

<div class="record-body">
	{#if turns}
		{#if heading}<p class="rb-title">{heading}</p>{/if}
		<blockquote>
			{#each turns as turn, t (t)}
				<p class:near={turn.near}>{@render pieces(turn.pieces)}</p>
			{/each}
		</blockquote>
	{:else if kind === "purchase" && money}
		<p class="rb-amount">{money.refund ? "−" : ""}{money.whole}<span class="rb-cents">{money.frac}</span></p>
		<p class="rb-line">
			{heading}{#if category}<span class="rb-sub">{` · ${category}`}</span>{/if}{#if row.is_pending === true}<span class="rb-sub">{" · pending"}</span>{/if}
		</p>
	{:else if kind === "event"}
		<div class="rb-event">
			{#if eventTile}
				<span class="rb-tile"><span class="rb-tile-m">{eventTile.month}</span><span class="rb-tile-d">{eventTile.day}</span></span>
			{/if}
			<span class="rb-event-text">
				<span class="rb-line">{heading}</span>
				{#if eventTime}<span class="rb-sub">{eventTime}</span>{/if}
				{#if str("location_name")}<span class="rb-sub">{str("location_name")}</span>{/if}
			</span>
		</div>
	{:else}
		{#if heading}<p class="rb-title" class:rb-serif={kind === "page"}>{heading}</p>{/if}
		{#if kind === "bookmark" && domain}<p class="rb-sub">{domain}</p>{/if}
		{#if quote.length}
			<blockquote><p class="near">{@render pieces(quote)}</p></blockquote>
		{/if}
		{#if otherFields.length}
			<dl class="rb-fields">
				{#each otherFields as f (f.key)}
					<div><dt>{f.label}</dt><dd>{renderValue(f.value).text}</dd></div>
				{/each}
			</dl>
		{/if}
	{/if}
</div>

<style>
	.record-body {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
		min-width: 0;
	}

	.record-body p {
		margin: 0;
	}

	.rb-title {
		font-size: 0.9375rem;
		line-height: 1.35;
		color: var(--color-foreground);
	}

	/* The serif ships one cut: rank comes from size and ink, never weight. */
	.rb-serif {
		font-family: var(--font-serif);
		font-size: 1.125rem;
		line-height: 1.3;
	}

	.rb-line {
		font-size: 0.875rem;
		line-height: 1.45;
		color: var(--color-foreground);
	}

	.rb-sub {
		font-size: 0.8125rem;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	/* The record's own words, in the serif. */
	blockquote {
		margin: 0;
		font-family: var(--font-serif);
		font-size: 0.98rem;
		line-height: 1.45;
		color: var(--color-foreground-subtle);
	}

	blockquote p + p {
		margin-top: 0.45rem;
	}

	/* The turn nearest the sentence, when one shares its words; its
	   neighbors set it in context. A message is one turn at full strength. */
	blockquote p.near {
		color: var(--color-foreground);
	}

	/* The record's words the sentence shares. */
	mark {
		background: var(--color-highlight);
		color: inherit;
		padding: 0 1px;
		box-decoration-break: clone;
		-webkit-box-decoration-break: clone;
	}

	.rb-amount {
		font-variant-numeric: tabular-nums;
		font-size: 2rem;
		line-height: 1.1;
		color: var(--color-foreground);
	}

	.rb-cents {
		color: var(--color-foreground-subtle);
	}

	.rb-event {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.rb-tile {
		flex: none;
		width: 3.25rem;
		height: 3.5rem;
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
	}

	.rb-tile-m {
		font-size: 0.75rem;
		color: var(--color-secondary);
	}

	.rb-tile-d {
		font-family: var(--font-serif);
		font-size: 1.5rem;
		line-height: 1;
		color: var(--color-foreground);
	}

	.rb-event-text {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		min-width: 0;
	}

	.rb-fields {
		margin: 0;
		display: grid;
		gap: 0.25rem;
		font-size: 0.8125rem;
	}

	.rb-fields div {
		display: flex;
		gap: 0.75rem;
	}

	.rb-fields dt {
		flex: none;
		width: 6rem;
		color: var(--color-foreground-subtle);
	}

	.rb-fields dd {
		margin: 0;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--color-foreground);
	}
</style>
