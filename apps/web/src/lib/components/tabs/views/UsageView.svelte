<!--
	Billing → the call log. Every paid AI call, in order, with what it cost.

	This is the last chapter of Billing (see BillingView), not a page. It
	answers exactly one
	question — "where did the balance above go?" — and nothing else belongs in
	it. Three things that had accumulated here were removed on 2026-09-03:

	  Background runs · Recent failures — applet run health. It is real, but it
	    is not spending, and Applets already tells it better: the attention
	    strip names the applets that failed or silently stopped firing, and each
	    card carries its own run pulse and last-run status. An aggregate
	    success-rate panel on the billing page was a second, worse telling with
	    no way to act on it.
	  System (last 24h) — CPU/memory/GPU/temperature sparklines, which are the
	    machine, measured. They moved to Settings → System, beside the live
	    vitals they are the history of.

	The log is served a page at a time. It used to render a bare `LIMIT 100` as
	an unpaginated list of divs — the wrong 100 rows (no search, no way to reach
	row 101), and a hand-rolled table beside the grid every other list uses.
-->
<script lang="ts">
	import UniversalDataGrid, {
		type Column,
	} from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import type { GridQuery, GridPage } from "$lib/components/datagrid/types";
	import { formatMicrosPrecise } from "$lib/utils/currency";
	import { getAiCallsPage, type AiCallRow } from "$lib/api/client";

	// No props: the grid pages itself off the server, so this section has
	// nothing to learn from the tab it sits in.

	// ─── AI-call log (server-paged) ───────────────────────────────────────────
	async function fetchCalls(q: GridQuery): Promise<GridPage<AiCallRow>> {
		const page = await getAiCallsPage({
			offset: q.offset,
			limit: q.limit,
			search: q.search || undefined,
			// "When" is the only server-sortable column; anything else stays
			// newest-first.
			dir: q.sort?.key === "created_at" && q.sort.dir === "asc" ? "asc" : "desc",
		});
		return { items: page.items, total: page.total };
	}

	const callColumns: Column<AiCallRow>[] = [
		{
			key: "created_at",
			label: "When",
			icon: "ri:time-line",
			width: "20%",
			minWidth: "150px",
			sortable: true,
		},
		{ key: "feature", label: "Feature", icon: "ri:price-tag-3-line", width: "20%", minWidth: "120px" },
		{ key: "model", label: "Model", icon: "ri:cpu-line", width: "30%", minWidth: "170px" },
		{
			// Prompt + completion + reasoning: three columns of small numbers
			// nobody reads separately. `getValue` keeps the column keyed on a
			// real field while displaying the sum.
			key: "prompt_tokens",
			label: "Tokens",
			icon: "ri:hashtag",
			width: "15%",
			minWidth: "100px",
			hideOnMobile: true,
			getValue: (c) => tokens(c),
		},
		{ key: "cost_micros", label: "Cost", icon: "ri:coin-line", width: "15%", minWidth: "100px" },
	];

	function tokens(c: AiCallRow): number {
		return c.prompt_tokens + c.completion_tokens + c.reasoning_tokens;
	}

	/**
	 * What a call cost, or an honest refusal to say.
	 *
	 * Only our own gateway reports `usage.cost`, so a BYO row's `cost_micros`
	 * is 0 meaning *unknown*, not free. Rendering "$0.00" there would read as a
	 * measurement — a whole month of BYO traffic totalling nothing — when it is
	 * the absence of one. The tokens are real and sit in the column beside it;
	 * the price belongs to the user's provider and is theirs to look up.
	 */
	function cost(c: AiCallRow): string {
		return c.route === "byo" ? "your key" : formatMicrosPrecise(c.cost_micros);
	}

	function fmtWhen(ts: string): string {
		return new Date(ts).toLocaleString(undefined, {
			month: "short",
			day: "numeric",
			hour: "2-digit",
			minute: "2-digit",
		});
	}
</script>

<!-- No <Page>: the last chapter of Billing. See BillingView. -->
<section class="chapter">
	<h2 class="settings-label">AI calls</h2>
	<!-- Names its relation to the wallet ledger above it. Two lists of
	     spending on one page, with nothing saying how they differ, is how you
	     get a reader adding them together. -->
	<p class="chapter-lede">
		Every paid call, itemized. The “Usage” lines in the wallet ledger above are
		these, totalled.
	</p>
	<UniversalDataGrid
		items={[]}
		columns={callColumns}
		entityType="ai-call"
		server={fetchCalls}
		pageSize={25}
		emptyIcon="ri:sparkling-line"
		emptyMessage="No AI calls recorded yet"
		loadingMessage="Reading the call log…"
		searchPlaceholder="Search by feature or model…"
		defaultViewMode="table"
	>
		{#snippet tableRow(call: AiCallRow)}
			<td class="cell when">{fmtWhen(call.created_at)}</td>
			<td class="cell">{call.feature ?? "—"}</td>
			<td class="cell mono">{call.model ?? "—"}</td>
			<td class="cell num hide-mobile">{tokens(call).toLocaleString()}</td>
			<td class="cell num" class:muted={call.route === "byo"}>{cost(call)}</td>
		{/snippet}
	</UniversalDataGrid>
</section>

<style>
	/* Same chapter rule as the sections above (see BillingView) — a hairline
	   under a small-caps eyebrow, not a bordered card. */
	.chapter {
		padding-top: 28px;
		margin-top: 28px;
		border-top: 1px solid var(--color-border-subtle);
	}
	.chapter-lede {
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
		margin: 0 0 14px;
		max-width: 60ch;
	}

	.cell {
		padding: 0.625rem 0.75rem;
		font-size: 0.8125rem;
		color: var(--color-foreground);
	}
	.cell.when {
		padding-left: 0;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.cell.mono {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
	}
	.cell.num {
		font-variant-numeric: tabular-nums;
	}
	.cell.num:last-child {
		padding-right: 0;
	}

	/* "your key" is prose in a column of figures — subdue it so a scan down the
	   column reads the numbers, and don't align it as though it were one. */
	.cell.num.muted {
		color: var(--color-foreground-subtle);
		font-variant-numeric: normal;
	}

	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}
	}
</style>
