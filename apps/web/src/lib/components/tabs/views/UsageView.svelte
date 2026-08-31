<!--
	Settings → Usage. What the box has been doing and what it cost.

	Was "Telemetry", filed under Developer — a word for something you'd send
	somewhere, on a shelf for people who write SQL, for a page whose main content
	is the user's own AI spend. It is neither: nothing here leaves the box, and
	the AI-call log is the only window onto a runaway applet burning the wallet.

	The call log is served a page at a time. It used to render a bare `LIMIT 100`
	as an unpaginated list of divs, which is both the wrong 100 rows (no search,
	no way to reach row 101) and a hand-rolled table sitting next to the grid
	every other list in the app uses.

	The old Billing page carried a second "Usage" panel — a wallet headline and a
	spend breakdown — that never loaded. It's gone rather than moved: this page
	is the one that works.
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page } from "$lib";
	import UniversalDataGrid, {
		type Column,
	} from "$lib/components/datagrid/UniversalDataGrid.svelte";
	import type { GridQuery, GridPage } from "$lib/components/datagrid/types";
	import { formatMicrosPrecise } from "$lib/utils/currency";
	import { apiGet, getMetricsActivity, getAiCallsPage, type AiCallRow } from "$lib/api/client";
	import { onMount, onDestroy } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// ─── Persisted system history ─────────────────────────────────────────────
	type Sample = {
		sampled_at: string;
		cpu_pct: number | null;
		mem_used_bytes: number | null;
		mem_total_bytes: number | null;
		gpu_pct: number | null;
		temp_c: number | null;
	};
	// ─── Applet run metrics ───────────────────────────────────────────────────
	type Metrics = {
		summary: {
			total_jobs: number;
			succeeded: number;
			failed: number;
			active: number;
			success_rate_percent: number;
			total_records_processed: number;
		};
		by_stream: {
			stream_name: string;
			job_count: number;
			success_rate_percent: number;
			total_records: number;
			last_sync_at: string | null;
		}[];
		recent_errors: {
			job_id: string;
			job_type: string;
			error_message: string;
			failed_at: string;
		}[];
	};

	let history = $state<Sample[]>([]);
	let metrics = $state<Metrics | null>(null);
	let loading = $state(true);
	let timer: ReturnType<typeof setInterval> | undefined;

	onMount(() => {
		void load();
		// Refresh on a calm cadence while the tab is open. The call log refreshes
		// itself — it's paged, and yanking rows out from under a reader who has
		// paged into the past is worse than a slightly stale page.
		timer = setInterval(() => void load(), 30_000);
	});
	onDestroy(() => timer && clearInterval(timer));

	async function load() {
		const [h, m] = await Promise.allSettled([
			apiGet<Sample[]>("/system/history", { since_secs: 86400 }),
			getMetricsActivity<Metrics>(),
		]);
		if (h.status === "fulfilled" && Array.isArray(h.value)) history = h.value;
		if (m.status === "fulfilled") metrics = m.value;
		loading = false;
	}

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

	// Build an SVG sparkline path from a numeric series (0-based y, auto-scaled).
	function sparkline(values: (number | null)[], max?: number): string {
		const v = values.map((x) => x ?? 0);
		if (v.length === 0) return "";
		const hi = max ?? Math.max(1, ...v);
		const w = 240;
		const h = 36;
		const step = v.length > 1 ? w / (v.length - 1) : w;
		return v
			.map((y, i) => {
				const px = (i * step).toFixed(1);
				const py = (h - (Math.min(y, hi) / hi) * h).toFixed(1);
				return `${i === 0 ? "M" : "L"}${px},${py}`;
			})
			.join(" ");
	}

	const cpuSeries = $derived(history.map((s) => s.cpu_pct));
	const memSeries = $derived(
		history.map((s) =>
			s.mem_total_bytes ? ((s.mem_used_bytes ?? 0) / s.mem_total_bytes) * 100 : 0,
		),
	);
	const gpuSeries = $derived(history.map((s) => s.gpu_pct));
	const tempSeries = $derived(history.map((s) => s.temp_c));
	const hasGpu = $derived(history.some((s) => s.gpu_pct != null));
	const hasTemp = $derived(history.some((s) => s.temp_c != null));

	function fmtTime(ts: string): string {
		return new Date(ts).toLocaleTimeString(undefined, {
			hour: "2-digit",
			minute: "2-digit",
		});
	}
