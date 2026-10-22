<script lang="ts">
	// A record's body inside a ref card: laid out for what kind of record it
	// is, with the passage a search matched and, inside it, the words a
	// citation quoted. The row is fetched once per record and kept for the
	// session; the passage comes from the turn's own tool output, so it shows
	// before the row arrives.
	import { getRecord, type OntologyRecord } from "$lib/api/client";
	import { recordFields, recordTitle, renderValue } from "$lib/utils/recordFields";

	let { ontology, recordId, passage, mark, title: givenTitle } = $props<{
		ontology: string;
		recordId: string;
		/** The passage the search matched, from the turn's tool output. */
		passage?: string;
		/** The citation's own words, shaded where they appear in the passage. */
		mark?: string;
		title?: string;
	}>();

	let record = $state<OntologyRecord | null>(null);
	let failed = $state(false);

	$effect(() => {
		const key = `${ontology}/${recordId}`;
		record = null;
		failed = false;
		loadRecord(ontology, recordId)
			.then((r) => {
				if (`${ontology}/${recordId}` === key) record = r;
			})
			.catch(() => {
				if (`${ontology}/${recordId}` === key) failed = true;
			});
	});

	const row = $derived(record?.row ?? {});
	const str = (k: string): string => (typeof row[k] === "string" ? (row[k] as string).trim() : "");
	const num = (k: string): number | null => {
		const v = row[k];
		const n = typeof v === "number" ? v : typeof v === "string" ? Number(v) : NaN;
		return Number.isFinite(n) ? n : null;
	};

	const kind = $derived(
		ontology === "communication_transcription" ? "voice"
		: ontology === "financial_transaction" ? "purchase"
		: ontology === "calendar_event" ? "event"
		: ontology === "communication_message" ? "message"
		: ontology === "communication_email" ? "email"
		: ontology === "content_bookmark" ? "bookmark"
		: ontology === "app_page" || ontology === "wiki_article" ? "page"
		: "other",
	);

	const KIND_LABEL: Record<string, string> = {
		voice: "Voice note", purchase: "Purchase", event: "Calendar", message: "Message",
		email: "Mail", bookmark: "Bookmark", page: "Page",
	};
	const kindLabel = $derived(KIND_LABEL[kind] ?? record?.display_name ?? "Record");

	const when = $derived.by(() => {
		const v = record ? row[record.timestamp_column] : undefined;
		if (typeof v !== "string" && typeof v !== "number") return "";
		const d = new Date(v);
		return Number.isNaN(d.getTime()) ? "" : d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
	});

	const heading = $derived(
		kind === "voice" ? str("title") || givenTitle || "Voice note"
		: kind === "message" ? str("from_name") || str("from_handle") || str("from_identifier") || givenTitle || "Message"
		: kind === "email" ? str("subject") || givenTitle || "Mail"
		: record ? recordTitle(row, givenTitle ?? record.display_name) : givenTitle ?? "",
	);

	// The text to quote: the matched passage when the search gave one, else the
	// record's own words.
	const quoteSource = $derived(
		passage
		|| (kind === "voice" ? str("text")
			: kind === "message" ? str("body")
			: kind === "email" ? str("body_preview") || str("body")
			: kind === "bookmark" ? str("note") || str("description")
			: kind === "page" ? str("content")
			: ""),
	);
	const quote = $derived(excerpt(quoteSource, mark));

	/** About 320 characters around the cited words, or from the start. */
	function excerpt(text: string, cited?: string): { before: string; hit: string; after: string } | null {
		if (!text) return null;
		const clean = text.replace(/\s+/g, " ").trim();
		const needle = (cited ?? "").replace(/^[\s“”"'‘’]+|[\s“”"'‘’.,;:!?]+$/g, "");
		const at = needle.length >= 3 ? clean.toLowerCase().indexOf(needle.toLowerCase()) : -1;
		if (at < 0) {
			const cut = clean.length > 320 ? clean.slice(0, 320).replace(/\s+\S*$/, "") + "…" : clean;
			return { before: cut, hit: "", after: "" };
		}
		const start = Math.max(0, at - 140);
		const end = Math.min(clean.length, at + needle.length + 140);
		return {
			before: (start > 0 ? "…" : "") + clean.slice(start, at),
			hit: clean.slice(at, at + needle.length),
			after: clean.slice(at + needle.length, end) + (end < clean.length ? "…" : ""),
		};
	}

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
		const s = str("started_at"), e = str("ended_at");
		const fmt = (v: string) => new Date(v).toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
		if (!s) return "";
		return e ? `${fmt(s)} to ${fmt(e)}` : fmt(s);
	});

	const duration = $derived.by(() => {
		const s = num("duration_seconds");
		if (s === null) return "";
		const m = Math.floor(s / 60), r = Math.round(s % 60);
		return m ? `${m}:${String(r).padStart(2, "0")}` : `${r} seconds`;
	});

	const domain = $derived.by(() => {
		try {
			return new URL(str("url")).hostname.replace(/^www\./, "");
		} catch {
			return "";
		}
	});

	const otherFields = $derived(kind === "other" && record ? recordFields(record, heading).slice(0, 3) : []);
	const meta = $derived(
		[kindLabel, when, kind === "voice" ? duration : "", kind === "bookmark" ? domain : ""].filter(Boolean).join(" · "),
	);
