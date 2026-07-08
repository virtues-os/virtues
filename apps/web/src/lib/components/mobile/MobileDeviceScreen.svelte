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
	import { onMount } from "svelte";

	interface ProbeRow {
		ts: string;
		lat: number;
		lon: number;
		source: string;
		appState: string;
		launchReason: string;
	}

	let rows = $state<ProbeRow[]>([]);
	let loading = $state(true);
	let starting = $state(false);
	let error = $state<string | null>(null);

	const enabled = $derived(rows.length > 0);
	const lastTs = $derived(rows[0]?.ts ?? null);

	async function load() {
		if (!mobileLayout.isNativeShell) {
			loading = false;
			return;
		}
		loading = true;
		error = null;
		try {
			const resp = await invoke<{ rows: ProbeRow[] }>("plugin:location-probe|read_rows", {
				payload: { limit: 30 },
			});
			// Newest first.
			rows = (resp.rows ?? []).slice().reverse();
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
		<div class="stream muted">
			<div class="s-icon"><Icon icon="ri:heart-pulse-line" width={18} /></div>
			<div class="s-body">
				<div class="s-title">Health</div>
				<div class="s-sub">Coming soon</div>
			</div>
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
			{#each rows as r, i (i)}
				<div class="log">
					<Icon icon="ri:pulse-line" width={15} />
					<div class="l-body">
						<div class="l-top">
							<span class="l-time">{rel(r.ts)}</span>
							<span class="l-badge" class:bg={isBackground(r)}>{r.appState}</span>
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

	<p class="foot">
		Recorded locally on this phone. The shared upload queue that syncs these to
		your box lands in the next update.
	</p>
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
	.l-sub {
		font-size: 12px;
		font-variant-numeric: tabular-nums;
		margin-top: 1px;
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
	.foot {
		font-size: 11px;
		color: var(--color-foreground-muted);
		line-height: 1.5;
		margin: 16px 4px 0;
	}
</style>
