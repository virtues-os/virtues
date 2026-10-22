<script lang="ts">
	// The card a citation in an answer opens: the cited record's body, or for
	// a figure, the query behind it. The footer steps through the answer's
	// citations in the order it links them and opens the record beside the chat.
	import type { Citation, RecordSource } from "$lib/types/Citation";
	import RecordCardBody from "$lib/components/RecordCardBody.svelte";

	let { route, source, query, mark, index, total, onstep, onopenbeside, onshowrows } = $props<{
		route: string;
		source?: RecordSource;
		query?: Citation;
		/** The citation's own words in the answer. */
		mark?: string;
		index: number;
		total: number;
		onstep: (dir: 1 | -1) => void;
		onopenbeside: () => void;
		onshowrows?: () => void;
	}>();

	const rows = $derived.by(() => {
		const d = (query?.data ?? {}) as Record<string, unknown>;
		const list = Array.isArray(d.rows) ? (d.rows as Record<string, unknown>[]) : [];
		const columns = (Array.isArray(d.columns) ? (d.columns as string[]) : Object.keys(list[0] ?? {}))
			.filter((c) => c !== "ref" && c !== "id")
			.slice(0, 3);
		const count = typeof d.row_count === "number" ? d.row_count : list.length;
		return { list: list.slice(0, 3), columns, count, truncated: d.truncated === true };
	});

	const cell = (v: unknown) => (v === null || v === undefined ? "" : typeof v === "object" ? JSON.stringify(v) : String(v));
</script>

{#if source}
	<RecordCardBody
		ontology={source.ontology}
		recordId={source.recordId}
		passage={source.passage}
		title={source.title}
		{mark}
	/>
{:else if query}
	<div class="query-card">
		<div class="qc-meta">Query · counted on your server</div>
		<div class="qc-title">{rows.count === 1 ? "1 row" : `${rows.count.toLocaleString()} rows`}</div>
		{#if rows.list.length && rows.columns.length}
			<table class="qc-rows">
				<thead><tr>{#each rows.columns as c (c)}<th>{c}</th>{/each}</tr></thead>
				<tbody>
					{#each rows.list as r, i (i)}
						<tr>{#each rows.columns as c (c)}<td>{cell(r[c])}</td>{/each}</tr>
					{/each}
				</tbody>
			</table>
		{/if}
		<div class="qc-sub">
			Your server ran this query, and the model read {rows.truncated ? "the first part of" : ""} its rows.
		</div>
	</div>
{/if}

<div class="cc-foot">
	{#if total > 1}
		<span class="cc-step">
			<button type="button" class="cc-icon" aria-label="Previous source" onclick={() => onstep(-1)}>
				<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M9.5 4 5.5 8l4 4" /></svg>
			</button>
			<span class="cc-count">{index + 1} of {total}</span>
			<button type="button" class="cc-icon" aria-label="Next source" onclick={() => onstep(1)}>
				<svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6.5 4l4 4-4 4" /></svg>
			</button>
		</span>
	{:else}
		<span></span>
	{/if}
	{#if source}
		<button type="button" class="cc-action" onclick={onopenbeside}>Open beside</button>
	{:else if query && onshowrows}
		<button type="button" class="cc-action" onclick={onshowrows}>Show all rows</button>
	{/if}
</div>

<style>
	.query-card {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 16px 4px;
	}
	.qc-meta {
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
	.qc-title {
		font-size: 15px;
		color: var(--color-foreground);
	}
	.qc-rows {
		width: 100%;
		border-collapse: collapse;
		font-size: 12px;
		table-layout: fixed;
	}
	.qc-rows th {
		text-align: left;
		font-weight: 400;
		color: var(--color-foreground-subtle);
		padding: 0 8px 4px 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.qc-rows td {
		padding: 4px 8px 4px 0;
		color: var(--color-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.qc-sub {
		font-size: 13px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}
	.cc-foot {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 12px;
		padding: 4px 12px 8px 8px;
		font-size: 12px;
		color: var(--color-foreground-subtle);
	}
	.cc-step {
		display: flex;
		align-items: center;
		gap: 4px;
	}
	.cc-count {
		min-width: 44px;
		text-align: center;
	}
	.cc-icon {
		width: 28px;
		height: 28px;
		padding: 0;
		border: none;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--color-foreground-muted);
		display: flex;
		align-items: center;
		justify-content: center;
		cursor: pointer;
		transition: background-color var(--motion-hover-out) var(--ease-premium);
	}
	.cc-icon:hover {
		background: var(--hover-bg);
		color: var(--color-foreground);
		transition-duration: var(--motion-hover-in);
	}
	.cc-icon:active {
		background: var(--press-bg);
	}
	.cc-action {
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		font-size: 12px;
		color: var(--color-primary);
		cursor: pointer;
		border-bottom: 1px dotted color-mix(in srgb, var(--color-primary) 55%, transparent);
	}
	.cc-action:hover {
		border-bottom-color: var(--color-primary);
	}
</style>
