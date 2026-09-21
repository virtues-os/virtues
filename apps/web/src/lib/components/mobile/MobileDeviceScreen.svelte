<script lang="ts">
	/**
	 * Settings — the native collector dashboard (iOS/Android shell only).
	 *
	 * This phone as a data collector. The list keeps every stream to one line
	 * — name, state, a chevron — and each stream's controls (enable, stop,
	 * when, where, notify) live on its own page (`MobileStreamPage`). The
	 * page and the list read the same status objects, so they cannot
	 * disagree.
	 *
	 * Around the streams: the link to the server, Sync with Recent activity
	 * as a row beneath it (the full log is its own page — it used to sit
	 * inline as three runs and a strip of dots, which made the settings list
	 * scroll past a log nobody had asked to read), the profile, and About.
	 * The radio counters that used to headline the Sync card are the battery
	 * bench's instruments, not settings; they sit under About now.
	 */
	import Icon from "$lib/components/Icon.svelte";
	import { mobileLayout } from "$lib/stores/mobileLayout.svelte";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { confirmAction } from "$lib/stores/dialog.svelte";
	import { invoke } from "@tauri-apps/api/core";
	import { getVersion } from "@tauri-apps/api/app";
	import { onMount } from "svelte";
	import MobileStreamPage from "./MobileStreamPage.svelte";
	import MobileAudioSettings from "./MobileAudioSettings.svelte";
	import type { AudioStatus, OutboxStats, StreamKey, StreamStatus } from "$lib/mobile/deviceTypes";

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
		session?: string;
		reachable: boolean; // live: box actually answered a probe just now
		path?: string;
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
	let health = $state<StreamStatus | null>(null);
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
	let version = $state<string>("");
	let loading = $state(true);
	let starting = $state(false);
	let enablingHealth = $state(false);
	let enablingCal = $state(false);
	let enablingContacts = $state(false);
	let enablingFinance = $state(false);
	let error = $state<string | null>(null);
	/** Which stream's page is open, if any. */
	let open = $state<StreamKey | null>(null);
	/** The Recent activity page. */
	let activityOpen = $state(false);

	/** Location fixes only — the probe also writes 0,0 marker rows (start,
	 * mode changes, regions) for the field log, which are not places. */
	const fixes = $derived(rows.filter((r) => !(r.lat === 0 && r.lon === 0)));
	const enabled = $derived(fixes.length > 0);
	const lastTs = $derived(fixes[0]?.ts ?? null);
	const lastFix = $derived(fixes[0] ? { lat: fixes[0].lat, lon: fixes[0].lon } : null);

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
		for (const r of fixes) {
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
	/** The activity row's one line, and the page's status. */
	const activityLine = $derived(
		fixes.length === 0
			? "No location fixes yet"
			: `${fixes.length} location fixes · last ${rel(lastTs ?? "")}`,
	);

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
				// A page's worth of history now that the log has a page; 60 was
				// sized for an inline strip that showed three runs.
				invoke<{ rows: ProbeRow[] }>("plugin:location-probe|read_rows", {
					payload: { limit: 300 },
				}),
				invoke<ReachStatus>("plugin:reach|reach_status").catch(() => null),
				invoke<OutboxStats>("plugin:reach|outbox_stats", { stream: "location" }).catch(() => null),
				invoke<StreamStatus>("plugin:health|status").catch(() => null),
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
			health = await invoke<StreamStatus>("plugin:health|enable");
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

	/// The one stream that records people who never consented gets a real
	/// consent beat before the OS dialog: the first Enable opens the
	/// interstitial; only its confirm actually starts recording. Once
	/// authorized, the button is a plain pause control and the interstitial
	/// never returns.
	let audioConsentOpen = $state(false);

	/// "The user has recording switched on" — capturing, or paused for a
	/// reason they did not choose (CarPlay). One derivation for the control
	/// and its label, so they cannot disagree: a label reading Resume over a
	/// handler that calls disable is exactly the re-evict this guards.
	const audioOn = $derived(!!(audio?.recording || audio?.pausedReason));

	async function toggleAudio() {
		togglingAudio = true;
		error = null;
		try {
			audio = await invoke<AudioStatus>(audioOn ? "plugin:audio|disable" : "plugin:audio|enable");
			setTimeout(load, 2000);
		} catch (e) {
			error = String(e);
		} finally {
			togglingAudio = false;
		}
	}

	function audioAction() {
		if (!audio?.authorized) audioConsentOpen = !audioConsentOpen;
		else void toggleAudio();
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

	// ── The streams, as the list and the pages both see them ────────────
	function syncWord(s: OutboxStats | null): string {
		if (!s) return "";
		if (s.failing > 0) return ` · ${s.failing} retrying`;
		if (s.queued > 0) return ` · ${s.queued} syncing`;
		return " · synced";
	}

	interface StreamMeta {
		key: StreamKey;
		title: string;
		icon: string;
		/** What it collects — the sub-line while off, and the page's sentence. */
		what: string;
		description: string;
	}
	const STREAMS: StreamMeta[] = [
		{
			key: "location",
			title: "Location",
			icon: "ri:map-pin-line",
			what: "Where you are, through the day",
			description:
				"Fixes from the phone's location services, including while the app is in the background. They become the visits and movements in your record, and they keep the app alive for the other streams.",
		},
		{
			key: "health",
			title: "Health",
			icon: "ri:heart-pulse-line",
			what: "Steps, sleep, heart rate",
			description: "What the Health app already keeps — steps, sleep, heart rate, workouts — copied to your server as it lands.",
		},
		{
			key: "calendar",
			title: "Calendar",
			icon: "ri:calendar-line",
			what: "Events & invitations",
			description: "Your calendars' events and invitations. They are plans, not evidence; the record checks them against what actually happened.",
		},
		{
			key: "contacts",
			title: "Contacts",
			icon: "ri:contacts-book-line",
			what: "Names for the people in your record",
			description: "Your address book, so the people in messages, calls and calendars are known by name.",
		},
		{
			key: "finance",
			title: "Finance",
			icon: "ri:bank-card-line",
			what: "Accounts & transactions",
			description: "Accounts and transactions from Apple Wallet and Apple Card, through the phone's Finance framework.",
		},
		{
			key: "audio",
			title: "Audio",
			icon: "ri:mic-line",
			what: "Ambient sound & transcripts",
			description:
				"The microphone stays on while your phone is with you and records the sound of your day. Recordings are transcribed on your server and become part of each day's record.",
		},
	];

	function streamOn(k: StreamKey): boolean {
		switch (k) {
			case "location":
				return enabled;
			case "health":
				return !!health?.authorized;
			case "calendar":
				return !!cal?.authorized;
			case "contacts":
				return !!contacts?.authorized;
			case "finance":
				return !!finance?.authorized;
			case "audio":
				return !!audio?.recording;
		}
	}

	function streamStatus(k: StreamKey): string {
		const meta = STREAMS.find((s) => s.key === k)!;
		if (loading && k === "location") return "Checking…";
		switch (k) {
			case "location":
				return enabled ? `On · ${fixes.length} recent${lastTs ? ` · ${rel(lastTs)}` : ""}` : "Off";
			case "health":
				return health?.authorized ? `On${syncWord(healthSync)}` : meta.what;
			case "calendar":
				return cal?.authorized ? `On${syncWord(calSync)}` : meta.what;
			case "contacts":
				return contacts?.authorized ? `On${syncWord(contactsSync)}` : meta.what;
			case "finance":
				return finance?.authorized ? `On${syncWord(financeSync)}` : meta.what;
			case "audio":
				if (audio?.mutedBy === "schedule") return "On · not recording now (hours)";
				if (audio?.mutedBy === "place") return "On · not recording here";
				if (audio?.recording) return `Recording${syncWord(audioSync)}`;
				if (audio?.pausedReason === "carplay") return "Paused · CarPlay";
				if (audio?.authorized) return "Paused";
				return meta.what;
		}
	}

	function streamSync(k: StreamKey): OutboxStats | null {
		switch (k) {
			case "location":
				return sync;
			case "health":
				return healthSync;
			case "calendar":
				return calSync;
			case "contacts":
				return contactsSync;
			case "finance":
				return financeSync;
			case "audio":
				return audioSync;
		}
	}

	function streamAction(k: StreamKey): { label: string; onclick: () => void; disabled?: boolean } | null {
		switch (k) {
			case "location":
				return enabled ? null : { label: starting ? "Enabling…" : "Enable", onclick: enableLocation, disabled: starting };
			case "health":
				return health?.authorized
					? null
					: { label: enablingHealth ? "Enabling…" : "Enable", onclick: enableHealth, disabled: enablingHealth };
			case "calendar":
				return cal?.authorized
					? null
					: { label: enablingCal ? "Enabling…" : "Enable", onclick: enableCalendar, disabled: enablingCal };
			case "contacts":
				return contacts?.authorized
					? null
					: { label: enablingContacts ? "Enabling…" : "Enable", onclick: enableContacts, disabled: enablingContacts };
			case "finance":
				return finance?.authorized
					? null
					: { label: enablingFinance ? "Enabling…" : "Enable", onclick: enableFinance, disabled: enablingFinance };
			case "audio":
				return {
					label: togglingAudio ? "…" : audioOn ? "Stop" : audio?.authorized ? "Resume" : "Enable",
					onclick: audioAction,
					disabled: togglingAudio,
				};
		}
	}

	const openMeta = $derived(open ? STREAMS.find((s) => s.key === open)! : null);

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
				<div class="s-sub">{conn.sub}</div>
			</div>
			<span class="dot" class:on={conn.tone === "on"} class:off={conn.tone === "off"}></span>
		</div>
	</div>

	<!-- "This phone", not "Streams": under a page titled Settings, the group
	     label is what says these rows are about the device in hand. -->
	<div class="group-label">This phone</div>
	<div class="card">
		{#each STREAMS as s (s.key)}
			{@const on = streamOn(s.key)}
			<button class="stream link" type="button" onclick={() => (open = s.key)}>
				<div class="s-icon" class:on>
					<Icon icon={s.icon} width={18} />
				</div>
				<div class="s-body">
					<div class="s-title">{s.title}</div>
					<div class="s-sub">{streamStatus(s.key)}</div>
				</div>
				{#if on}
					<span class="dot on" class:hollow={s.key === "audio" && !!audio?.mutedBy}></span>
				{/if}
				<Icon icon="ri:arrow-right-s-line" width={18} class="chev" />
			</button>
		{/each}
	</div>

	<div class="group-label">Sync</div>
	<div class="card">
		<div class="stream">
			<div class="s-icon" class:on={!!sync && sync.queued === 0}>
				<Icon icon="ri:refresh-line" width={18} />
			</div>
			<div class="s-body">
				<div class="s-title">
					{#if !sync}—{:else if sync.queued === 0}Synced to your server{:else}{sync.queued} waiting to sync{/if}
				</div>
				<div class="s-sub">
					{#if sync && sync.failing > 0}{sync.failing} retrying{:else}Uploaded over your private link{/if}
				</div>
			</div>
			<button class="s-action" onclick={syncNow} disabled={syncingNow}>
				{syncingNow ? "Syncing…" : "Sync now"}
			</button>
		</div>
		<!-- What the phone has recorded lately, as a door (VIR-348). It sits
		     under Sync because that is the question it answers: is anything
		     being kept, and when was the last of it. -->
		<button class="stream link" type="button" onclick={() => (activityOpen = true)}>
			<div class="s-icon" class:on={enabled}>
				<Icon icon="ri:history-line" width={18} />
			</div>
			<div class="s-body">
				<div class="s-title">Recent activity</div>
				<div class="s-sub">{loading ? "Loading…" : activityLine}</div>
			</div>
			<Icon icon="ri:arrow-right-s-line" width={18} class="chev" />
		</button>
	</div>

	<!-- The profile lives on the box (name, appearance); this is its only door
	     on the phone since the drawer's gear went. A route push, so the way
	     back is the drawer, like every other page on the phone. -->
	<div class="group-label">You</div>
	<div class="card">
		<button
			class="stream link"
			type="button"
			onclick={() => windowShellStore.openTabFromRoute("/virtues/you", { label: "Profile" })}
		>
			<div class="s-icon">
				<Icon icon="ri:user-line" width={18} />
			</div>
			<div class="s-body">
				<div class="s-title">Profile</div>
				<div class="s-sub">Your name, and how the app looks</div>
			</div>
			<Icon icon="ri:arrow-right-s-line" width={18} class="chev" />
		</button>
	</div>

	<div class="group-label">About</div>
	<div class="card">
		<div class="about">
			<span>App version</span><span class="v">{version || "—"}</span>
		</div>
		<div class="about">
			<span>Recorded points</span><span class="v">{fixes.length}</span>
		</div>
		{#if radio}
			<div class="about">
				<span>{radio.parked ? "Radio resting" : "Link active"}</span>
				<span class="v">{radio.drains} uploads · {fmtBytes(radio.bytes)}</span>
			</div>
		{/if}
		<button class="danger-row" onclick={unpairDevice} disabled={forgetting} type="button">
			{forgetting ? "Unpairing…" : "Unpair this device"}
		</button>
	</div>
</div>

{#if open && openMeta}
	<MobileStreamPage
		title={openMeta.title}
		icon={openMeta.icon}
		status={streamStatus(open)}
		on={streamOn(open)}
		description={openMeta.description}
		action={streamAction(open)}
		sync={streamSync(open)}
		{error}
		onBack={() => {
			open = null;
			audioConsentOpen = false;
		}}
	>
		{#if open === "audio"}
			{#if audioConsentOpen && !audio?.authorized}
				<!-- The consent beat this one stream earns. -->
				<div class="consent">
					<p>
						The microphone stays on while your phone is with you. It records the sound of
						your day — and everyone in the room. Recordings and transcripts go to your
						server and nowhere else.
					</p>
					<p>
						Recording hours can silence any part of the week, and a place can be marked
						"never record here". Other people's voices will still be in the record: in
						some places, recording a conversation needs everyone's consent. That part is
						yours to honor.
					</p>
					<div class="consent-actions">
						<button
							class="s-action"
							onclick={() => {
								audioConsentOpen = false;
								void toggleAudio();
							}}
							disabled={togglingAudio}
						>
							Turn the microphone on
						</button>
						<button class="s-action quiet" onclick={() => (audioConsentOpen = false)}>Not now</button>
					</div>
				</div>
			{/if}
			{#if audio?.authorized}
				<MobileAudioSettings
					{audio}
					fix={lastFix}
					onStatus={(s) => (audio = s)}
					onError={(m) => (error = m)}
				/>
			{/if}
		{/if}
	</MobileStreamPage>
{/if}

{#if activityOpen}
	<!-- Every run, newest first, in the stream page's frame: the same back
	     header and status card the streams get, no primary action beyond a
	     refresh. Consecutive fixes at one spot are one run with a count, so
	     a phone that sat on a desk all afternoon is one line, not forty. -->
	<MobileStreamPage
		title="Recent activity"
		icon="ri:history-line"
		status={activityLine}
		on={enabled}
		description="Location fixes this phone has recorded, newest first. Fixes at the same spot are collapsed into one run with a count; a filled dot means the app was in the background when it recorded."
		action={{ label: loading ? "Loading…" : "Refresh", onclick: load, disabled: loading }}
		{error}
		onBack={() => (activityOpen = false)}
	>
		<div class="card activity">
			{#if loading && runs.length === 0}
				<div class="empty">Loading…</div>
			{:else if runs.length === 0}
				<div class="empty">No location fixes yet. Turn on Location to see them here.</div>
			{:else}
				{#each runs as r, i (i)}
					<div class="log">
						<span class="l-dot" class:bg={r.appState !== "active"}></span>
						<div class="l-body">
							<div class="l-top">
								<span class="l-time">{rel(r.ts)}</span>
								<span class="l-state">{r.appState === "active" ? "in use" : "background"}</span>
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
	</MobileStreamPage>
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
	.stream.link {
		width: 100%;
		border-left: 0;
		border-right: 0;
		border-top: 0;
		background: transparent;
		color: inherit;
		text-align: left;
		cursor: pointer;
	}
	.stream.link:last-child {
		border-bottom: 0;
	}
	.stream :global(.chev) {
		color: var(--color-foreground-muted);
		flex: none;
		margin-right: -4px;
	}
	.s-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		flex: none;
		border-radius: 8px;
		background: color-mix(in srgb, var(--wash-ink) 6%, transparent);
		color: var(--color-foreground-muted);
	}
	.s-icon.on {
		background: color-mix(in srgb, var(--color-primary) 16%, transparent);
		color: var(--color-primary);
	}
	.s-body {
		flex: 1;
		min-width: 0;
	}
	.s-title {
		font-size: 15px;
		font-weight: 550;
	}
	.s-sub {
		font-size: 12px;
		color: var(--color-foreground-muted);
		margin-top: 1px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.s-action {
		border: 1px solid var(--color-primary);
		color: var(--color-primary);
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
	.s-action.quiet {
		border-color: var(--color-border);
		color: var(--color-foreground-muted);
	}
	.dot {
		width: 8px;
		height: 8px;
		flex: none;
		border-radius: 50%;
		background: var(--color-foreground-muted);
	}
	.dot.on {
		background: var(--color-success);
	}
	/* On, but deliberately keeping nothing right now (a muted place or hour). */
	.dot.on.hollow {
		background: transparent;
		box-shadow: inset 0 0 0 2px var(--color-success);
	}

	/* The consent interstitial on the Audio page: plain sentences, then the choice. */
	.consent {
		margin-top: 16px;
		padding: 14px;
		border: 1px solid var(--color-border);
		border-radius: 12px;
	}
	.consent p {
		margin: 0 0 10px;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}
	.consent-actions {
		display: flex;
		gap: 10px;
		margin-top: 2px;
	}

	/* Recent activity: one run per line, on its own page. */
	.card.activity {
		margin-top: 16px;
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
	.l-dot {
		width: 8px;
		height: 8px;
		flex: none;
		border-radius: 50%;
		margin-top: 6px;
		background: color-mix(in srgb, var(--color-foreground) 22%, transparent);
	}
	.l-dot.bg {
		background: var(--color-success);
	}
	.l-body {
		flex: 1;
		min-width: 0;
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
	.l-state {
		font-size: 11px;
		color: var(--color-foreground-muted);
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
		gap: 12px;
		padding: 12px 14px;
		border-bottom: 1px solid var(--color-border);
		font-size: 14px;
	}
	.about .v {
		color: var(--color-foreground-muted);
		font-variant-numeric: tabular-nums;
		text-align: right;
	}
	.danger-row {
		display: block;
		width: 100%;
		padding: 12px 14px;
		border: none;
		background: transparent;
		color: var(--color-error);
		font-size: 14px;
		font-weight: 600;
		text-align: left;
		cursor: pointer;
	}
	.danger-row:disabled {
		opacity: 0.5;
	}
	.empty {
		padding: 14px;
		font-size: 13px;
		color: var(--color-foreground-muted);
	}
</style>
