<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import SettingsPage from "$lib/components/settings/SettingsPage.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import { apiGet } from "$lib/api/client";
	import { onMount } from "svelte";
	import { getBackupStatus } from "$lib/api/client";
	import { backupSummary } from "$lib/settings/backup";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { formatTemperature } from "$lib/utils/units";
	import { units } from "$lib/stores/units.svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// This page measures the machine. Versions are on the server's device page
	// (SoftwareView), and its settings are on the Settings page.

	// Detail lives ON the page, not in the pane toolbar. The toolbar is shared
	// chrome — the same strip whatever tab you are in — so a control that only
	// means something on this one screen reads there as an app-wide mode. It
	// also sat far from the thing it changes: the extra rows appear inside the
	// sections below, and the switch for them was in a different region of the
	// window entirely (2026-08-13).
	function toggleDetail() {
		detail = !detail;
		loadTelemetry();
	}

	let loading = $state(true);
	let detail = $state(false); // dev-mode "Detail" layer

	// ─── /health (service liveness) ─────────────────────────────────────────
	let serverStatus = $state("unknown");
	let database = $state("unknown");

	// ─── /api/system/telemetry (live) ───────────────────────────────────────
	type Telemetry = any;
	let t = $state<Telemetry | null>(null);
	let rawOpen = $state(false);

	// Small rolling histories for the sparklines (newest last).
	const HIST = 48;
	let cpuHist = $state<number[]>([]);
	let memHist = $state<number[]>([]);
	let gpuHist = $state<number[]>([]);
	let netHist = $state<number[]>([]);


	function push(arr: number[], v: number): number[] {
		const next = arr.length >= HIST ? arr.slice(1) : arr.slice();
		next.push(v);
		return next;
	}

	// ─── Unit typesetting ───────────────────────────────────────────────────
	function splitBytes(n: number): { value: string; unit: string } {
		if (!n || n < 0) return { value: "0", unit: "B" };
		const units = ["B", "KB", "MB", "GB", "TB", "PB"];
		let i = 0;
		let v = n;
		while (v >= 1024 && i < units.length - 1) {
			v /= 1024;
			i++;
		}
		const value = v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1);
		return { value, unit: units[i] };
	}
	function bytesStr(n: number): string {
		const { value, unit } = splitBytes(n);
		return `${value} ${unit}`;
	}
	function rateStr(bytesPerSec: number): string {
		const { value, unit } = splitBytes(bytesPerSec);
		return `${value} ${unit}/s`;
	}
	function uptimeStr(secs: number): string {
		const d = Math.floor(secs / 86400);
		const h = Math.floor((secs % 86400) / 3600);
		const m = Math.floor((secs % 3600) / 60);
		if (d > 0) return `${d}d ${h}h ${m}m`;
		if (h > 0) return `${h}h ${m}m`;
		return `${m}m`;
	}
	// A processor that doesn't report its clock reads as its cores alone. Some
	// report a placeholder of a few MHz instead of nothing, which would print
	// "0.00 GHz", so anything under 100 MHz counts as unreported.
	function cores(cpu: { logical_cores: number; frequency_mhz: number }): string {
		const n = `${cpu.logical_cores} cores`;
		return cpu.frequency_mhz >= 100 ? `${n} · ${(cpu.frequency_mhz / 1000).toFixed(2)} GHz` : n;
	}
	const temp = (c: number) => formatTemperature(c, units.current, { scale: true });

	// Volumes that share one container (APFS's / and /System/Volumes/Data)
	// report the same space; each disk is shown once, under its shortest mount.
	const disks = $derived.by(() => {
		const seen = new Map<string, NonNullable<typeof t>["disks"][number]>();
		for (const d of t?.disks ?? []) {
			const key = `${d.total}:${d.available}`;
			const kept = seen.get(key);
			if (!kept || d.mount.length < kept.mount.length) seen.set(key, d);
		}
		return [...seen.values()];
	});

	// Pressure → semantic class (calm neutral → warning → error). We never
	// assign arbitrary colors; utilization is genuinely semantic.
	function pressure(pct: number): "ok" | "warn" | "crit" {
		if (pct >= 90) return "crit";
		if (pct >= 70) return "warn";
		return "ok";
	}

	// ─── Derived vitals ─────────────────────────────────────────────────────
	const cpuPct = $derived(t ? Math.round(t.cpu?.usage_pct ?? 0) : 0);
	const memPct = $derived(
		t && t.memory?.total ? Math.round((t.memory.used / t.memory.total) * 100) : 0,
	);
	const gpuPct = $derived(t?.gpu?.usage_pct != null ? Math.round(t.gpu.usage_pct) : null);

	// Sparkline path from a history array, normalized to a 100×28 viewbox.
	function sparkPath(hist: number[], max = 100): string {
		if (hist.length < 2) return "";
		const w = 100;
		const h = 28;
		const m = Math.max(max, ...hist, 1);
		const step = w / (HIST - 1);
		return hist
			.map((v, i) => {
				const x = (i + (HIST - hist.length)) * step;
				const y = h - (Math.min(v, m) / m) * (h - 2) - 1;
				return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
			})
			.join(" ");
	}

	// ─── Persisted history (last 24h) ───────────────────────────────────────
	// The live sparklines above are a rolling 48-sample window — about two and a
	// half minutes. This is the same four readings off the box's own stored
	// samples, which is the only thing on the page that can answer "was it like
	// this overnight?". It lived on the billing page until 2026-09-03, where a
	// temperature chart sat under a wallet balance.
	type Sample = {
		sampled_at: string;
		cpu_pct: number | null;
		mem_used_bytes: number | null;
		mem_total_bytes: number | null;
		gpu_pct: number | null;
		temp_c: number | null;
	};
	let history = $state<Sample[]>([]);

	async function loadHistory() {
		// Absent is fine here and only here: a box that booted minutes ago has
		// no stored samples yet, and the chapter says so rather than erroring.
		try {
			const h = await apiGet<Sample[]>("/system/history", { since_secs: 86400 });
			if (Array.isArray(h)) history = h;
		} catch (e) {
			console.error("system history failed", e);
		}
	}

	// Full-width path over an arbitrary-length series (the vitals' sparkPath is
	// pinned to the HIST window and would draw this squeezed into a corner).
	function historyPath(values: (number | null)[], max?: number): string {
		const v = values.map((x) => x ?? 0);
		if (v.length < 2) return "";
		const hi = max ?? Math.max(1, ...v);
		const w = 240;
		const h = 36;
		const step = w / (v.length - 1);
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
	const hasGpuHistory = $derived(history.some((s) => s.gpu_pct != null));
	const hasTempHistory = $derived(history.some((s) => s.temp_c != null));

	// Backup freshness. Polled once on mount, not on the 3s telemetry cadence —
	// it changes nightly, and a number that only moves once a day has no business
	// on a live ticker.
	let backup = $state<import("$lib/api/client").BackupStatus | null>(null);

	async function loadBackup() {
		try {
			backup = await getBackupStatus();
		} catch {
			backup = null;
		}
	}

	async function loadHealth() {
		try {
			const r = await fetch("/health");
			if (r.ok) {
				const d = await r.json();
				serverStatus = d.status || "unknown";
				database = d.database || "unknown";
			}
		} catch (e) {
			console.error("health failed", e);
		}
	}

	async function loadTelemetry() {
		try {
			// Ask for the process table only while the Detail panel is open —
			// process enumeration is the heaviest sample, so the default poll skips it.
			const d = await apiGet<Telemetry>(
				"/system/telemetry",
				detail ? { processes: 1 } : undefined,
			);
			t = d;
			cpuHist = push(cpuHist, Math.round(d.cpu?.usage_pct ?? 0));
			memHist = push(
				memHist,
				d.memory?.total ? Math.round((d.memory.used / d.memory.total) * 100) : 0,
			);
			if (d.gpu?.usage_pct != null) gpuHist = push(gpuHist, Math.round(d.gpu.usage_pct));
			netHist = push(netHist, (d.network?.rx_per_sec ?? 0) + (d.network?.tx_per_sec ?? 0));
		} catch (e) {
			console.error("telemetry failed", e);
		}
	}

	onMount(() => {
		void Promise.all([loadHealth(), loadTelemetry(), loadBackup(), loadHistory()]).then(
			() => (loading = false)
		);
	});

	// Calm cadence: the machine breathes, it doesn't twitch. Only while this
	// view is on screen, and started here rather than after an await, where a
	// view closed during the await kept fetching telemetry forever.
	$effect(() => {
		if (!active) return;
		const timer = setInterval(() => {
			if (document.visibilityState === "visible") void loadTelemetry();
		}, 3000);
		return () => clearInterval(timer);
	});
</script>

{#snippet vital(name: string, pct: number | null, big: string, unit: string, hist: number[], sub: string)}
	<div class="vital">
		<div class="vital-head">
			<span class="vital-name">{name}</span>
			{#if pct != null}
				<span class="vital-pct mono {pressure(pct)}">{pct}<span class="pct-sign">%</span></span>
			{/if}
		</div>
		<div class="vital-figure">
			<span class="vital-big mono">{big}</span>
			<span class="vital-unit">{unit}</span>
		</div>
		<svg class="spark {pct != null ? pressure(pct) : 'ok'}" viewBox="0 0 100 28" preserveAspectRatio="none">
			<path d={sparkPath(hist)} fill="none" vector-effect="non-scaling-stroke" />
		</svg>
		{#if pct != null}
			<div class="meter"><div class="meter-fill {pressure(pct)}" style="width:{Math.min(pct, 100)}%"></div></div>
		{/if}
		<div class="vital-sub">{sub}</div>
	</div>
{/snippet}

{#snippet ledger(label: string, value: string, mono = false, status: "" | "ok" | "warn" | "crit" = "")}
	<div class="ledger-row">
		<span class="ledger-label">{label}</span>
		<span class="leader"></span>
		<span class="ledger-value {mono ? 'mono' : ''} {status}">{value}</span>
	</div>
{/snippet}

<SettingsPage
	title="How your server is doing"
	lede="Its processor, memory, storage and services, now and over the last day."
	width="medium"
>
	{#snippet actions()}
		<button class="detail-btn" class:on={detail} onclick={toggleDetail} aria-pressed={detail}>
			<Icon icon="mynaui:terminal" width="14" />
			<span>Detail</span>
		</button>
	{/snippet}

	{#if loading}
		<div class="flex items-center justify-center h-64">
			<Icon icon="mynaui:spinner" width="20" class="spin" />
		</div>
	{:else}
		<!-- In the order an operator asks: is it healthy (vitals, thermal,
		     service), what is it doing (processes, inference), what is it
		     holding (storage, backup), and what is it (host). -->
		<!-- ─── VITALS ─────────────────────────────────────────────────── -->
		<section class="chapter">
			<h2 class="settings-label">Vitals</h2>
			<div class="vitals-grid">
				{@render vital(
					"Processor",
					cpuPct,
					String(cpuPct),
					"%",
					cpuHist,
					t?.cpu ? cores(t.cpu) : "",
				)}
				{@render vital(
					"Memory",
					memPct,
					splitBytes(t?.memory?.used ?? 0).value,
					splitBytes(t?.memory?.used ?? 0).unit,
					memHist,
					t?.memory ? `of ${bytesStr(t.memory.total)} · ${bytesStr(t.memory.available)} free` : "",
				)}
				{#if t?.gpu}
					{@render vital(
						"Graphics",
						gpuPct,
						gpuPct != null ? String(gpuPct) : "—",
						"%",
						gpuHist,
						gpuPct == null
							? "Starting"
							: t.gpu.offload_active
								? "Search runs on it"
								: "Search runs on the processor instead",
					)}
				{/if}
				{@render vital(
					"Network",
					null,
					rateStr((t?.network?.rx_per_sec ?? 0) + (t?.network?.tx_per_sec ?? 0)).split(" ")[0],
					rateStr((t?.network?.rx_per_sec ?? 0) + (t?.network?.tx_per_sec ?? 0)).split(" ").slice(1).join(" "),
					netHist,
					t?.network ? `↓ ${rateStr(t.network.rx_per_sec)}  ↑ ${rateStr(t.network.tx_per_sec)}` : "",
				)}
			</div>

			{#if detail && t?.cpu?.per_core?.length}
				<div class="cores">
					{#each t.cpu.per_core as c, i}
						<div class="core" title={`core ${i}: ${Math.round(c)}%`}>
							<div class="core-fill {pressure(c)}" style="height:{Math.max(c, 2)}%"></div>
						</div>
					{/each}
				</div>
				<div class="core-legend mono">
					load {t.cpu.load_avg.one.toFixed(2)} · {t.cpu.load_avg.five.toFixed(2)} · {t.cpu.load_avg.fifteen.toFixed(2)}
					&nbsp;·&nbsp; {t.cpu.brand}
				</div>
			{/if}
		</section>

		<!-- ─── THERMAL (detail) ───────────────────────────────────────── -->
		{#if detail && t?.thermal?.length}
			<section class="chapter">
				<h2 class="settings-label">Thermal</h2>
				<div class="cols">
					<div class="col">
						{#each t.thermal as s}
							{@render ledger(s.label, temp(s.temp_c), true, pressure(s.temp_c))}
						{/each}
					</div>
					<div class="col">
						{#if t.gpu?.temp_c != null}{@render ledger("GPU temperature", temp(t.gpu.temp_c), true)}{/if}
						{#if t.gpu?.power_mw != null}{@render ledger("GPU power", `${(t.gpu.power_mw / 1000).toFixed(2)} W`, true)}{/if}
						{#if t.gpu?.mem_total}{@render ledger("GPU memory", `${bytesStr(t.gpu.mem_used ?? 0)} / ${bytesStr(t.gpu.mem_total)}`, true)}{/if}
					</div>
				</div>
			</section>
		{/if}

		<!-- ─── LAST 24 HOURS ──────────────────────────────────────────── -->
		<!-- Directly under the live vitals, because it is those same four
		     readings with a memory. -->
		<section class="chapter">
			<h2 class="settings-label">Last 24 hours</h2>
			{#if history.length < 2}
				<p class="history-empty">
					Collecting samples. History appears once your server has run a few minutes.
				</p>
			{:else}
				<div class="history-grid">
					{#each [{ label: "Processor", series: cpuSeries, max: 100, pct: true, show: true }, { label: "Memory", series: memSeries, max: 100, pct: true, show: true }, { label: "Graphics", series: gpuSeries, max: 100, pct: true, show: hasGpuHistory }, { label: "Temperature", series: tempSeries, max: undefined, pct: false, show: hasTempHistory }] as chart (chart.label)}
						{#if chart.show}
							<div class="history-cell">
								<div class="history-head">
									<span class="vital-name">{chart.label}</span>
									<span class="vital-pct mono"
										>{chart.pct ? `${(chart.series.at(-1) ?? 0).toFixed(0)}%` : temp(chart.series.at(-1) ?? 0)}</span
									>
								</div>
								<svg class="spark tall" viewBox="0 0 240 36" preserveAspectRatio="none">
									<path
										d={historyPath(chart.series, chart.max)}
										fill="none"
										vector-effect="non-scaling-stroke"
									/>
								</svg>
							</div>
						{/if}
					{/each}
				</div>
			{/if}
		</section>

		<!-- ─── SERVICE ────────────────────────────────────────────────── -->
		<!-- What is live about the service: is it healthy, is the database
		     there, how much of the pool is in use. Versions are on the server's
		     device page. -->
		<section class="chapter">
			<h2 class="settings-label">Service</h2>
			<!-- Three facts, so ONE column rather than the two-column grid the
			     other chapters use: three splits 2/1 whichever way you cut it,
			     and a lone ledger row across from a pair reads as something
			     failed to load. The half-width measure is unchanged, so the
			     chapter still lines up with its neighbours. -->
			<div class="cols">
				<div class="col">
					{@render ledger("Status", serverStatus, false, serverStatus === "healthy" ? "ok" : "crit")}
					{@render ledger("Database", database, false, database === "connected" ? "ok" : "crit")}
					{@render ledger("Pool", t?.pool ? `${t.pool.idle} idle / ${t.pool.size} total` : "—", true)}
				</div>
			</div>

			{#if detail}
				<button class="raw-toggle mono" onclick={() => (rawOpen = !rawOpen)}>
					<Icon icon={rawOpen ? "mynaui:chevron-down" : "mynaui:chevron-right"} width="14" />
					raw telemetry
				</button>
				{#if rawOpen}
					<pre class="raw mono">{JSON.stringify(t, null, 2)}</pre>
				{/if}
			{/if}
		</section>

		<!-- ─── PROCESSES (detail) ─────────────────────────────────────── -->
		{#if detail && t?.processes?.length}
			<section class="chapter">
				<h2 class="settings-label">Processes</h2>
				<table class="data-table proc mono">
					<thead>
						<tr>
							<th class="rank">#</th>
							<th>process</th>
							<th class="num">pid</th>
							<th class="num">cpu</th>
							<th class="num">memory</th>
						</tr>
					</thead>
					<tbody>
						{#each t.processes as p, i}
							<tr>
								<td class="rank">{i + 1}</td>
								<td class="pname">{p.name}</td>
								<td class="num dim">{p.pid}</td>
								<td class="num {pressure(p.cpu_pct)}">{p.cpu_pct.toFixed(1)}%</td>
								<td class="num">{bytesStr(p.mem)}</td>
							</tr>
						{/each}
					</tbody>
				</table>
				<div class="table-caption">top {t.processes.length} by memory</div>
			</section>
		{/if}

		<!-- ─── INFERENCE ──────────────────────────────────────────────── -->
		<!-- Operator detail, like the process list: the Search page says
		     where search runs in plain words. -->
		{#if detail && t?.inference}
			<section class="chapter">
				<h2 class="settings-label">Inference</h2>
				<div class="cols">
					<div class="col">
						{@render ledger("Accelerator", t.inference.accelerator, true)}
						{@render ledger("Precision", t.inference.precision, true)}
						{@render ledger(
							"Models",
							t.inference.models_baked ? "All downloaded" : "Some missing",
							false,
							t.inference.models_baked ? "ok" : "warn",
						)}
						{#if t.gpu}
							{@render ledger(
								"Offload",
								t.gpu.offload_active ? "On the GPU" : "On the CPU",
								false,
								t.gpu.offload_active ? "ok" : "crit",
							)}
						{/if}
					</div>
					<div class="col">
						{#each t.inference.models as m}
							{@render ledger(m.name, m.source === "baked" ? "Downloaded" : "Missing", true, m.source === "baked" ? "ok" : "warn")}
						{/each}
						{#each t.services as s}
							{@render ledger(`${s.name} service`, s.up ? "Running" : "Stopped", true, s.up ? "ok" : "crit")}
						{/each}
					</div>
				</div>
			</section>
		{/if}

		<!-- ─── STORAGE ────────────────────────────────────────────────── -->
		{#if disks.length}
			<section class="chapter">
				<h2 class="settings-label">Storage</h2>
				<div class="drives">
					{#each disks as d (d.mount)}
						{@const usedPct = d.total ? Math.round(((d.total - d.available) / d.total) * 100) : 0}
						<div class="drive">
							<div class="drive-head">
								<span class="drive-mount mono">{d.mount}</span>
								<span class="drive-meta">{d.fs}{d.removable ? " · removable" : ""}</span>
							</div>
							<div class="meter tall"><div class="meter-fill {pressure(usedPct)}" style="width:{usedPct}%"></div></div>
							<div class="drive-foot">
								<span class="mono">{bytesStr(d.total - d.available)}</span> used
								<span class="leader"></span>
								<span class="mono">{bytesStr(d.available)}</span> free of <span class="mono">{bytesStr(d.total)}</span>
							</div>
						</div>
					{/each}
				</div>
			</section>
		{/if}

		<!-- ─── BACKUP ─────────────────────────────────────────────────── -->
		<!-- Backup's state, its drives and restoring are on their own page
		     (settings/BackupPage.svelte); this row says how old the newest
		     backup is and opens it. -->
		{#if backup}
			<section class="chapter">
				<h2 class="settings-label">Backup</h2>
				<button type="button" class="backup-link" onclick={() => windowShellStore.navigate('/virtues/backup', { label: 'Settings · Backup' })}>
					<span class:warn={backup.state !== "ok"}>{backupSummary(backup)}</span>
					<span class="open">Open</span>
				</button>
			</section>
		{/if}

		<!-- ─── HOST ───────────────────────────────────────────────────── -->
		<!-- The machine's own identity and the traffic crossing it. Reaching the
		     server away from home is on the Settings page; paired devices are
		     under Devices. -->
		<section class="chapter">
			<h2 class="settings-label">Host</h2>
			<div class="cols">
				<div class="col">
					{@render ledger("Hostname", t?.host?.hostname ?? "—", true)}
					{@render ledger("Uptime", t?.host ? uptimeStr(t.host.uptime_secs) : "—", true)}
					{@render ledger("Throughput", t?.network ? `↓ ${rateStr(t.network.rx_per_sec)}  ↑ ${rateStr(t.network.tx_per_sec)}` : "—", true)}
				</div>
				<div class="col">
					{@render ledger("OS", t?.host?.os ?? "—")}
					{@render ledger("Kernel", t?.host?.kernel ?? "—", true)}
				</div>
			</div>

			{#if detail && t?.network?.interfaces?.length}
				<table class="data-table mono">
					<thead><tr><th>interface</th><th class="num">rx total</th><th class="num">tx total</th></tr></thead>
					<tbody>
						{#each t.network.interfaces as iface}
							<tr><td>{iface.name}</td><td class="num">{bytesStr(iface.rx_total)}</td><td class="num">{bytesStr(iface.tx_total)}</td></tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</section>

	{/if}
</SettingsPage>

<style>
	.detail-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 3px 9px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: none;
		cursor: pointer;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.detail-btn:hover {
		background: color-mix(in srgb, var(--wash-ink) 6%, transparent);
		color: var(--color-foreground);
	}

	/* Held down, not merely hovered — this is a mode you are in. */
	.detail-btn.on {
		border-color: var(--color-primary);
		background: var(--primary-subtle);
		color: var(--color-foreground);
	}


	/* ─── Chapters ─────────────────────────────────────────────────────── */

	.chapter {
		padding-top: 28px;
		margin-top: 28px;
		border-top: 1px solid var(--border-subtle, var(--border));
	}
	.chapter:first-of-type { border-top: none; margin-top: 8px; padding-top: 8px; }

	/* ─── Vitals grid ──────────────────────────────────────────────────── */
	.vitals-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
		gap: 14px;
	}
	.vital {
		border: 1px solid var(--border-subtle, var(--border));
		border-radius: var(--radius-xl);
		padding: 16px;
		background: var(--surface-elevated);
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.vital-head { display: flex; align-items: baseline; justify-content: space-between; }
	.vital-name {
		font-size: 11px;
		color: var(--foreground-subtle);
	}
	.vital-pct { font-size: 12px; }
	.pct-sign { font-size: 9px; opacity: 0.6; margin-left: 1px; }
	.vital-figure { display: flex; align-items: baseline; gap: 5px; }
	.vital-big {
		font-size: 30px;
		line-height: 1;
		font-weight: 500;
		color: var(--foreground);
		font-feature-settings: "tnum";
	}
	.vital-unit { font-size: 12px; color: var(--foreground-subtle); }
	.vital-sub {
		font-size: 11px;
		color: var(--foreground-muted);
		min-height: 14px;
	}

	.spark { width: 100%; height: 24px; display: block; }
	.spark.tall { height: 36px; }

	/* ─── Last 24 hours ────────────────────────────────────────────────── */
	.history-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 14px;
	}
	@media (max-width: 880px) { .history-grid { grid-template-columns: 1fr; } }
	.history-cell {
		border: 1px solid var(--border-subtle, var(--border));
		border-radius: var(--radius-xl);
		padding: 16px;
		background: var(--surface-elevated);
	}
	.history-head {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		margin-bottom: 8px;
	}
	.history-empty { font-size: 12px; color: var(--foreground-subtle); }
	.spark path { stroke: var(--foreground-subtle); stroke-width: 1.25; opacity: 0.7; transition: stroke 600ms ease; }
	.spark.warn path { stroke: var(--warning); }
	.spark.crit path { stroke: var(--error); }

	/* ─── Meters (hairline) ────────────────────────────────────────────── */
	.meter {
		height: 3px;
		border-radius: var(--radius-xs);
		background: var(--border);
		overflow: hidden;
	}
	.meter.tall { height: 5px; }
	.meter-fill {
		height: 100%;
		border-radius: var(--radius-xs);
		background: var(--foreground);
		opacity: 0.55;
		transition: width 600ms var(--ease-premium, ease), background 400ms ease;
	}
	.meter-fill.warn { background: var(--warning); opacity: 0.85; }
	.meter-fill.crit { background: var(--error); opacity: 0.9; }

	.vital-pct.ok { color: var(--foreground-muted); }
	.vital-pct.warn { color: var(--warning); }
	.vital-pct.crit { color: var(--error); }

	/* ─── Per-core strip (detail) ──────────────────────────────────────── */
	.cores {
		display: flex;
		gap: 3px;
		align-items: flex-end;
		height: 36px;
		margin-top: 16px;
	}
	.core {
		flex: 1;
		height: 100%;
		background: var(--border);
		border-radius: var(--radius-xs);
		display: flex;
		align-items: flex-end;
		overflow: hidden;
		min-width: 4px;
	}
	.core-fill { width: 100%; background: var(--foreground); opacity: 0.5; transition: height 500ms ease; border-radius: var(--radius-xs); }
	.core-fill.warn { background: var(--warning); opacity: 0.85; }
	.core-fill.crit { background: var(--error); opacity: 0.9; }
	.core-legend { font-size: 11px; color: var(--foreground-subtle); margin-top: 8px; }

	/* ─── Ledger rows (dot leaders) ────────────────────────────────────── */
	.cols { display: grid; grid-template-columns: 1fr 1fr; gap: 14px 48px; }
	@media (max-width: 720px) { .cols { grid-template-columns: 1fr; } }
	.col { display: flex; flex-direction: column; gap: 11px; }
	/* Wraps rather than overflows. Label and value both refuse to shrink (a
	   truncated reading is a wrong reading), so a long pair — "Accelerator"
	   against "llama-server (GPU or CPU per sidecar build)" — needs 397px and
	   had been scrolling the whole page sideways on a phone. Wrapped, the
	   leader runs out to the end of the first line and the value takes the
	   second, right-aligned by the auto margin; on a wide window nothing
	   changes, because nothing wraps. */
	.ledger-row { display: flex; align-items: baseline; gap: 4px; flex-wrap: wrap; }
	.ledger-label { font-size: 13px; color: var(--foreground); flex-shrink: 0; }
	.leader {
		flex: 1;
		border-bottom: 1px dotted var(--border);
		transform: translateY(-3px);
		min-width: 12px;
	}
	.ledger-value { font-size: 13px; color: var(--foreground-muted); flex-shrink: 0; user-select: text; margin-left: auto; }
	.ledger-value.mono { font-family: var(--font-mono); font-size: 12px; }
	.ledger-value.ok { color: var(--success); }
	.ledger-value.warn { color: var(--warning); }
	.ledger-value.crit { color: var(--error); }

	/* Prose under a chapter — the sentence a reading needs when the number
	   alone does not tell you what to do about it. */
	.note {
		font-size: 12px;
		color: var(--foreground-muted);
		margin: 10px 0 0;
		max-width: 60ch;
	}

	/* ─── Drives ───────────────────────────────────────────────────────── */
	.drives { display: flex; flex-direction: column; gap: 18px; }
	.drive { display: flex; flex-direction: column; gap: 7px; }
	.drive-head { display: flex; align-items: baseline; justify-content: space-between; }
	.drive-mount { font-size: 13px; color: var(--foreground); }
	.drive-meta { font-size: 11px; color: var(--foreground-subtle); }
	.drive-foot { display: flex; align-items: baseline; gap: 4px; font-size: 12px; color: var(--foreground-muted); }
	.drive-foot .mono { font-family: var(--font-mono); color: var(--foreground); }

	/* ─── Data table (detail) ──────────────────────────────────────────── */
	.data-table { width: 100%; border-collapse: collapse; margin-top: 18px; font-size: 12px; }
	.data-table th {
		text-align: left;
		font-weight: 400;
		color: var(--foreground-subtle);
		font-size: 10px;
		padding: 4px 8px;
		border-bottom: 1px solid var(--border);
	}
	.data-table td { padding: 5px 8px; color: var(--foreground-muted); border-bottom: 1px solid var(--border-subtle, var(--border)); }
	.data-table .num { text-align: right; font-feature-settings: "tnum"; }

	/* Process table specifics */
	.proc .rank { width: 28px; text-align: right; color: var(--foreground-subtle); padding-right: 12px; }
	.proc th.rank { text-align: right; }
	.proc .pname { color: var(--foreground); max-width: 320px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.proc td.dim { color: var(--foreground-subtle); }
	.proc td.num.ok { color: var(--foreground-muted); }
	.proc td.num.warn { color: var(--warning); }
	.proc td.num.crit { color: var(--error); }
	.table-caption { font-size: 11px; color: var(--foreground-subtle); margin-top: 8px; text-align: right; }

	/* ─── Raw JSON ─────────────────────────────────────────────────────── */
	.raw-toggle {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-size: 11px;
		color: var(--foreground-subtle);
		margin-top: 18px;
	}
	.raw-toggle:hover { color: var(--foreground-muted); }
	.raw {
		margin-top: 10px;
		padding: 14px;
		background: var(--surface-elevated);
		border: 1px solid var(--border-subtle, var(--border));
		border-radius: var(--radius-lg);
		font-size: 11px;
		line-height: 1.5;
		color: var(--foreground-muted);
		max-height: 360px;
		overflow: auto;
		white-space: pre;
	}

	.mono { font-family: var(--font-mono); }

	/* Backup's own page opens from here. */
	.backup-link {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		width: 100%;
		max-width: 60ch;
		padding: 8px 0;
		border: none;
		border-bottom: 1px solid var(--color-border-subtle);
		background: none;
		font: inherit;
		font-size: 14px;
		text-align: left;
		color: var(--color-foreground);
		cursor: pointer;
	}

	.backup-link .warn {
		color: var(--warning);
	}

	.backup-link .open {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
</style>
