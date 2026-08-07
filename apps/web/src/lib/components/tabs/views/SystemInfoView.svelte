<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { apiGet } from "$lib/api/client";
	import { formatDate } from "$lib/utils/dateUtils";
	import { onMount, onDestroy } from "svelte";
	import { paneActions } from "$lib/stores/paneActions.svelte";
	import { getBackupStatus } from "$lib/api/client";

	import { BUILD, buildLabel } from "$lib/build";
	import { shellIdentity, describeOtaCheck, type ShellIdentity } from "$lib/tauri/bridge";

	// @ts-ignore — Vite compile-time constant (see vite.config.ts + app.d.ts)
	const BUILD_COMMIT: string = __BUILD_COMMIT__;

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// ─── The three version lines ────────────────────────────────────────────
	// A box, a UI bundle, and a native shell, each with its own version and no
	// reason to agree. On 2026-08-05 a phone was running visibly newer UI than
	// the Mac beside it and the reason was not discoverable from either screen —
	// it took ssh and a git log. With OTA moving the UI independently of the
	// app, that question gets asked more often, not less.
	//
	// `Package` below is the box. These two are the other two: `BUILD` is this
	// bundle's own identity, baked at build time, and the shell reports whether
	// the bundle arrived over the air or shipped inside the app — which the
	// bundle itself cannot know.
	let shell = $state<ShellIdentity | null>(null);
	onMount(async () => {
		shell = await shellIdentity();
	});

	// A toggle, not an event — `active` renders it held down, so the toolbar can
	// show what mode the view is in rather than just what you can do to it.
	$effect(() =>
		paneActions.set(tab.id, [
			{
				id: "system.detail",
				label: "Detail",
				icon: "ri:terminal-line",
				active: detail,
				run: () => {
					detail = !detail;
					loadTelemetry();
				},
			},
		]),
	);

	let loading = $state(true);
	let detail = $state(false); // dev-mode "Detail" layer
	let live = $state(false); // a successful poll happened recently

	// ─── /health (static version + db) ──────────────────────────────────────
	let serverStatus = $state("unknown");
	let version = $state("");
	let commit = $state("");
	let builtAt = $state("");
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

	let pollTimer: ReturnType<typeof setInterval> | null = null;

	function push(arr: number[], v: number): number[] {
		const next = arr.length >= HIST ? arr.slice(1) : arr.slice();
		next.push(v);
		return next;
	}

	function formatBuildTime(iso: string): string {
		return formatDate(iso, {
			year: "numeric",
			month: "long",
			day: "numeric",
			hour: "numeric",
			minute: "2-digit",
		});
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
	function ghz(mhz: number): string {
		if (!mhz) return "—";
		return (mhz / 1000).toFixed(2) + " GHz";
	}

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

	/** "4 hours", "9 days" — the only figure this surface really carries. */
	function backupAge(secs: number | null): string {
		if (secs === null) return "never";
		const h = Math.floor(secs / 3600);
		if (h < 1) return "under an hour ago";
		if (h < 48) return `${h} hour${h === 1 ? "" : "s"} ago`;
		return `${Math.floor(h / 24)} days ago`;
	}

	async function loadHealth() {
		try {
			const r = await fetch("/health");
			if (r.ok) {
				const d = await r.json();
				serverStatus = d.status || "unknown";
				version = d.version || "";
				commit = d.commit || BUILD_COMMIT;
				builtAt = d.built_at || "";
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
			live = true;
			cpuHist = push(cpuHist, Math.round(d.cpu?.usage_pct ?? 0));
			memHist = push(
				memHist,
				d.memory?.total ? Math.round((d.memory.used / d.memory.total) * 100) : 0,
			);
			if (d.gpu?.usage_pct != null) gpuHist = push(gpuHist, Math.round(d.gpu.usage_pct));
			netHist = push(netHist, (d.network?.rx_per_sec ?? 0) + (d.network?.tx_per_sec ?? 0));
		} catch (e) {
			live = false;
			console.error("telemetry failed", e);
		}
	}

	onMount(async () => {
		await Promise.all([loadHealth(), loadTelemetry(), loadBackup()]);
		loading = false;
		// Calm cadence — the machine breathes, it doesn't twitch.
		pollTimer = setInterval(loadTelemetry, 3000);
	});

	onDestroy(() => {
		if (pollTimer) clearInterval(pollTimer);
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

<Page title="System" description="The machine, examined." maxWidth="wide">
	<!-- The Detail toggle moved to the pane toolbar; the live pill stayed. It
	     reports state rather than doing anything, and the action slot is for
	     things you can press. Putting a status light in a row of buttons would
	     invite people to click it. -->
	{#snippet actions()}
		<div class="head-actions">
			<span class="live" class:on={live}><span class="dot"></span>{live ? "live" : "—"}</span>
		</div>
	{/snippet}

	{#if loading}
		<div class="flex items-center justify-center h-64">
			<Icon icon="ri:loader-4-line" width="20" class="spin" />
		</div>
	{:else}
		<!-- ─── VITALS ─────────────────────────────────────────────────── -->
		<section class="chapter">
			<h2 class="chapter-title">Vitals</h2>
			<div class="vitals-grid">
				{@render vital(
					"Processor",
					cpuPct,
					String(cpuPct),
					"%",
					cpuHist,
					t?.cpu ? `${t.cpu.logical_cores} cores · ${ghz(t.cpu.frequency_mhz)}` : "",
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
							? "warming…"
							: t.gpu.offload_active
								? "GPU offload active"
								: "⚠ CPU fallback",
					)}
				{:else}
					<div class="vital muted-vital">
						<div class="vital-head"><span class="vital-name">Graphics</span></div>
						<div class="vital-figure"><span class="vital-big mono dim">CPU</span></div>
						<div class="vital-sub">No discrete GPU detected</div>
					</div>
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

		<!-- ─── INFERENCE ──────────────────────────────────────────────── -->
		{#if t?.inference}
			<section class="chapter">
				<h2 class="chapter-title">Inference</h2>
				<div class="cols">
					<div class="col">
						{@render ledger("Accelerator", t.inference.accelerator, true)}
						{@render ledger("Precision", t.inference.precision, true)}
						{@render ledger(
							"Models",
							t.inference.models_baked ? "all baked" : "incomplete",
							false,
							t.inference.models_baked ? "ok" : "warn",
						)}
						{#if t.gpu}
							{@render ledger(
								"Offload",
								t.gpu.offload_active ? "GPU active" : "CPU fallback",
								false,
								t.gpu.offload_active ? "ok" : "crit",
							)}
						{/if}
					</div>
					<div class="col">
						{#each t.inference.models as m}
							{@render ledger(m.name, m.source === "baked" ? "● on disk" : "○ missing", true, m.source === "baked" ? "ok" : "warn")}
						{/each}
						{#each t.services as s}
							{@render ledger(`${s.name} sidecar`, s.up ? "listening" : "down", true, s.up ? "ok" : "crit")}
						{/each}
					</div>
				</div>
			</section>
		{/if}

		<!-- ─── BACKUP ─────────────────────────────────────────────────── -->
		<!--
			Deliberately above Storage: "how much would I lose" is a more urgent
			question than "how full is it", and this is the only place the answer
			appears. There is no restore button — restore needs the service
			stopped, so the box cannot do it to itself, and a button would imply
			a capability that cannot exist.
		-->
		{#if backup}
			<section class="chapter">
				<h2 class="chapter-title">Backup</h2>
				<div class="cols">
					<div class="col">
						{#if backup.state === "none"}
							{@render ledger("Off-box copies", "none", false, "crit")}
						{:else}
							{@render ledger(
								"Last backup",
								backupAge(backup.age_seconds),
								false,
								backup.state === "ok" ? "ok" : backup.state === "never" ? "crit" : "warn",
							)}
						{/if}
					</div>
					<div class="col">
						{#each backup.volumes as v}
							{@render ledger(
								v.name,
								v.last_error ? "failing" : v.attached ? "attached" : "not attached",
								false,
								v.last_error ? "crit" : v.attached ? "ok" : "warn",
							)}
						{/each}
					</div>
				</div>
				{#if backup.state === "none"}
					<p class="note">
						This box holds one copy of its data. Register a drive with
						<code>virtues volumes add &lt;path&gt;</code>.
					</p>
				{:else if backup.volumes.some((v) => v.last_error)}
					<p class="note">
						{backup.volumes.find((v) => v.last_error)?.last_error}
					</p>
				{/if}
			</section>
		{/if}

		<!-- ─── STORAGE ────────────────────────────────────────────────── -->
		{#if t?.disks?.length}
			<section class="chapter">
				<h2 class="chapter-title">Storage</h2>
				<div class="drives">
					{#each t.disks as d}
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

		<!-- ─── NETWORK & DEVICES ──────────────────────────────────────── -->
		<section class="chapter">
			<h2 class="chapter-title">Network &amp; Devices</h2>
			<div class="cols">
				<div class="col">
					{@render ledger("Paired devices", t?.devices?.paired_wg != null ? `${t.devices.paired_wg}` : "—")}
					{@render ledger("Throughput", t?.network ? `↓ ${rateStr(t.network.rx_per_sec)}  ↑ ${rateStr(t.network.tx_per_sec)}` : "—", true)}
				</div>
				<div class="col">
					{@render ledger("Hostname", t?.host?.hostname ?? "—", true)}
					{@render ledger("OS", t?.host?.os ?? "—")}
					{@render ledger("Kernel", t?.host?.kernel ?? "—", true)}
					{@render ledger("Uptime", t?.host ? uptimeStr(t.host.uptime_secs) : "—", true)}
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

		<!-- ─── PROCESSES (detail) ─────────────────────────────────────── -->
		{#if detail && t?.processes?.length}
			<section class="chapter">
				<h2 class="chapter-title">Processes</h2>
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

		<!-- ─── THERMAL (detail) ───────────────────────────────────────── -->
		{#if detail && t?.thermal?.length}
			<section class="chapter">
				<h2 class="chapter-title">Thermal</h2>
				<div class="cols">
					<div class="col">
						{#each t.thermal as s}
							{@render ledger(s.label, `${s.temp_c.toFixed(1)} °C`, true, pressure(s.temp_c))}
						{/each}
					</div>
					<div class="col">
						{#if t.gpu?.temp_c != null}{@render ledger("GPU temp", `${t.gpu.temp_c.toFixed(1)} °C`, true)}{/if}
						{#if t.gpu?.power_mw != null}{@render ledger("GPU power", `${(t.gpu.power_mw / 1000).toFixed(2)} W`, true)}{/if}
						{#if t.gpu?.mem_total}{@render ledger("GPU memory", `${bytesStr(t.gpu.mem_used ?? 0)} / ${bytesStr(t.gpu.mem_total)}`, true)}{/if}
					</div>
				</div>
			</section>
		{/if}

		<!-- ─── ABOUT (static, demoted) ────────────────────────────────── -->
		<section class="chapter">
			<h2 class="chapter-title">About</h2>
			<div class="cols">
				<div class="col">
					{@render ledger("Status", serverStatus, false, serverStatus === "healthy" ? "ok" : "crit")}
					{@render ledger("Database", database, false, database === "connected" ? "ok" : "crit")}
					{@render ledger("Pool", t?.pool ? `${t.pool.idle} idle / ${t.pool.size} total` : "—", true)}
				</div>
				<div class="col">
					{@render ledger("Package", version || "—")}
					{@render ledger("Built", formatBuildTime(builtAt) || "—")}
					{@render ledger("Commit", commit ? commit.slice(0, 12) : "—", true)}
					<!--
						The other two version lines. "Interface" is this bundle; when it
						came over the air the shell knows its content hash and we show
						that, because two bundles can report the same version (every dev
						build says "dev") while being different builds. "App" only
						renders inside the native shell — in a browser there is no third
						artifact to name, and an em-dash there would imply one exists.
					-->
					{@render ledger(
						"Interface",
						shell?.activeBundle
							? `${buildLabel(BUILD)} · ota ${shell.activeBundle.slice(0, 8)}`
							: `${buildLabel(BUILD)} · bundled`,
						true
					)}
					{#if shell}
						{@render ledger(
							"App",
							`${shell.appVersion} · surface ${shell.commandSurface}`,
							true
						)}
						<!--
							Only speaks when there is something to say. The loud case is
							a shell too old for the bundle the box offers: everything is
							working correctly and the user still sees stale UI, which
							without a reason on screen reads as OTA being broken.
						-->
						{#if describeOtaCheck(shell.lastCheck)}
							<p class="ota-note">{describeOtaCheck(shell.lastCheck)}</p>
						{/if}
					{/if}
				</div>
			</div>

			{#if detail}
				<button class="raw-toggle mono" onclick={() => (rawOpen = !rawOpen)}>
					<Icon icon={rawOpen ? "ri:arrow-down-s-line" : "ri:arrow-right-s-line"} width="14" />
					raw telemetry
				</button>
				{#if rawOpen}
					<pre class="raw mono">{JSON.stringify(t, null, 2)}</pre>
				{/if}
			{/if}
		</section>
	{/if}
</Page>

<style>
	/* ─── Head actions ─────────────────────────────────────────────────── */
	.head-actions {
		display: flex;
		align-items: center;
		gap: 14px;
	}
	.live {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--foreground-subtle);
	}
	.live .dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--foreground-disabled, var(--foreground-subtle));
	}
	.live.on .dot {
		background: var(--success);
		animation: breathe 3s var(--ease-in-out-quad, ease-in-out) infinite;
	}
	@keyframes breathe {
		0%, 100% { opacity: 0.35; }
		50% { opacity: 1; }
	}
	.toggle {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 12px;
		color: var(--foreground-muted);
		padding: 4px 9px;
		border: 1px solid var(--border);
		border-radius: 6px;
		transition: all var(--duration-fast) ease;
	}
	.toggle:hover { color: var(--foreground); border-color: var(--border-strong, var(--foreground-subtle)); }
	.toggle.active {
		color: var(--foreground);
		border-color: var(--foreground-subtle);
		background: var(--surface-elevated);
	}

	/* ─── Chapters ─────────────────────────────────────────────────────── */
	.chapter {
		padding-top: 28px;
		margin-top: 28px;
		border-top: 1px solid var(--border-subtle, var(--border));
	}
	.chapter:first-of-type { border-top: none; margin-top: 8px; padding-top: 8px; }
	.chapter-title {
		font-family: var(--font-serif);
		font-size: 19px;
		font-weight: 500;
		letter-spacing: 0.01em;
		color: var(--foreground);
		margin-bottom: 18px;
	}

	/* ─── Vitals grid ──────────────────────────────────────────────────── */
	.vitals-grid {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 14px;
	}
	@media (max-width: 880px) { .vitals-grid { grid-template-columns: repeat(2, 1fr); } }
	.vital {
		border: 1px solid var(--border-subtle, var(--border));
		border-radius: 10px;
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
	.vital-big.dim { color: var(--foreground-muted); font-size: 22px; }
	.vital-unit { font-size: 12px; color: var(--foreground-subtle); }
	.vital-sub {
		font-size: 11px;
		color: var(--foreground-muted);
		min-height: 14px;
	}
	.muted-vital .vital-sub { padding-top: 4px; }

	.spark { width: 100%; height: 24px; display: block; }
	.spark path { stroke: var(--foreground-subtle); stroke-width: 1.25; opacity: 0.7; transition: stroke 600ms ease; }
	.spark.warn path { stroke: var(--warning); }
	.spark.crit path { stroke: var(--error); }

	/* ─── Meters (hairline) ────────────────────────────────────────────── */
	.meter {
		height: 3px;
		border-radius: 2px;
		background: var(--border);
		overflow: hidden;
	}
	.meter.tall { height: 5px; }
	.meter-fill {
		height: 100%;
		border-radius: 2px;
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
		border-radius: 2px;
		display: flex;
		align-items: flex-end;
		overflow: hidden;
		min-width: 4px;
	}
	.core-fill { width: 100%; background: var(--foreground); opacity: 0.5; transition: height 500ms ease; border-radius: 2px; }
	.core-fill.warn { background: var(--warning); opacity: 0.85; }
	.core-fill.crit { background: var(--error); opacity: 0.9; }
	.core-legend { font-size: 11px; color: var(--foreground-subtle); margin-top: 8px; }

	/* ─── Ledger rows (dot leaders) ────────────────────────────────────── */
	.cols { display: grid; grid-template-columns: 1fr 1fr; gap: 14px 48px; }
	@media (max-width: 720px) { .cols { grid-template-columns: 1fr; } }
	.col { display: flex; flex-direction: column; gap: 11px; }
	/* Only rendered when there is something to say — see describeOtaCheck. */
	.ota-note {
		margin: 0.5rem 0 0;
		font-size: 0.75rem;
		line-height: 1.4;
		color: var(--warning);
	}

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
	.note code {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--foreground);
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
		border-radius: 8px;
		font-size: 11px;
		line-height: 1.5;
		color: var(--foreground-muted);
		max-height: 360px;
		overflow: auto;
		white-space: pre;
	}

	.mono { font-family: var(--font-mono); }
</style>