</script>

<Page
	title="Usage"
	description="What your server has been running, and what each AI call cost the wallet — server-local, nothing leaves your machine. Calls on your own key show tokens, not a price we'd be guessing at."
	maxWidth="wide"
>
	<!-- AI-call log. First, because it is the one thing here you'd come back
	     for: every paid call, in order, with what it cost. -->
	<section class="block">
		<h2 class="settings-label">AI calls</h2>
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

	<!-- Background runs -->
	{#if metrics}
		<section class="block">
			<h2 class="settings-label">Background runs</h2>
			<div class="border border-border rounded-lg p-6">
				<div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-5">
					{#each [{ k: "Total", v: metrics.summary.total_jobs }, { k: "Succeeded", v: metrics.summary.succeeded }, { k: "Failed", v: metrics.summary.failed }, { k: "Records", v: metrics.summary.total_records_processed }] as stat (stat.k)}
						<div>
							<div class="text-2xl font-semibold text-foreground tabular-nums">{stat.v}</div>
							<div class="text-xs text-foreground-muted">{stat.k}</div>
						</div>
					{/each}
				</div>
				{#if metrics.by_stream.length > 0}
					<div class="divide-y divide-border-subtle">
						{#each metrics.by_stream.slice(0, 12) as s (s.stream_name)}
							<div class="flex items-center justify-between py-2 text-sm">
								<span class="text-foreground truncate max-w-[50%]">{s.stream_name}</span>
								<span class="flex items-center gap-3 text-xs tabular-nums">
									<span class="text-foreground-subtle">{s.job_count} runs</span>
									<span class="text-foreground-subtle">{s.total_records} rec</span>
									<span
										class={s.success_rate_percent >= 90
											? "text-success"
											: s.success_rate_percent >= 50
												? "text-warning"
												: "text-error"}
										>{s.success_rate_percent.toFixed(0)}%</span
									>
								</span>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		</section>
	{/if}

	<!-- Recent failures -->
	{#if metrics && metrics.recent_errors.length > 0}
		<section class="block">
			<h2 class="settings-label">Recent failures</h2>
			<div class="border border-border rounded-lg p-6 space-y-2">
				{#each metrics.recent_errors as e (e.job_id)}
					<div class="text-xs">
						<div class="flex justify-between">
							<span class="text-foreground">{e.job_type}</span>
							<span class="text-foreground-subtle tabular-nums">{fmtTime(e.failed_at)}</span>
						</div>
						<div class="text-error font-mono break-words mt-0.5">{e.error_message}</div>
					</div>
				{/each}
			</div>
		</section>
	{/if}

	<!-- System history sparklines -->
	<section class="block">
		<h2 class="settings-label">System (last 24h)</h2>
		<div class="border border-border rounded-lg p-6">
			{#if loading && history.length === 0}
				<div class="text-sm text-foreground-subtle">Loading…</div>
			{:else if history.length < 2}
				<div class="text-sm text-foreground-subtle">
					Collecting samples… history appears after the box has run a few minutes.
				</div>
			{:else}
				<div class="grid grid-cols-1 md:grid-cols-2 gap-6">
					{#each [{ label: "CPU", series: cpuSeries, max: 100, show: true }, { label: "Memory", series: memSeries, max: 100, show: true }, { label: "GPU", series: gpuSeries, max: 100, show: hasGpu }, { label: "Temp °C", series: tempSeries, max: undefined, show: hasTemp }] as chart (chart.label)}
						{#if chart.show}
							<div>
								<div class="flex justify-between text-xs mb-1">
									<span class="text-foreground-muted">{chart.label}</span>
									<span class="text-foreground tabular-nums"
										>{(chart.series.at(-1) ?? 0).toFixed(0)}{chart.max === 100 ? "%" : ""}</span
									>
								</div>
								<svg viewBox="0 0 240 36" class="w-full h-9" preserveAspectRatio="none">
									<path
										d={sparkline(chart.series, chart.max)}
										fill="none"
										stroke="currentColor"
										stroke-width="1.5"
										class="text-foreground"
									/>
								</svg>
							</div>
						{/if}
					{/each}
				</div>
			{/if}
		</div>
	</section>
</Page>

<style>
	.block {
		margin-bottom: 2rem;
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
