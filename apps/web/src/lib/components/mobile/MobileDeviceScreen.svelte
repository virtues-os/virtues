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
	import MobileKeyboardProbe from "$lib/components/mobile/MobileKeyboardProbe.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { confirmAction } from "$lib/stores/dialog.svelte";
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
		reachable: boolean; // live: box actually answered a probe just now
		path: string; // live: direct | relay | offline
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
	// Same shape for the other opt-in collectors.
	type StreamStatus = HealthStatus;

	interface AudioStatus {
		authorized: boolean;
		recording: boolean;
		notify: boolean;
		/** Chunks shipped metadata-only because they measured silent. */
		silentDropped?: number;
		/** Quiet-hours window, minutes since local midnight; -1 or absent = off.
		 * Mute-don't-release: the mic stays armed, chunks stop being written. */
		quietStart?: number;
		quietEnd?: number;
	}

	/** Radio-hygiene counters — the battery A/B harness (reach plugin). */
	interface RadioStats {
		drains: number;
		dials: number;
		records: number;
		bytes: number;
		parks: number;
		last_drain_at: number | null;
		/** No warm endpoint right now — the radio is free to idle. */
		parked: boolean;
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
	let cal = $state<StreamStatus | null>(null);
	let calSync = $state<OutboxStats | null>(null);
	let contacts = $state<StreamStatus | null>(null);
	let contactsSync = $state<OutboxStats | null>(null);
	let finance = $state<StreamStatus | null>(null);
	let financeSync = $state<OutboxStats | null>(null);
	let audio = $state<AudioStatus | null>(null);
	let audioSync = $state<OutboxStats | null>(null);
	let radio = $state<RadioStats | null>(null);
	let togglingAudio = $state(false);
	let forgetting = $state(false);
	let probeOpen = $state(false);
	let version = $state<string>("");
	let loading = $state(true);
	let starting = $state(false);
	let enablingHealth = $state(false);
	let enablingCal = $state(false);
	let enablingContacts = $state(false);
	let enablingFinance = $state(false);
	let error = $state<string | null>(null);

	const enabled = $derived(rows.length > 0);
	const lastTs = $derived(rows[0]?.ts ?? null);

	// Connection verdict from LIVE reach status (probe + iroh path), not just the
	// stored "paired" flag — so it can't claim "connected" when the box is
	// actually unreachable.
	const conn = $derived.by(() => {
		if (!reach) return { label: "Checking…", sub: "", tone: "idle" };
		if (!reach.paired)
			return { label: "Not paired", sub: "Pair this phone to your server to sync", tone: "off" };
		if (reach.session === "rejected")
			return { label: "Access rejected", sub: "Re-pair this phone", tone: "off" };
		if (!reach.reachable)
			return { label: "Can’t reach your server", sub: "Paired, but offline right now", tone: "off" };
		// Reachable — show HOW we're connected.
		const via =
			reach.path === "direct"
				? "Direct · on your network"
				: reach.path === "relay"
					? "Via relay"
					: "Connected";
		return { label: "Connected to your server", sub: via, tone: "on" };
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
			const [
				rowsResp,
				reachResp,
				syncResp,
				healthResp,
				healthSyncResp,
				calResp,
				calSyncResp,
				contactsResp,
				contactsSyncResp,
				financeResp,
				financeSyncResp,
				audioResp,
				audioSyncResp,
				radioResp,
				ver,
			] = await Promise.all([
					invoke<{ rows: ProbeRow[] }>("plugin:location-probe|read_rows", {
						payload: { limit: 50 },
					}),
					invoke<ReachStatus>("plugin:reach|reach_status").catch(() => null),
					invoke<OutboxStats>("plugin:reach|outbox_stats", { stream: "location" }).catch(() => null),
					invoke<HealthStatus>("plugin:health|status").catch(() => null),
					invoke<OutboxStats>("plugin:reach|outbox_stats", { stream: "healthkit" }).catch(() => null),
					invoke<StreamStatus>("plugin:eventkit|status").catch(() => null),
					invoke<OutboxStats>("plugin:reach|outbox_stats", { stream: "eventkit" }).catch(() => null),
					invoke<StreamStatus>("plugin:contacts|status").catch(() => null),
					invoke<OutboxStats>("plugin:reach|outbox_stats", { stream: "contacts" }).catch(() => null),
					invoke<StreamStatus>("plugin:finance|status").catch(() => null),
					invoke<OutboxStats>("plugin:reach|outbox_stats", { stream: "financekit" }).catch(() => null),
					invoke<AudioStatus>("plugin:audio|status").catch(() => null),
					invoke<OutboxStats>("plugin:reach|outbox_stats", { stream: "microphone" }).catch(() => null),
					invoke<RadioStats>("plugin:reach|radio_stats").catch(() => null),
					getVersion().catch(() => ""),
				]);
			rows = (rowsResp.rows ?? []).slice().reverse(); // newest first
			reach = reachResp;
			sync = syncResp;
			health = healthResp;
			healthSync = healthSyncResp;
			cal = calResp;
			calSync = calSyncResp;
			contacts = contactsResp;
			contactsSync = contactsSyncResp;
			finance = financeResp;
			financeSync = financeSyncResp;
			audio = audioResp;
			audioSync = audioSyncResp;
			radio = radioResp;
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

	async function enableCalendar() {
		enablingCal = true;
		error = null;
		try {
			cal = await invoke<StreamStatus>("plugin:eventkit|enable");
			setTimeout(load, 1500);
		} catch (e) {
			error = String(e);
		} finally {
			enablingCal = false;
		}
	}

	async function enableContacts() {
		enablingContacts = true;
		error = null;
		try {
			contacts = await invoke<StreamStatus>("plugin:contacts|enable");
			setTimeout(load, 1500);
		} catch (e) {
			error = String(e);
		} finally {
			enablingContacts = false;
		}
	}

	async function enableFinance() {
		enablingFinance = true;
		error = null;
		try {
			finance = await invoke<StreamStatus>("plugin:finance|enable");
			setTimeout(load, 2000);
		} catch (e) {
			error = String(e);
		} finally {
			enablingFinance = false;
		}
	}

	/// Audio is toggleable (its toggle doubles as the pause control): Enable
	/// prompts + starts; once authorized the button stops/resumes recording.
	async function toggleAudio() {
		togglingAudio = true;
		error = null;
		try {
			if (audio?.recording) {
				audio = await invoke<AudioStatus>("plugin:audio|disable");
			} else {
				audio = await invoke<AudioStatus>("plugin:audio|enable");
			}
			setTimeout(load, 2000);
		} catch (e) {
			error = String(e);
		} finally {
			togglingAudio = false;
		}
	}

	/// Toggle the "notify me if recording stops" gap-nudge (default on).
	async function toggleAudioNotify() {
		if (!audio) return;
		try {
			audio = await invoke<AudioStatus>("plugin:audio|set_notify", {
				enabled: !audio.notify,
			});
		} catch (e) {
			error = String(e);
		}
	}

	// Quiet hours (mute-don't-release). Window is minutes since local midnight.
	const quietOn = $derived(
		audio != null && (audio.quietStart ?? -1) >= 0 && (audio.quietEnd ?? -1) >= 0,
	);
	function minToTime(m: number): string {
		const h = Math.floor(m / 60) % 24;
		return `${String(h).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
	}
	function timeToMin(t: string): number {
		const [h, m] = t.split(":").map(Number);
		return (h || 0) * 60 + (m || 0);
	}
	async function setQuietHours(start: number, end: number) {
		try {
			audio = await invoke<AudioStatus>("plugin:audio|set_quiet_hours", { start, end });
		} catch (e) {
			error = String(e);
		}
	}
	/// Toggle: default window 22:00 → 07:00 on first enable.
	function toggleQuietHours() {
		if (quietOn) void setQuietHours(-1, -1);
		else void setQuietHours(22 * 60, 7 * 60);
	}

	/// Unpair this device: clear the Keychain-stored pairing (seed + box info), so
	/// the app forgets the box entirely. Since the pairing survives app deletion
	/// (Keychain), this is the only way to fully reset — useful for switching boxes
	/// or a clean re-pair. Reloads into the pairing flow afterward.
	async function unpairDevice() {
		const ok = await confirmAction({
			title: "Unpair this device?",
			body: "This clears the saved connection to your box. You'll need to pair again to reconnect. Your data on the box is untouched.",
			confirmLabel: "Unpair",
			danger: true,
		});
		if (!ok) return;
		forgetting = true;
		error = null;
		try {
			await invoke("plugin:reach|forget");
			// Drop the just-paired marker too — it exists to bridge the launch
			// that paired, and surviving an unpair would resurrect the pairing
			// in the eyes of mobileLayout.
			try {
				localStorage.removeItem("virtues-just-paired");
			} catch {
				/* best effort */
			}
			// Go to the connect shell, don't reload. `reload()` re-requests the
			// URL we are already on — the SPA root — so the app came back up
			// unpaired, with no way to pair, and the only escape was force
			// quitting. The shell confirms pairing with the plugin before it
			// redirects, so landing here after a forget stays here.
			// (connect.html absorbed mobile-pair.html on 2026-08-11.)
			window.location.replace("/connect.html");
		} catch (e) {
			error = String(e);
			forgetting = false;
		}
	}

	let syncingNow = $state(false);
	async function syncNow() {
		syncingNow = true;
		error = null;
		try {
			// Grab the latest samples from each collector, then drain to the box.
			if (health?.authorized) await invoke("plugin:health|collect").catch(() => {});
			if (cal?.authorized) await invoke("plugin:eventkit|collect").catch(() => {});
			if (contacts?.authorized) await invoke("plugin:contacts|collect").catch(() => {});
			if (finance?.authorized) await invoke("plugin:finance|collect").catch(() => {});
			await invoke("plugin:reach|drain_now");
			await load();
		} catch (e) {
			error = String(e);
		} finally {
			syncingNow = false;
		}
	}

	function fmtBytes(n: number): string {
		if (n >= 1024 * 1024 * 1024) return `${(n / (1024 * 1024 * 1024)).toFixed(1)} GB`;
		if (n >= 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
		if (n >= 1024) return `${(n / 1024).toFixed(0)} KB`;
		return `${n} B`;
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
					{conn.sub}
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
		<div class="stream">
			<div class="s-icon" class:on={cal?.authorized}>
				<Icon icon="ri:calendar-line" width={18} />
			</div>
			<div class="s-body">
				<div class="s-title">Calendar</div>
				<div class="s-sub">
					{#if cal?.authorized}
						On{#if calSync && calSync.queued > 0} · {calSync.queued} syncing{:else} · synced{/if}
					{:else}Events, past &amp; upcoming{/if}
				</div>
			</div>
			{#if !cal?.authorized}
				<button class="s-action" onclick={enableCalendar} disabled={enablingCal}>
					{enablingCal ? "Enabling…" : "Enable"}
				</button>
			{:else}
				<span class="dot on"></span>
			{/if}
		</div>
		<div class="stream">
			<div class="s-icon" class:on={contacts?.authorized}>
				<Icon icon="ri:contacts-book-line" width={18} />
			</div>
			<div class="s-body">
				<div class="s-title">Contacts</div>
				<div class="s-sub">
					{#if contacts?.authorized}
						On{#if contactsSync && contactsSync.queued > 0} · {contactsSync.queued} syncing{:else} · synced{/if}
					{:else}The people in your life{/if}
				</div>
			</div>
			{#if !contacts?.authorized}
				<button class="s-action" onclick={enableContacts} disabled={enablingContacts}>
					{enablingContacts ? "Enabling…" : "Enable"}
				</button>
			{:else}
				<span class="dot on"></span>
			{/if}
		</div>
		<div class="stream">
			<div class="s-icon" class:on={finance?.authorized}>
				<Icon icon="ri:bank-card-line" width={18} />
			</div>
			<div class="s-body">
				<div class="s-title">Finance</div>
				<div class="s-sub">
					{#if finance?.authorized}
						On{#if financeSync && financeSync.queued > 0} · {financeSync.queued} syncing{:else} · synced{/if}
					{:else}Accounts &amp; transactions{/if}
				</div>
			</div>
			{#if !finance?.authorized}
				<button class="s-action" onclick={enableFinance} disabled={enablingFinance}>
					{enablingFinance ? "Enabling…" : "Enable"}
				</button>
			{:else}
				<span class="dot on"></span>
			{/if}
		</div>
		<div class="stream">
			<div class="s-icon" class:on={audio?.recording}>
				<Icon icon="ri:mic-line" width={18} />
			</div>
			<div class="s-body">
				<div class="s-title">Audio</div>
				<div class="s-sub">
					{#if audio?.recording}
						Recording{#if audioSync && audioSync.queued > 0} · {audioSync.queued} syncing{:else} · synced{/if}
					{:else if audio?.authorized}
						Paused
					{:else}Ambient sound &amp; transcripts{/if}
				</div>
			</div>
			<button
				class="s-action"
				onclick={toggleAudio}
				disabled={togglingAudio}
			>
				{#if togglingAudio}…{:else if audio?.recording}Stop{:else if audio?.authorized}Resume{:else}Enable{/if}
			</button>
		</div>
		{#if audio?.authorized}
			<button class="s-subrow" onclick={toggleAudioNotify} type="button">
				<span class="s-subrow-label">Notify me if recording stops</span>
				<span class="switch" class:on={audio?.notify} aria-hidden="true"></span>
			</button>
			<button class="s-subrow" onclick={toggleQuietHours} type="button">
				<span class="s-subrow-label">Quiet hours</span>
				<span class="switch" class:on={quietOn} aria-hidden="true"></span>
			</button>
			{#if quietOn && audio}
				<div class="s-subrow s-times">
					<input
						class="s-time"
						type="time"
						value={minToTime(audio.quietStart ?? 0)}
						onchange={(e) => setQuietHours(timeToMin(e.currentTarget.value), audio?.quietEnd ?? 0)}
					/>
					<span class="s-subrow-label">to</span>
					<input
						class="s-time"
						type="time"
						value={minToTime(audio.quietEnd ?? 0)}
						onchange={(e) => setQuietHours(audio?.quietStart ?? 0, timeToMin(e.currentTarget.value))}
					/>
					<span class="s-subrow-label s-times-note">mic stays on, nothing is kept</span>
				</div>
			{/if}
		{/if}
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
		{#if radio}
			<div class="stream">
				<div class="s-icon" class:on={radio.parked}>
					<Icon icon="ri:battery-charge-line" width={18} />
				</div>
				<div class="s-body">
					<div class="s-title">{radio.parked ? "Radio resting" : "Link active"}</div>
					<div class="s-sub">
						{radio.drains} uploads · {radio.dials} dials · {fmtBytes(radio.bytes)} sent{#if audio?.silentDropped}
							· {audio.silentDropped} silent chunks kept local{/if}
					</div>
				</div>
			</div>
		{/if}
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
		<!-- Temporary dev tooling: the Phase-1a keyboard spike (mobile-ux-plan). -->
		<button class="probe-row" onclick={() => (probeOpen = true)} type="button">
			Keyboard probe
		</button>
		<button class="danger-row" onclick={unpairDevice} disabled={forgetting} type="button">
			{forgetting ? "Unpairing…" : "Unpair this device"}
		</button>
	</div>

</div>

{#if probeOpen}
	<MobileKeyboardProbe close={() => (probeOpen = false)} />
{/if}

<style>
	.device {
		padding-bottom: 8px;
	}
	.group-label {
		display: flex;
		align-items: center;
		justify-content: space-between;
		font-size: 11px;
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
	/* Secondary toggle row beneath a stream (e.g. audio gap-nudge). */
	.s-subrow {
		display: flex;
		align-items: center;
		justify-content: space-between;
		width: 100%;
		gap: 12px;
		padding: 11px 14px 11px 48px;
		border: none;
		border-top: 1px solid var(--color-border);
		background: transparent;
		cursor: pointer;
		text-align: left;
	}
	.s-subrow-label {
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
	.s-times {
		justify-content: flex-start;
		cursor: default;
	}
	.s-time {
		font: inherit;
		font-size: 13px;
		color: var(--color-foreground);
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		padding: 3px 6px;
	}
	.s-times-note {
		margin-left: auto;
		font-size: 11px;
	}
	.switch {
		flex: none;
		width: 38px;
		height: 22px;
		border-radius: 11px;
		background: var(--color-foreground-muted);
		opacity: 0.4;
		position: relative;
		transition: background 0.15s, opacity 0.15s;
	}
	.switch::after {
		content: "";
		position: absolute;
		top: 2px;
		left: 2px;
		width: 18px;
		height: 18px;
		border-radius: 50%;
		background: #fff;
		transition: transform 0.15s;
	}
	.switch.on {
		background: var(--color-success);
		opacity: 1;
	}
	.switch.on::after {
		transform: translateX(16px);
	}
	.dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--color-foreground-muted);
	}
	.dot.on {
		background: var(--color-success);
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
		letter-spacing: 0.03em;
		padding: 1px 6px;
		border-radius: 5px;
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		color: var(--color-foreground-muted);
	}
	.l-badge.bg {
		background: color-mix(in srgb, var(--color-success) 20%, transparent);
		color: color-mix(in srgb, var(--color-success) 75%, #000);
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
	.danger-row {
		display: block;
		width: 100%;
		padding: 12px 14px;
		border: none;
		border-top: 1px solid var(--color-border);
		background: transparent;
		color: var(--color-error);
		font-size: 14px;
		font-weight: 600;
		text-align: left;
		cursor: pointer;
	}
	.probe-row {
		display: block;
		width: 100%;
		padding: 12px 14px;
		border: none;
		border-top: 1px solid var(--color-border);
		background: transparent;
		color: var(--color-foreground-muted);
		font-size: 14px;
		text-align: left;
		cursor: pointer;
	}
	.danger-row:disabled {
		opacity: 0.5;
	}

	.empty {
		padding: 18px 14px;
		font-size: 13px;
		color: var(--color-foreground-muted);
		line-height: 1.4;
	}
	.empty.err {
		color: var(--color-error);
		font-variant-numeric: tabular-nums;
	}
</style>
