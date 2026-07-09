<script lang="ts">
	/**
	 * "This device" — the native collector dashboard (iOS/Android shell only).
	 *
	 * A stripped-down descendant of the old native app: this phone as a data
	 * collector. Shows the real state the plugins already expose — location
	 * events recorded (incl. background/cold-relaunch rows), with a toggle to
	 * start the collector — plus a live recent-activity log.
	 *
	 * Reads through the location-probe plugin (`read_rows` / `start_probe`).
	 * Storage size, health, and the shared upload queue land in the next pass.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { getVersion } from "@tauri-apps/api/app";
	import { onMount } from "svelte";

	interface ProbeRow {
		ts: string;
		lat: number;
		lon: number;
		source: string;
		appState: string;
		launchReason: string;
	}

	interface ReachStatus {
		paired: boolean;
		session: string; // authed | rejected | unknown | unpaired
		loopbackUrl: string;
	}

	interface OutboxStats {
		queued: number;
		failing: number;
		oldest: number; // unix seconds, 0 if empty
	}

	interface HealthStatus {
		authorized: boolean;
		collecting: boolean;
	}

	/** A collapsed run of consecutive near-identical fixes. */
	interface LogRun {
		ts: string;
		lat: number;
		lon: number;
		appState: string;
		launchReason: string;
		count: number;
	}

	let rows = $state<ProbeRow[]>([]);
	let reach = $state<ReachStatus | null>(null);
	let sync = $state<OutboxStats | null>(null);
	let health = $state<HealthStatus | null>(null);
	let healthSync = $state<OutboxStats | null>(null);
	let version = $state<string>("");
	let loading = $state(true);
	let starting = $state(false);
	let enablingHealth = $state(false);
	let error = $state<string | null>(null);

	const enabled = $derived(rows.length > 0);
	const lastTs = $derived(rows[0]?.ts ?? null);

	// Connection verdict from reach status.
	const conn = $derived.by(() => {
		if (!reach) return { label: "Checking…", tone: "idle" };
		if (!reach.paired) return { label: "Not paired", tone: "off" };
		if (reach.session === "authed") return { label: "Connected to your box", tone: "on" };
		if (reach.session === "rejected") return { label: "Access rejected — re-pair", tone: "off" };
		return { label: "Reconnecting…", tone: "idle" };
	});

	// Collapse consecutive fixes at the same rounded coord + state into one run,
	// so a stationary phone shows "7 fixes" not 30 identical lines.
	const runs = $derived.by<LogRun[]>(() => {
		const out: LogRun[] = [];
		for (const r of rows) {
			const last = out[out.length - 1];
			const sameSpot =
				last &&
				last.appState === r.appState &&
				Math.abs(last.lat - r.lat) < 0.0005 &&
				Math.abs(last.lon - r.lon) < 0.0005;
			if (sameSpot) {
				last.count++;
			} else {
				out.push({
					ts: r.ts,
					lat: r.lat,
					lon: r.lon,
					appState: r.appState,
					launchReason: r.launchReason,
					count: 1,
				});
			}
		}
		return out;
	});

	async function load() {
		if (!mobileLayout.isNativeShell) {
			loading = false;
			return;
		}
		loading = true;
		error = null;
		try {
			const [rowsResp, reachResp, syncResp, healthResp, healthSyncResp, ver] = await Promise.all([
				invoke<{ rows: ProbeRow[] }>("plugin:location-probe|read_rows", {
					payload: { limit: 50 },
				}),
				invoke<ReachStatus>("plugin:reach|reach_status").catch(() => null),
				invoke<OutboxStats>("plugin:reach|outbox_stats", { stream: "location" }).catch(() => null),
				invoke<HealthStatus>("plugin:health|status").catch(() => null),
				invoke<OutboxStats>("plugin:reach|outbox_stats", { stream: "healthkit" }).catch(() => null),
				getVersion().catch(() => ""),
			]);
			rows = (rowsResp.rows ?? []).slice().reverse(); // newest first
			reach = reachResp;
			sync = syncResp;
			health = healthResp;
			healthSync = healthSyncResp;
			version = ver;
		} catch (e) {
			error = String(e);
		} finally {
			loading = false;
		}
	}

	async function enableLocation() {
		starting = true;
		error = null;
		try {
			await invoke("plugin:location-probe|start_probe");
			// Give the first fix a beat, then refresh.
			setTimeout(load, 800);
		} catch (e) {
			error = String(e);
		} finally {
			starting = false;
		}
	}

	async function enableHealth() {
		enablingHealth = true;
		error = null;
		try {
			health = await invoke<HealthStatus>("plugin:health|enable");
			// Backfill takes a moment to start enqueuing; refresh shortly after.
			setTimeout(load, 1500);
		} catch (e) {
			error = String(e);
		} finally {
			enablingHealth = false;
		}
	}

	let syncingNow = $state(false);
	async function syncNow() {
		syncingNow = true;
		error = null;
		try {
			// Grab the latest health samples, then drain everything to the box.
			if (health?.authorized) await invoke("plugin:health|collect").catch(() => {});
			await invoke("plugin:reach|drain_now");
			await load();
		} catch (e) {
			error = String(e);
		} finally {
			syncingNow = false;
		}
	}

	function rel(ts: string): string {
		const t = new Date(ts).getTime();
		if (Number.isNaN(t)) return ts;
		const s = Math.round((Date.now() - t) / 1000);
		if (s < 60) return `${s}s ago`;
		const m = Math.round(s / 60);
		if (m < 60) return `${m}m ago`;
		const h = Math.round(m / 60);
		if (h < 24) return `${h}h ago`;
		return `${Math.round(h / 24)}d ago`;
	}

	function isBackground(r: ProbeRow): boolean {
		return r.appState !== "active";
	}

	onMount(load);
