<script lang="ts">
	/**
	 * Settings · Your server (/virtues/system): whether your server is all
	 * right, what it runs, and the machine itself.
	 *
	 * It opens with one sentence for the first thing that needs you
	 * (`serverLead` in lib/settings/serverStatus.ts). Below it: the disk and
	 * the backup, a row each for Updates, Search and Applets with where each
	 * stands, the last 24 hours from the server's stored samples
	 * (`/system/history`), and the machine's name, system and uptime. For
	 * developers, at the foot: Terminal, and Query and Streams in Sources.
	 *
	 * Nothing outside the troubleshooting fold changes while you read: the
	 * samples and the rows refresh once a minute. The fold holds the operator
	 * detail, and only while it is open does the page poll telemetry every
	 * 3 seconds, with the process table (the heaviest sample) included.
	 */
	import type { Tab } from '$lib/tabs/types';
	import { onMount } from 'svelte';
	import SettingsPage from '$lib/components/settings/SettingsPage.svelte';
	import LinkRow from '$lib/components/settings/LinkRow.svelte';
	import Fold from '$lib/components/settings/Fold.svelte';
	import Button from '$lib/components/Button.svelte';
	import {
		apiGet,
		getBackupStatus,
		getSearchStatus,
		getUpdateStatus,
		listApplets,
		type BackupStatus,
		type SearchStatus,
		type UpdateStatus,
	} from '$lib/api/client';
	import { DEFAULT_ASSISTANT_NAME } from '$lib/settings';
	import { backupSummary } from '$lib/settings/backup';
	import { openSettingsPage } from '$lib/settings/focus';
	import {
		DISK_FULL_PCT,
		appletsValue,
		dataDisk,
		diskUsedPct,
		formatBytes,
		searchValue,
		serverLead,
		updatesValue,
		upFor,
		type Health,
	} from '$lib/settings/serverStatus';
	import { needsYou } from '$lib/applets/days';
	import { formatTemperature } from '$lib/utils/units';
	import { units } from '$lib/stores/units.svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// ─── What the page reads ────────────────────────────────────────────────
	// Each read fails on its own: a value that didn't load leaves its row
	// without a value, and the opening sentence uses only what loaded.

	type Disk = { name: string; mount: string; fs: string; total: number; available: number; removable: boolean };
	type Telemetry = {
		host?: { hostname: string | null; os: string | null; kernel: string | null; arch: string; uptime_secs: number };
		cpu?: {
			brand: string;
			logical_cores: number;
			usage_pct: number;
			per_core: number[];
			frequency_mhz: number;
			load_avg: { one: number; five: number; fifteen: number };
		};
		memory?: { total: number; used: number; available: number };
		gpu?: {
			usage_pct: number | null;
			offload_active: boolean;
			temp_c: number | null;
			power_mw: number | null;
			mem_used: number | null;
			mem_total: number | null;
		} | null;
		disks?: Disk[];
		network?: { rx_per_sec: number; tx_per_sec: number; interfaces: { name: string; rx_total: number; tx_total: number }[] };
		thermal?: { label: string; temp_c: number }[];
		pool?: { size: number; idle: number };
		processes?: { name: string; pid: number; cpu_pct: number; mem: number }[];
		inference?: {
			accelerator: string;
			precision: string;
			models_baked: boolean;
			models: { name: string; source: string }[];
		};
		services?: { name: string; up: boolean }[];
	};
	type Sample = {
		sampled_at: string;
		cpu_pct: number | null;
		mem_used_bytes: number | null;
		mem_total_bytes: number | null;
		gpu_pct: number | null;
		temp_c: number | null;
	};

	let loaded = $state(false);
	let health = $state<Health | null>(null);
	let t = $state<Telemetry | null>(null);
	let history = $state<Sample[]>([]);
	let backup = $state<BackupStatus | null>(null);
	let update = $state<UpdateStatus | null>(null);
	let search = $state<SearchStatus | null>(null);
	/** Names of the applets that need you; null until the list loads. */
	let appletsNeedingYou = $state<string[] | null>(null);
	/** Whether the troubleshooting fold is open, set by the fold's contents. */
	let troubleOpen = $state(false);
	let rawOpen = $state(false);

	async function loadHealth() {
		try {
			const r = await fetch('/health', { cache: 'no-store' });
			const d = await r.json();
			health = { status: d.status ?? 'unknown', database: d.database ?? 'unknown' };
		} catch {
			// absent-ok: the page itself loaded from this server, so a failed
			// probe says nothing about the database; the sentence skips it.
			health = null;
		}
	}

	async function loadTelemetry() {
		try {
			t = await apiGet<Telemetry>('/system/telemetry', troubleOpen ? { processes: 1 } : undefined);
		} catch {
			// absent-ok: rows that need telemetry show no value.
		}
	}

	async function loadHistory() {
		try {
			const h = await apiGet<Sample[]>('/system/history', { since_secs: 86400 });
			if (Array.isArray(h)) history = h;
		} catch {
			// absent-ok: a server that booted minutes ago has no samples yet,
			// and the chapter says so.
		}
	}

	async function loadRows() {
		const [b, u, s, a] = await Promise.allSettled([getBackupStatus(), getUpdateStatus(), getSearchStatus(), listApplets()]);
		backup = b.status === 'fulfilled' ? b.value : null;
		update = u.status === 'fulfilled' ? u.value : null;
		search = s.status === 'fulfilled' ? s.value : null;
		appletsNeedingYou = a.status === 'fulfilled' ? a.value.filter((applet) => needsYou(applet)).map((applet) => applet.name) : null;
	}

	onMount(() => {
		void Promise.all([loadHealth(), loadTelemetry(), loadHistory(), loadRows()]).then(() => (loaded = true));
	});

	// Once a minute while this tab is on screen: the samples and the rows.
	// Started here rather than after an await, so a view closed during the
	// first load doesn't keep fetching.
	$effect(() => {
		if (!active) return;
		const timer = setInterval(() => {
			if (document.visibilityState !== 'visible') return;
			void loadHealth();
			void loadHistory();
			void loadRows();
			if (!troubleOpen) void loadTelemetry();
		}, 60_000);
		return () => clearInterval(timer);
	});

	// Every 3 seconds, only while the troubleshooting fold is open.
	$effect(() => {
		if (!active || !troubleOpen) return;
		const timer = setInterval(() => {
			if (document.visibilityState === 'visible') void loadTelemetry();
		}, 3000);
		return () => clearInterval(timer);
	});

	/** On the fold's contents, which exist only while it is open. */
	function whileOpen(_node: HTMLElement) {
		troubleOpen = true;
		void loadTelemetry();
		return {
			destroy() {
				troubleOpen = false;
				rawOpen = false;
			},
		};
	}

	// ─── The opening sentence and the rows ──────────────────────────────────

	const disk = $derived(dataDisk(t?.disks ?? []));
	const lead = $derived(serverLead({ health, disk, backup, appletsNeedingYou }));
	const diskPct = $derived(disk ? diskUsedPct(disk) : 0);

	// ─── Last 24 hours ──────────────────────────────────────────────────────

	const W = 300;
	const H = 36;
	const times = $derived(history.map((s) => new Date(s.sampled_at).getTime()));
	const t0 = $derived(times[0] ?? 0);
	const t1 = $derived(times.at(-1) ?? 0);

	/** The line and its faint area for one reading, x by time so a gap in the
	 *  samples stays a gap. A missing value breaks the line. */
	function chart(values: (number | null)[], lo: number, hi: number): { line: string; area: string } {
		const span = t1 - t0 || 1;
		const x = (i: number) => (((times[i] - t0) / span) * W).toFixed(1);
		const y = (v: number) => (H - 1 - ((Math.min(Math.max(v, lo), hi) - lo) / (hi - lo || 1)) * (H - 2)).toFixed(1);
		let line = '';
		let area = '';
		let run: number[] = [];
		const flush = () => {
			if (run.length > 1) {
				line += run.map((i, k) => `${k === 0 ? 'M' : 'L'}${x(i)},${y(values[i]!)}`).join(' ') + ' ';
				area += `M${x(run[0])},${H} ` + run.map((i) => `L${x(i)},${y(values[i]!)}`).join(' ') + ` L${x(run.at(-1)!)},${H} Z `;
			}
			run = [];
		};
		values.forEach((v, i) => (v == null ? flush() : run.push(i)));
		flush();
		return { line, area };
	}

	const cpuSeries = $derived(history.map((s) => s.cpu_pct));
	const memSeries = $derived(history.map((s) => (s.mem_total_bytes ? ((s.mem_used_bytes ?? 0) / s.mem_total_bytes) * 100 : null)));
	const gpuSeries = $derived(history.map((s) => s.gpu_pct));
	const tempSeries = $derived(history.map((s) => s.temp_c));
	const hasGpu = $derived(!!t?.gpu || gpuSeries.some((v) => v != null));
	const hasTemp = $derived(tempSeries.some((v) => v != null));

	/** Temperatures swing over a few degrees, so their scale is their own range. */
	const tempRange = $derived.by(() => {
		const v = tempSeries.filter((x): x is number => x != null);
		if (!v.length) return { lo: 0, hi: 100 };
		const lo = Math.min(...v);
		const hi = Math.max(...v);
		const pad = Math.max(5, (20 - (hi - lo)) / 2);
		return { lo: lo - pad, hi: hi + pad };
	});

	/** Times under the charts: where the samples start, midnight, 7 AM, and now. */
	const axis = $derived.by(() => {
		if (times.length < 2) return [];
		const span = t1 - t0;
		const hour = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour: 'numeric' });
		const marks: { label: string; pct: number }[] = [{ label: hour(t0), pct: 0 }];
		const midnight = new Date(t1);
		midnight.setHours(0, 0, 0, 0);
		const morning = new Date(midnight);
		morning.setHours(7);
		for (const [ms, label] of [
			[midnight.getTime(), 'Midnight'],
			[morning.getTime(), hour(morning.getTime())],
		] as const) {
			const pct = ((ms - t0) / span) * 100;
			if (pct > 12 && pct < 88 && marks.every((m) => Math.abs(m.pct - pct) >= 12)) marks.push({ label, pct });
		}
		marks.push({ label: 'Now', pct: 100 });
		return marks;
	});

	const cpuNow = $derived(t?.cpu ? `${Math.round(t.cpu.usage_pct)}%` : '');
	const memNow = $derived(t?.memory ? `${formatBytes(t.memory.used)} of ${formatBytes(t.memory.total)}` : '');
	const memPct = $derived(t?.memory?.total ? Math.round((t.memory.used / t.memory.total) * 100) : null);
	const gpuNow = $derived(t?.gpu?.usage_pct != null ? `${Math.round(t.gpu.usage_pct)}%` : '');
	const hottest = $derived.by(() => {
		const sensors = (t?.thermal ?? []).map((s) => s.temp_c);
		if (sensors.length) return Math.max(...sensors);
		if (t?.gpu?.temp_c != null) return t.gpu.temp_c;
		return tempSeries.findLast((v) => v != null) ?? null;
	});
	const temp = (c: number) => formatTemperature(c, units.current, { scale: true });

	// ─── Troubleshooting ────────────────────────────────────────────────────

	/** The interfaces that have carried anything, most first. A Mac lists two
	 *  dozen, most of them idle; the raw readings keep every one. */
	const busiest = $derived(
		(t?.network?.interfaces ?? [])
			.filter((i) => i.rx_total + i.tx_total > 0)
			.sort((a, b) => b.rx_total + b.tx_total - (a.rx_total + a.tx_total))
			.slice(0, 6),
	);

	function rate(bytesPerSec: number): string {
		return `${formatBytes(bytesPerSec)}/s`;
	}

	/** Calm, warning, error: a reading this high is genuinely worth a color. */
	function pressure(pct: number): 'ok' | 'warn' | 'crit' {
		if (pct >= 90) return 'crit';
		if (pct >= 70) return 'warn';
		return 'ok';
	}

	// A processor that doesn't report its clock reads as its cores alone. Some
	// report a few MHz instead of nothing, so under 100 MHz counts as unreported.
	function clock(cpu: NonNullable<Telemetry['cpu']>): string {
		return cpu.frequency_mhz >= 100 ? `${(cpu.frequency_mhz / 1000).toFixed(2)} GHz` : '';
	}
