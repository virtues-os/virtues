<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import Icon from "$lib/components/Icon.svelte";
	import { getRecord, type OntologyRecord } from "$lib/api/client";

	let { tab }: { tab: Tab } = $props();

	// Route: /record/<ontology>/<record_id>. The id may itself contain slashes in
	// theory; ontology is the first segment, the rest is the id.
	const parsed = $derived.by(() => {
		const m = tab.route.match(/^\/record\/([a-z_]+)\/(.+)$/);
		return { ontology: m?.[1] ?? "", recordId: m?.[2] ?? "" };
	});

	let record = $state<OntologyRecord | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	$effect(() => {
		const { ontology, recordId } = parsed;
		if (!ontology || !recordId) {
			error = "Malformed record link.";
			loading = false;
			return;
		}
		loading = true;
		error = null;
		record = null;
		getRecord(ontology, recordId)
			.then((r) => {
				record = r;
			})
			.catch((e) => {
				error = e instanceof Error ? e.message : "Failed to load record";
			})
			.finally(() => {
				loading = false;
			});
	});

	// Columns we never want to show a reader: search plumbing and opaque blobs.
	const HIDDEN = /embedding|vector|tsv|_hash$|bm25|search_/i;
	// Candidate keys for the record's human title, in priority order.
	const TITLE_KEYS = [
		"title",
		"subject",
		"name",
		"canonical_name",
		"merchant",
		"summary",
		"headline",
		"label",
		"description",
	];

	const row = $derived(record?.row ?? {});

	const title = $derived.by(() => {
		for (const k of TITLE_KEYS) {
			const v = row[k];
			if (typeof v === "string" && v.trim()) return v.trim();
		}
		return record?.display_name ?? "Record";
	});

	// The lead timestamp (from the ontology's declared timestamp column).
	const timestamp = $derived.by(() => {
		const v = record ? row[record.timestamp_column] : undefined;
		return typeof v === "string" || typeof v === "number" ? formatDate(v) : null;
	});

	// Field rows: everything meaningful, timestamp column first, id last, minus
	// the hidden plumbing and the value we already used as the title.
	const fields = $derived.by(() => {
		const rec = record;
		if (!rec) return [] as Array<{ key: string; label: string; value: unknown }>;
		const tsCol = rec.timestamp_column;
		const usedTitleKey = TITLE_KEYS.find(
			(k) => typeof row[k] === "string" && (row[k] as string).trim() === title
		);
		const entries = Object.entries(row).filter(
			([k, v]) =>
				!HIDDEN.test(k) &&
				k !== "id" &&
				k !== usedTitleKey &&
				v !== null &&
				v !== "" &&
				!(Array.isArray(v) && v.length === 0)
		);
		entries.sort(([a], [b]) => {
			if (a === tsCol) return -1;
			if (b === tsCol) return 1;
			return 0;
		});
		return entries.map(([key, value]) => ({ key, label: humanize(key), value }));
	});

	function humanize(key: string): string {
		return key
			.replace(/_/g, " ")
			.replace(/\b\w/g, (c) => c.toUpperCase());
	}

	function looksLikeDate(s: string): boolean {
		return /^\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}/.test(s);
	}

	function formatDate(v: string | number): string {
		const d = new Date(v);
		if (Number.isNaN(d.getTime())) return String(v);
		return d.toLocaleString(undefined, {
			weekday: "short",
			year: "numeric",
			month: "short",
			day: "numeric",
			hour: "numeric",
			minute: "2-digit",
		});
	}

	type Rendered = { kind: "text" | "date" | "bool" | "num" | "json"; text: string };

	function render(value: unknown): Rendered {
		if (typeof value === "boolean") return { kind: "bool", text: value ? "Yes" : "No" };
		if (typeof value === "number")
			return { kind: "num", text: value.toLocaleString() };
		if (typeof value === "string") {
			if (looksLikeDate(value)) return { kind: "date", text: formatDate(value) };
			return { kind: "text", text: value };
		}
		if (value && typeof value === "object")
			return { kind: "json", text: JSON.stringify(value, null, 2) };
		return { kind: "text", text: String(value) };
	}
</script>

<div class="data-view">
	<div class="inner">
		{#if loading}
			<div class="state">Loading record…</div>
		{:else if error}
			<div class="state error">
				<Icon icon="ri:error-warning-line" width="16" />
				<span>{error}</span>
			</div>
		{:else if record}
			<header class="head">
				<div class="eyebrow">
					<Icon icon="ri:database-2-line" width="12" />
					<span>{record.display_name}</span>
				</div>
				<h1 class="title">{title}</h1>
				{#if timestamp}
					<div class="meta">{timestamp}</div>
				{/if}
			</header>

			<dl class="fields">
				{#each fields as f (f.key)}
					{@const r = render(f.value)}
					<div class="field">
						<dt>{f.label}</dt>
						<dd class:mono={r.kind === "json"} class:muted={r.kind === "date"}>
							{#if r.kind === "json"}
								<pre>{r.text}</pre>
							{:else}
								{r.text}
							{/if}
						</dd>
					</div>
				{/each}
			</dl>

			<footer class="foot">
				<span class="id-chip">{record.ontology} · {record.record_id}</span>
			</footer>
		{/if}
	</div>
</div>

<style>
	.data-view {
		width: 100%;
		height: 100%;
		overflow-y: auto;
	}
	.inner {
		max-width: 720px;
		margin: 0 auto;
		padding: 3.5rem 2rem 6rem;
	}

	.state {
		color: var(--color-foreground-subtle);
		font-size: 0.9rem;
		padding: 2rem 0;
	}
	.state.error {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		color: var(--color-danger, #c0392b);
	}

	.head {
		margin-bottom: 2.5rem;
	}
	.eyebrow {
		display: inline-flex;
		align-items: center;
		gap: 0.35rem;
		font-family: var(--font-mono);
		font-size: 0.7rem;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
		margin-bottom: 0.75rem;
	}
	.title {
		font-family: var(--font-serif);
		font-weight: 500;
		font-size: 1.75rem;
		line-height: 1.2;
		margin: 0;
		color: var(--color-foreground);
	}
	.meta {
		margin-top: 0.5rem;
		font-size: 0.85rem;
		color: var(--color-foreground-subtle);
	}

	.fields {
		margin: 0;
		border-top: 1px solid var(--color-border);
	}
	.field {
		display: grid;
		grid-template-columns: 180px 1fr;
		gap: 1.5rem;
		padding: 0.85rem 0;
		border-bottom: 1px solid var(--color-border);
	}
	dt {
		font-size: 0.8rem;
		color: var(--color-foreground-subtle);
		padding-top: 0.1rem;
	}
	dd {
		margin: 0;
		font-size: 0.9rem;
		line-height: 1.55;
		color: var(--color-foreground);
		white-space: pre-wrap;
		word-break: break-word;
	}
	dd.muted {
		color: var(--color-foreground-muted, var(--color-foreground-subtle));
	}
	dd.mono pre {
		font-family: var(--font-mono);
		font-size: 0.8rem;
		margin: 0;
		padding: 0.75rem;
		background: var(--color-surface-elevated);
		border-radius: 6px;
		overflow-x: auto;
		white-space: pre;
	}

	.foot {
		margin-top: 2.5rem;
	}
	.id-chip {
		font-family: var(--font-mono);
		font-size: 0.7rem;
		color: var(--color-foreground-subtle);
		opacity: 0.7;
	}

	@media (max-width: 640px) {
		.inner {
			padding: 2rem 1.25rem 4rem;
		}
		.field {
			grid-template-columns: 1fr;
			gap: 0.25rem;
		}
	}
</style>