</script>

<div class="device">
	<div class="group-label">Connection</div>
	<div class="card">
		<div class="stream">
			<div class="s-icon" class:on={conn.tone === "on"}>
				<Icon icon="ri:links-line" width={18} />
			</div>
			<div class="s-body">
				<div class="s-title">{conn.label}</div>
				<div class="s-sub">
					{#if reach?.paired}This phone is paired{:else}Pair this phone to your box to sync{/if}
				</div>
			</div>
			<span class="dot" class:on={conn.tone === "on"} class:off={conn.tone === "off"}></span>
		</div>
	</div>

	<div class="group-label">Streams</div>
	<div class="card">
		<div class="stream">
			<div class="s-icon" class:on={enabled}><Icon icon="ri:map-pin-line" width={18} /></div>
			<div class="s-body">
				<div class="s-title">Location</div>
				<div class="s-sub">
					{#if loading}Checking…{:else if enabled}On · {rows.length} recent
						{#if lastTs}· {rel(lastTs)}{/if}{:else}Off{/if}
				</div>
			</div>
			{#if !enabled}
				<button class="s-action" onclick={enableLocation} disabled={starting}>
					{starting ? "Enabling…" : "Enable"}
				</button>
			{:else}
				<span class="dot on"></span>
			{/if}
		</div>
		<div class="stream">
			<div class="s-icon" class:on={health?.authorized}>
				<Icon icon="ri:heart-pulse-line" width={18} />
			</div>
			<div class="s-body">
				<div class="s-title">Health</div>
				<div class="s-sub">
					{#if health?.authorized}
						On{#if healthSync && healthSync.queued > 0} · {healthSync.queued} syncing{:else} · synced{/if}
					{:else}Heart rate, steps, sleep &amp; more{/if}
				</div>
			</div>
			{#if !health?.authorized}
				<button class="s-action" onclick={enableHealth} disabled={enablingHealth}>
					{enablingHealth ? "Enabling…" : "Enable"}
				</button>
			{:else}
				<span class="dot on"></span>
			{/if}
		</div>
	</div>

	<div class="group-label">Sync</div>
	<div class="card">
		<div class="stream">
			<div class="s-icon" class:on={sync != null && sync.queued === 0}>
				<Icon icon="ri:refresh-line" width={18} />
			</div>
			<div class="s-body">
				<div class="s-title">
					{#if !sync}—{:else if sync.queued === 0}Synced to your box{:else}{sync.queued} waiting to sync{/if}
				</div>
				<div class="s-sub">
					{#if sync && sync.failing > 0}{sync.failing} retrying{:else}Uploaded over your private link{/if}
				</div>
			</div>
			<button class="s-action" onclick={syncNow} disabled={syncingNow}>
				{syncingNow ? "Syncing…" : "Sync now"}
			</button>
		</div>
	</div>

	<div class="group-label">
		<span>Recent activity</span>
		<button class="refresh" onclick={load} aria-label="Refresh">
			<Icon icon="ri:refresh-line" width={15} />
		</button>
	</div>
	<div class="card">
		{#if loading}
			<div class="empty">Loading…</div>
		{:else if error}
			<div class="empty err">{error}</div>
		{:else if rows.length === 0}
			<div class="empty">
				No location events recorded yet. Enable Location above — events (including
				background captures) will appear here.
			</div>
		{:else}
			{#each runs as r, i (i)}
				<div class="log">
					<Icon icon="ri:pulse-line" width={15} />
					<div class="l-body">
						<div class="l-top">
							<span class="l-time">{rel(r.ts)}</span>
							<span class="l-badge" class:bg={r.appState !== "active"}>{r.appState}</span>
							{#if r.count > 1}<span class="l-count">×{r.count}</span>{/if}
						</div>
						<div class="l-sub">
							{r.lat.toFixed(4)}, {r.lon.toFixed(4)}
							{#if r.launchReason && r.launchReason !== "none"}· {r.launchReason}{/if}
						</div>
					</div>
				</div>
			{/each}
		{/if}
	</div>

	<div class="group-label">About</div>
	<div class="card">
		<div class="about">
			<span>App version</span><span class="v">{version || "—"}</span>
		</div>
		<div class="about">
			<span>Recorded points</span><span class="v">{rows.length}</span>
		</div>
	</div>

</div>

<style>
	.device {
		padding-bottom: 8px;
	}
	.group-label {
		display: flex;
		align-items: center;
		justify-content: space-between;
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--color-foreground-muted);
		margin: 18px 4px 8px;
	}
	.refresh {
		display: flex;
		border: 0;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
	}
	.card {
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		overflow: hidden;
	}

	.stream {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 14px;
		border-bottom: 1px solid var(--color-border);
	}
	.stream:last-child {
		border-bottom: 0;
	}
	.stream.muted {
		opacity: 0.55;
	}
	.s-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		border-radius: 8px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
		color: var(--color-foreground-muted);
	}
	.s-icon.on {
		background: color-mix(in srgb, var(--color-primary, #2b6cff) 16%, transparent);
		color: var(--color-primary, #2b6cff);
	}
	.s-body {
		flex: 1;
	}
	.s-title {
		font-size: 15px;
		font-weight: 550;
	}
	.s-sub {
		font-size: 12px;
		color: var(--color-foreground-muted);
		margin-top: 1px;
	}
	.s-action {
		border: 1px solid var(--color-primary, #2b6cff);
		color: var(--color-primary, #2b6cff);
		background: transparent;
		border-radius: 8px;
		padding: 7px 14px;
		font-size: 13px;
		font-weight: 600;
		cursor: pointer;
	}
	.s-action:disabled {
		opacity: 0.5;
	}
	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--color-foreground-muted);
	}
	.dot.on {
		background: #34c759;
	}

	.log {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 10px 14px;
		border-bottom: 1px solid var(--color-border);
		color: var(--color-foreground-muted);
	}
	.log:last-child {
		border-bottom: 0;
	}
	.l-body {
		flex: 1;
	}
	.l-top {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.l-time {
		font-size: 13px;
		color: var(--color-foreground);
		font-weight: 500;
	}
	.l-badge {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		padding: 1px 6px;
		border-radius: 5px;
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground-muted);
	}
	.l-badge.bg {
		background: color-mix(in srgb, #34c759 20%, transparent);
		color: #248a3d;
	}
	.l-count {
		font-size: 11px;
		font-weight: 600;
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}
	.l-sub {
		font-size: 12px;
		font-variant-numeric: tabular-nums;
		margin-top: 1px;
	}

	.about {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 14px;
		border-bottom: 1px solid var(--color-border);
		font-size: 14px;
	}
	.about:last-child {
		border-bottom: 0;
	}
	.about .v {
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
	}

	.empty {
		padding: 18px 14px;
		font-size: 13px;
		color: var(--color-foreground-muted);
		line-height: 1.4;
	}
	.empty.err {
		color: #e5484d;
		font-variant-numeric: tabular-nums;
	}
</style>