</script>

<script lang="ts" module>
	const cache = new Map<string, Promise<OntologyRecord>>();
	function loadRecord(ontology: string, id: string): Promise<OntologyRecord> {
		const key = `${ontology}/${id}`;
		let p = cache.get(key);
		if (!p) {
			p = getRecord(ontology, id);
			cache.set(key, p);
			p.catch(() => cache.delete(key));
		}
		return p;
	}
</script>

<div class="record-card">
	<div class="rc-meta">{meta}</div>

	{#if kind === "purchase" && money}
		<div class="rc-amount">{money.refund ? "−" : ""}{money.whole}<span class="rc-cents">{money.frac}</span></div>
		<div class="rc-line">{heading}{#if category}<span class="rc-sub">{` · ${category}`}</span>{/if}{#if row.is_pending === true}<span class="rc-sub">{" · pending"}</span>{/if}</div>
	{:else if kind === "event"}
		<div class="rc-event">
			{#if eventTile}
				<span class="rc-tile"><span class="rc-tile-m">{eventTile.month}</span><span class="rc-tile-d">{eventTile.day}</span></span>
			{/if}
			<span class="rc-event-text">
				<span class="rc-line">{heading}</span>
				{#if eventTime}<span class="rc-sub">{eventTime}</span>{/if}
				{#if str("location_name")}<span class="rc-sub">{str("location_name")}</span>{/if}
			</span>
		</div>
	{:else if heading && heading !== kindLabel}
		<!-- An untitled voice note would only repeat the label above it. -->
		<div class="rc-title" class:rc-serif={kind === "page"}>{heading}</div>
	{/if}

	{#if quote && kind !== "purchase" && kind !== "event"}
		<p class="rc-quote">{quote.before}{#if quote.hit}<mark>{quote.hit}</mark>{/if}{quote.after}</p>
	{/if}

	{#if otherFields.length}
		<dl class="rc-fields">
			{#each otherFields as f (f.key)}
				<div><dt>{f.label}</dt><dd>{renderValue(f.value).text}</dd></div>
			{/each}
		</dl>
	{/if}

	{#if failed && !quote}
		<div class="rc-sub">This record could not be loaded. It may have been deleted.</div>
	{/if}
</div>

<style>
	.record-card {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 16px 4px;
		min-width: 0;
	}
	.rc-meta {
		font-size: 12px;
		line-height: 1.4;
		color: var(--color-foreground-subtle);
	}
	.rc-title {
		font-size: 15px;
		line-height: 1.35;
		color: var(--color-foreground);
	}
	/* The serif ships one cut: rank comes from size and ink, never weight. */
	.rc-serif {
		font-family: var(--font-serif);
		font-size: 18px;
		line-height: 1.3;
	}
	.rc-line {
		font-size: 14px;
		line-height: 1.45;
		color: var(--color-foreground);
	}
	.rc-sub {
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}
	.rc-quote {
		margin: 0;
		font-size: 14px;
		line-height: 1.55;
		color: var(--color-foreground-muted);
		display: -webkit-box;
		-webkit-line-clamp: 7;
		line-clamp: 7;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}
	.rc-quote mark {
		background: color-mix(in srgb, var(--color-primary) 12%, transparent);
		color: var(--color-foreground);
		border-radius: var(--radius-xs);
	}
	.rc-amount {
		font-variant-numeric: tabular-nums;
		font-size: 32px;
		line-height: 1.1;
		color: var(--color-foreground);
	}
	.rc-cents {
		color: var(--color-foreground-subtle);
	}
	.rc-event {
		display: flex;
		align-items: center;
		gap: 12px;
	}
	.rc-tile {
		flex: none;
		width: 52px;
		height: 56px;
		border-radius: var(--radius-md);
		background: var(--color-surface-elevated);
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
	}
	.rc-tile-m {
		font-size: 12px;
		color: var(--color-secondary);
	}
	.rc-tile-d {
		font-family: var(--font-serif);
		font-size: 24px;
		line-height: 1;
		color: var(--color-foreground);
	}
	.rc-event-text {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}
	.rc-fields {
		margin: 0;
		display: grid;
		gap: 4px;
		font-size: 13px;
	}
	.rc-fields div {
		display: flex;
		gap: 12px;
	}
	.rc-fields dt {
		flex: none;
		width: 96px;
		color: var(--color-foreground-subtle);
	}
	.rc-fields dd {
		margin: 0;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--color-foreground);
	}
</style>