</script>

{#snippet fact(label: string, value: string, tone: '' | 'ok' | 'warn' | 'crit' = '')}
	<div class="fact">
		<span class="fact-label">{label}</span>
		<span class="fact-value {tone}">{value}</span>
	</div>
{/snippet}

{#snippet reading(label: string, sub: string, values: (number | null)[], lo: number, hi: number, now: string)}
	{@const c = chart(values, lo, hi)}
	<div class="reading">
		<span class="reading-name">
			<span class="name">{label}</span>
			{#if sub}<span class="sub-line">{sub}</span>{/if}
		</span>
		<svg class="spark" viewBox="0 0 {W} {H}" preserveAspectRatio="none" aria-hidden="true">
			<path class="area" d={c.area} />
			<path class="line" d={c.line} vector-effect="non-scaling-stroke" />
		</svg>
		<span class="now">{now}</span>
	</div>
{/snippet}

<SettingsPage title="Your server" lede="Whether your server is all right, what it runs, and the machine itself.">
	{#if !loaded}
		<p class="quiet">Asking your server how it’s doing…</p>
	{:else}
		<p class="settings-lead">{lead.sentence}</p>
		<p class="settings-sub">
			{lead.sub}
			{#if lead.guide}
				The manual’s <a href={lead.guide.href} target="_blank" rel="noopener noreferrer">{lead.guide.label}</a> page covers what to check.
			{/if}
		</p>
		{#if lead.action}
			{@const action = lead.action}
			<div class="lead-action">
				<Button variant="secondary" onclick={() => openSettingsPage(action.route, DEFAULT_ASSISTANT_NAME)}>{action.label}</Button>
			</div>
		{/if}

		<section class="group">
			<h2 class="settings-label">Storage and backup</h2>
			{#if disk}
				<div class="disk">
					<span class="name">Disk</span>
					<div>
						<div class="bar" role="img" aria-label="{diskPct}% of the disk is in use">
							<div class="bar-fill" class:full={diskPct >= DISK_FULL_PCT} style:width="{diskPct}%"></div>
						</div>
						<p class="help">{formatBytes(disk.available)} free of {formatBytes(disk.total)}.</p>
					</div>
				</div>
			{/if}
			<LinkRow
				id="backup"
				label="Backup"
				help="Encrypted, every night, to the drives you plug into your server."
				href="/virtues/backup"
				value={backup ? backupSummary(backup) : undefined}
			/>
		</section>

		<section class="group">
			<h2 class="settings-label">What it runs</h2>
			<LinkRow id="updates" label="Updates" help="New releases, and which ones your server takes." href="/virtues/updates" value={updatesValue(update)} />
			<LinkRow
				id="search-runs-on"
				label="Search"
				help="Where search runs, and how much of your record it covers."
				href="/virtues/search"
				value={searchValue(search)}
			/>
			<LinkRow
				id="applets"
				label="Applets"
				help="What runs on its own, and how its last runs went."
				href="/virtues/applets"
				value={appletsValue(appletsNeedingYou)}
			/>
		</section>

		<section class="group">
			<h2 class="settings-label">Last 24 hours</h2>
			{#if history.length < 2}
				<p class="quiet">Collecting samples. The charts appear once your server has run a few minutes.</p>
			{:else}
				{@render reading('Processor', t?.cpu ? `${t.cpu.logical_cores} cores` : '', cpuSeries, 0, 100, cpuNow)}
				{@render reading('Memory', memPct != null ? `${memPct}% in use` : '', memSeries, 0, 100, memNow)}
				{#if hasGpu}
					{@render reading(
						'Graphics',
						t?.gpu ? (t.gpu.offload_active ? 'Search runs on it' : 'Search runs on the processor instead') : '',
						gpuSeries,
						0,
						100,
						gpuNow,
					)}
				{/if}
				{#if hasTemp}
					{@render reading('Temperature', 'The warmest sensor', tempSeries, tempRange.lo, tempRange.hi, hottest != null ? temp(hottest) : '')}
				{/if}
				<div class="axis" aria-hidden="true">
					<span></span>
					<span class="axis-times">
						{#each axis as mark (mark.label)}
							<span class="mark" class:first={mark.pct === 0} class:last={mark.pct === 100} style:left="{mark.pct}%">{mark.label}</span>
						{/each}
					</span>
					<span></span>
				</div>
			{/if}
		</section>

		{#if t?.host}
			<section class="group">
				<h2 class="settings-label">The machine</h2>
				{#if t.host.hostname}{@render fact('Name', t.host.hostname)}{/if}
				{#if t.host.os}{@render fact('System', t.host.os)}{/if}
				{#if t.cpu?.brand}{@render fact('Processor', t.cpu.brand)}{/if}
				{@render fact('Up for', upFor(t.host.uptime_secs))}
			</section>
		{/if}

		<div class="trouble">
			<Fold
				label="For troubleshooting"
				sub="Network traffic now, each processor core and temperature sensor, the processes using the most memory, the database pool, and the raw readings."
			>
				<div use:whileOpen>
					{#if t?.network}
						<h3>Network now</h3>
						{@render fact('Coming in', rate(t.network.rx_per_sec))}
						{@render fact('Going out', rate(t.network.tx_per_sec))}
						{#each busiest as iface (iface.name)}
							{@render fact(iface.name, `${formatBytes(iface.rx_total)} in, ${formatBytes(iface.tx_total)} out since it started`)}
						{/each}
					{/if}

					{#if t?.cpu}
						<h3>Processor</h3>
						{@render fact('Load', `${t.cpu.load_avg.one.toFixed(2)}, ${t.cpu.load_avg.five.toFixed(2)}, ${t.cpu.load_avg.fifteen.toFixed(2)}`)}
						{#if clock(t.cpu)}{@render fact('Clock', clock(t.cpu))}{/if}
						{#if t.cpu.per_core?.length}
							<div class="cores" aria-label="Each core’s use">
								{#each t.cpu.per_core as c, i (i)}
									<div class="core" title={`Core ${i + 1}: ${Math.round(c)}%`}>
										<div class="core-fill {pressure(c)}" style:height="{Math.max(c, 2)}%"></div>
									</div>
								{/each}
							</div>
						{/if}
					{/if}

					{#if t?.thermal?.length || t?.gpu}
						<h3>Temperatures</h3>
						{#each t?.thermal ?? [] as s (s.label)}
							{@render fact(s.label, temp(s.temp_c))}
						{/each}
						{#if t?.gpu?.temp_c != null}{@render fact('Graphics', temp(t.gpu.temp_c))}{/if}
						{#if t?.gpu?.power_mw != null}{@render fact('Graphics power', `${(t.gpu.power_mw / 1000).toFixed(2)} W`)}{/if}
						{#if t?.gpu?.mem_total}{@render fact('Graphics memory', `${formatBytes(t.gpu.mem_used ?? 0)} of ${formatBytes(t.gpu.mem_total)}`)}{/if}
					{/if}

					<h3>Service</h3>
					{@render fact('Status', health?.status ?? 'No answer', health?.status === 'healthy' ? 'ok' : 'crit')}
					{@render fact('Database', health?.database ?? 'No answer', health?.database === 'connected' ? 'ok' : 'crit')}
					{#if t?.pool}{@render fact('Database pool', `${t.pool.idle} idle of ${t.pool.size}`)}{/if}
					{#each t?.services ?? [] as s (s.name)}
						{@render fact(`The ${s.name} service`, s.up ? 'Running' : 'Stopped', s.up ? 'ok' : 'crit')}
					{/each}
					{#if t?.host?.kernel}{@render fact('Kernel', t.host.kernel)}{/if}

					{#if t?.inference}
						<h3>Search models</h3>
						{@render fact('Accelerator', t.inference.accelerator)}
						{@render fact('Precision', t.inference.precision)}
						{#each t.inference.models as m (m.name)}
							{@render fact(m.name, m.source === 'baked' ? 'Downloaded' : 'Missing', m.source === 'baked' ? 'ok' : 'warn')}
						{/each}
					{/if}

					{#if t?.processes?.length}
						<h3>Processes using the most memory</h3>
						<table class="procs">
							<thead>
								<tr><th>Process</th><th class="num">ID</th><th class="num">Processor</th><th class="num">Memory</th></tr>
							</thead>
							<tbody>
								{#each t.processes as p (p.pid)}
									<tr>
										<td class="pname">{p.name}</td>
										<td class="num">{p.pid}</td>
										<td class="num {pressure(p.cpu_pct)}">{p.cpu_pct.toFixed(1)}%</td>
										<td class="num">{formatBytes(p.mem)}</td>
									</tr>
								{/each}
							</tbody>
						</table>
					{/if}

					<div class="raw-head">
						<Button variant="secondary" onclick={() => (rawOpen = !rawOpen)}>{rawOpen ? 'Hide the raw readings' : 'Show the raw readings'}</Button>
					</div>
					{#if rawOpen}
						<pre class="raw">{JSON.stringify(t, null, 2)}</pre>
					{/if}
				</div>
			</Fold>
		</div>

		<section class="group">
			<h2 class="settings-label">For developers</h2>
			<LinkRow
				id="terminal"
				label="Terminal"
				help="A shell on your server. Every Terminal tab shows the same session."
				href="/virtues/developer/terminal"
				tabLabel="Terminal"
			/>
			<LinkRow
				id="query"
				label="Query"
				help="Read any table with SQL, and delete rows from your own records."
				href="/sources/query"
				tabLabel="Sources · Query"
			/>
			<LinkRow
				id="streams"
				label="Streams"
				help="Everything your devices and accounts sent, exactly as it arrived."
				href="/sources/streams"
				tabLabel="Sources · Streams"
			/>
		</section>
	{/if}
</SettingsPage>

<style>
	.quiet {
		margin: 24px 0 0;
		font-size: 14px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.lead-action {
		margin-top: 16px;
	}

	.group {
		padding: 24px 0 8px;
	}

	.name {
		display: block;
		font-size: 15px;
		color: var(--color-foreground);
	}

	.help,
	.sub-line {
		display: block;
		margin: 4px 0 0;
		font-size: 12px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	/* ─── Disk ─────────────────────────────────────────────────────────── */

	.disk {
		display: grid;
		grid-template-columns: 180px minmax(0, 1fr);
		align-items: start;
		gap: 16px;
		min-height: 56px;
		padding-top: 8px;
	}

	.bar {
		height: 6px;
		margin-top: 8px;
		border-radius: var(--radius-xs);
		background: var(--color-surface-elevated, var(--color-border));
		overflow: hidden;
	}

	.bar-fill {
		height: 100%;
		background: var(--color-foreground-muted);
	}

	.bar-fill.full {
		background: var(--color-warning);
	}

	/* ─── Last 24 hours ────────────────────────────────────────────────── */

	.reading,
	.axis {
		display: grid;
		grid-template-columns: 180px minmax(0, 1fr) 132px;
		align-items: center;
		gap: 16px;
	}

	.reading {
		min-height: 60px;
	}

	.spark {
		display: block;
		width: 100%;
		height: 36px;
	}

	.spark .area {
		fill: var(--color-surface-elevated, var(--color-border));
		stroke: none;
	}

	.spark .line {
		fill: none;
		stroke: var(--color-foreground-muted);
		stroke-width: 1.25;
	}

	.now {
		white-space: nowrap;
		text-align: right;
		font-size: 15px;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground);
	}

	.axis {
		margin-top: 4px;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	.axis-times {
		position: relative;
		height: 16px;
	}

	.mark {
		position: absolute;
		top: 0;
		transform: translateX(-50%);
		white-space: nowrap;
	}

	.mark.first {
		transform: none;
	}

	.mark.last {
		transform: translateX(-100%);
	}

	/* ─── Facts ────────────────────────────────────────────────────────── */

	.fact {
		display: grid;
		grid-template-columns: 180px minmax(0, 1fr);
		gap: 16px;
		padding: 8px 0;
		font-size: 15px;
		line-height: 1.4;
	}

	.fact-label {
		color: var(--color-foreground);
		overflow-wrap: anywhere;
	}

	.fact-value {
		color: var(--color-foreground-muted);
		overflow-wrap: anywhere;
		font-variant-numeric: tabular-nums;
	}

	.fact-value.ok {
		color: var(--color-success);
	}

	.fact-value.warn {
		color: var(--color-warning);
	}

	.fact-value.crit {
		color: var(--color-error);
	}

	/* ─── Troubleshooting ──────────────────────────────────────────────── */

	.trouble {
		margin-top: 24px;
	}

	h3 {
		margin: 24px 0 4px;
		font-family: inherit;
		font-size: 13px;
		font-weight: 400;
		color: var(--color-foreground-muted);
	}

	h3:first-child {
		margin-top: 8px;
	}

	.cores {
		display: flex;
		align-items: flex-end;
		gap: 4px;
		height: 36px;
		margin-top: 8px;
	}

	.core {
		flex: 1;
		min-width: 4px;
		height: 100%;
		display: flex;
		align-items: flex-end;
		border-radius: var(--radius-xs);
		background: var(--color-border);
		overflow: hidden;
	}

	.core-fill {
		width: 100%;
		background: var(--color-foreground-muted);
	}

	.core-fill.warn {
		background: var(--color-warning);
	}

	.core-fill.crit {
		background: var(--color-error);
	}

	.procs {
		width: 100%;
		margin-top: 8px;
		border-collapse: collapse;
		font-size: 13px;
	}

	.procs th {
		padding: 4px 8px 4px 0;
		border-bottom: 1px solid var(--color-border);
		font-weight: 400;
		font-size: 12px;
		text-align: left;
		color: var(--color-foreground-muted);
	}

	.procs td {
		padding: 4px 8px 4px 0;
		border-bottom: 1px solid var(--color-border);
		color: var(--color-foreground-muted);
	}

	.procs .num {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}

	.procs .pname {
		max-width: 240px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		color: var(--color-foreground);
	}

	.procs .warn {
		color: var(--color-warning);
	}

	.procs .crit {
		color: var(--color-error);
	}

	.raw-head {
		margin-top: 24px;
	}

	.raw {
		margin: 12px 0 0;
		padding: 12px;
		max-height: 360px;
		overflow: auto;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		font-size: 12px;
		line-height: 1.5;
		white-space: pre;
		color: var(--color-foreground-muted);
	}

	@media (max-width: 560px) {
		.disk,
		.fact {
			grid-template-columns: minmax(0, 1fr);
			gap: 4px;
		}

		.reading,
		.axis {
			grid-template-columns: minmax(0, 1fr) auto;
			row-gap: 4px;
		}

		.reading {
			padding-bottom: 12px;
		}

		.reading .spark,
		.axis-times {
			grid-column: 1 / -1;
			grid-row: 2;
		}

		.axis > span:empty {
			display: none;
		}
	}
</style>
