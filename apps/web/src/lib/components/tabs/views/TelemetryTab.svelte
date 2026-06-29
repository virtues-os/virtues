<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { formatMicrosPrecise } from "$lib/utils/currency";
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
	// ─── Action run metrics ───────────────────────────────────────────────────
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
	// ─── AI-call log ──────────────────────────────────────────────────────────
	type AiCall = {
		created_at: string;
		feature: string | null;
		model: string | null;
		prompt_tokens: number;
		completion_tokens: number;
		reasoning_tokens: number;
		cost_micros: number;
		status: string;
	};

	let history = $state<Sample[]>([]);
	let metrics = $state<Metrics | null>(null);
	let aiCalls = $state<AiCall[]>([]);
	let loading = $state(true);
	let timer: ReturnType<typeof setInterval> | undefined;

	onMount(() => {
		void load();
		// Refresh on a calm cadence while the tab is open.
		timer = setInterval(() => void load(), 30_000);
	});
	onDestroy(() => timer && clearInterval(timer));

	async function load() {
		const [h, m, a] = await Promise.allSettled([
			fetch("/api/system/history?since_secs=86400").then((r) => r.json()),
			fetch("/api/metrics/activity").then((r) => r.json()),
			fetch("/api/telemetry/ai-calls").then((r) => r.json()),
		]);
		if (h.status === "fulfilled" && Array.isArray(h.value)) history = h.value;
		if (m.status === "fulfilled") metrics = m.value;
		if (a.status === "fulfilled" && Array.isArray(a.value)) aiCalls = a.value;
		loading = false;
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
	function tokens(c: AiCall): number {
		return c.prompt_tokens + c.completion_tokens + c.reasoning_tokens;
	}
</script>

<Page
	title="Telemetry"
	description="System history, background runs, and the AI-call log — all box-local, nothing leaves your machine."
	maxWidth="full"
>
	{#if loading}
		<div class="flex items-center justify-center h-40">
			<Icon icon="ri:loader-4-line" width="20" class="spin" />
		</div>
	{:else}
		<!-- System history sparklines -->
		<div class="border border-border rounded-lg p-6 mb-6">
			<div class="text-xs uppercase tracking-wide text-foreground-muted mb-4">
				System (last 24h)
			</div>
			{#if history.length < 2}
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

		<!-- Action run summary -->
		{#if metrics}
			<div class="border border-border rounded-lg p-6 mb-6">
				<div class="text-xs uppercase tracking-wide text-foreground-muted mb-4">
					Background runs
				</div>
				<div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-5">
					{#each [{ k: "Total", v: metrics.summary.total_jobs }, { k: "Succeeded", v: metrics.summary.succeeded }, { k: "Failed", v: metrics.summary.failed }, { k: "Records", v: metrics.summary.total_records_processed }] as stat (stat.k)}
						<div>
							<div class="text-2xl font-semibold text-foreground tabular-nums">{stat.v}</div>
							<div class="text-xs text-foreground-muted">{stat.k}</div>
						</div>
					{/each}
				</div>
				<!-- Per-action throughput -->
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
		{/if}

		<!-- AI-call log -->
		<div class="border border-border rounded-lg p-6 mb-6">
			<div class="text-xs uppercase tracking-wide text-foreground-muted mb-3">
				AI-call log
			</div>
			{#if aiCalls.length === 0}
				<div class="text-sm text-foreground-subtle">No AI calls recorded yet.</div>
			{:else}
				<div class="divide-y divide-border-subtle">
					{#each aiCalls as c (c.created_at + (c.model ?? ""))}
						<div class="flex items-center justify-between py-1.5 text-xs">
							<span class="flex items-center gap-2 min-w-0">
								<span class="text-foreground-subtle tabular-nums">{fmtTime(c.created_at)}</span>
								<span class="text-foreground">{c.feature ?? "—"}</span>
								<span class="text-foreground-subtle font-mono truncate">{c.model ?? ""}</span>
							</span>
							<span class="flex items-center gap-3 tabular-nums shrink-0">
								<span class="text-foreground-subtle">{tokens(c)} tok</span>
								<span class="text-foreground">{formatMicrosPrecise(c.cost_micros)}</span>
							</span>
						</div>
					{/each}
				</div>
			{/if}
		</div>

		<!-- Recent errors -->
		{#if metrics && metrics.recent_errors.length > 0}
			<div class="border border-border rounded-lg p-6">
				<div class="text-xs uppercase tracking-wide text-foreground-muted mb-3">
					Recent failures
				</div>
				<div class="space-y-2">
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
			</div>
		{/if}
	{/if}
</Page>
