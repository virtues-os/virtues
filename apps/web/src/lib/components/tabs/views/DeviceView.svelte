<!--
  One device's page, whichever device it is (/virtues/devices/:id), and This
  device (/virtues/devices/this) for the device you're using. An iPhone or a
  Mac you're not using forwards to its page in Sources (`sourcePageOf`).

  A device can be looked at from two places: what your server knows about it,
  and what the device knows about itself. The server holds each device's
  pairing, last contact, reported build, push registration, and for a Mac the
  permissions its collector last reported. Collecting, the live permission
  check and the app's own updates come from the Mac app over local IPC, so
  they show only on the Mac app itself. A browser on a paired computer counts
  as that computer but has no bridge to ask, so it shows what the server knows.

  What the page holds depends on the kind (`platformOf`): a Mac, a phone, a
  Windows or Linux computer, an AI agent, or your server's console. The iPhone
  app's own This device is ThisDeviceView; SettingsView routes it there.
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { onMount } from "svelte";
	import SettingsPage from "$lib/components/settings/SettingsPage.svelte";
	import LoadError from "$lib/components/settings/LoadError.svelte";
	import LinkRow from "$lib/components/settings/LinkRow.svelte";
	import Button from "$lib/components/Button.svelte";
	import MacPermissions from "$lib/components/devices/MacPermissions.svelte";
	import { createResource } from "$lib/utils/resource.svelte";
	import { listDevices } from "$lib/api/client";
	import { isTauri, isMacOS } from "$lib/utils/platform";
	import { BUILD, buildLabel } from "$lib/build";
	import { windowShellStore } from "$lib/stores/window-shell.svelte";
	import { lightPendingSetting } from "$lib/settings/light";
	import {
		collectorOf,
		deviceName,
		kindSentence,
		machineWord,
		pairedOn,
		platformOf,
		reachedAgo,
		reportedPermissions,
		revokeDeviceFlow,
		sourcePageOf,
		type Device,
		type DevicesResponse,
	} from "$lib/devices/shared";
	import {
		getCollectorStatus,
		recheckCollector,
		pauseCollector,
		resumeCollector,
		shellIdentity,
		appUpdateState,
		applyAppUpdate,
		checkAppUpdate,
		type CollectorStatus,
		type ShellIdentity,
		type AppUpdateState,
	} from "$lib/tauri/bridge";
	import { uploadsHold } from "$lib/reload";

	let { deviceId, tab, active }: { deviceId: string; tab?: Tab; active?: boolean } = $props();

	const res = createResource(() => listDevices<DevicesResponse>());
	const devices = $derived(res.data?.devices ?? []);

	// `this` stands for whichever row is making the request, so a link can
	// point at "this machine" without knowing an id that changes on a re-pair.
	const device = $derived<Device | null>(
		deviceId === "this"
			? (devices.find((d) => d.is_current) ?? null)
			: (devices.find((d) => d.id === deviceId) ?? null),
	);

	// An iPhone or a Mac you're not using has its page in Sources; an old link
	// to it here goes there.
	$effect(() => {
		if (!device || !tab) return;
		const page = sourcePageOf(device, devices);
		if (page) windowShellStore.retarget(tab.id, page, page === "/sources/ios" ? "iPhone" : "Mac");
	});

	// A Mac's collector folds into its page: one Mac, one page.
	const collector = $derived<Device | null>(collectorOf(device, devices));
	const platform = $derived(device ? platformOf(device, !!collector) : null);
	const word = $derived(platform ? machineWord(platform) : "device");

	/** Standing on the device described, with a Tauri shell to ask. */
	const local = $derived(!!device?.is_current && isTauri);
	const localMac = $derived(local && isMacOS);

	// Permissions come from the collector: the app's row carries none. On a
	// collector's own page (its app is gone), that row is the reporter.
	const reporter = $derived<Device | null>(collector ?? (device?.permissions ? device : null));
	const reported = $derived(reportedPermissions(reporter));

	// ── What only the Mac app can read ───────────────────────────────────────
	let status = $state<CollectorStatus | null>(null);
	let shell = $state<ShellIdentity | null>(null);
	let upd = $state<AppUpdateState | null>(null);
	let toggling = $state(false);
	let checking = $state(false);

	async function readLocal() {
		status = await getCollectorStatus();
		upd = await appUpdateState();
	}

	onMount(() => {
		if (local) void shellIdentity().then((s) => (shell = s));
		return lightPendingSetting();
	});

	// Each reading starts the collector CLI as a process, so read only while
	// this view is on screen and the window is in front. The timer starts
	// here, not after an await, so a view closed during the await stops.
	$effect(() => {
		if (!localMac || active === false) return;
		void readLocal();
		const timer = setInterval(() => {
			if (document.visibilityState === "visible") void readLocal();
		}, 2000);
		return () => clearInterval(timer);
	});

	async function recheck() {
		const s = await recheckCollector();
		if (s) status = s;
	}

	async function togglePause() {
		if (!status) return;
		toggling = true;
		try {
			await (status.paused ? resumeCollector() : pauseCollector());
			await readLocal();
		} finally {
			toggling = false;
		}
	}

	async function checkApp() {
		checking = true;
		try {
			await checkAppUpdate();
			upd = await appUpdateState();
		} finally {
			checking = false;
		}
	}

	function relaunch() {
		if (!uploadsHold("relaunch", () => void applyAppUpdate())) void applyAppUpdate();
	}

	const queued = $derived((status?.pendingEvents ?? 0) + (status?.pendingMessages ?? 0));

	const collecting = $derived.by(() => {
		if (!status) return "Checking…";
		if (!status.running) return "Not running.";
		const state = status.paused ? "Paused." : "Collecting.";
		const waiting = queued > 0 ? ` ${queued} waiting to send.` : "";
		const synced = status.lastSync ? ` Synced ${status.lastSync}.` : "";
		return `${state}${waiting}${synced}`;
	});

	const appVersion = $derived((local ? shell?.appVersion : device?.app_version) ?? null);
	const collectorVersion = $derived(collector?.version ?? (localMac ? (status?.version ?? null) : null));

	// A collector runs the build its app installed, so a lasting disagreement
	// means the app hasn't relaunched since it updated.
	const collectorBehind = $derived(!!(collectorVersion && appVersion && collectorVersion !== appVersion));

	/**
	 * The app's own freshness, from its updater. A failed check is said as a
	 * failed check: it is a different fact from "up to date".
	 */
	const appState = $derived.by(() => {
		if (upd?.stagedVersion) return { value: appVersion ?? "", note: `Version ${upd.stagedVersion} is downloaded. Relaunch to use it.`, warn: false };
		if (upd?.lastCheck?.outcome === "failed") return { value: appVersion ?? "", note: "It couldn't check for a newer version.", warn: true };
		if (upd?.lastCheck?.outcome === "up_to_date") return { value: `${appVersion ?? ""}, up to date`, note: "", warn: false };
		return { value: appVersion ?? "Not reported", note: "", warn: false };
	});

	const lede = $derived.by(() => {
		if (!device) return "";
		if (localMac) return "The Mac you're using: what it collects, what it may read, and its app.";
		if (local) return "The computer you're using: its app, and how it looks here.";
		if (platform === "console") return `${kindSentence(device, devices)}.`;
		return `${kindSentence(device, devices)}, paired ${pairedOn(device.paired_at)}.`;
	});

	const lastReached = $derived.by(() => {
		if (!device) return "";
		if (device.is_current) return "Now. This is the device you're using.";
		if (!device.last_seen_at) return "Never";
		const ago = reachedAgo(device.last_seen_at);
		return ago.charAt(0).toUpperCase() + ago.slice(1);
	});

	const permissionsNote = $derived.by(() => {
		if (!device) return "";
		const checked = reporter?.permissions?.checked_at;
		const when = checked ? reachedAgo(checked) : null;
		if (device.is_current) {
			return when
				? `This Mac last reported these ${when}. To change one, open the Virtues app on this Mac.`
				: "To change one, open the Virtues app on this Mac.";
		}
		return when
			? `${device.label} last reported these ${when}. To change one, open this page on that Mac.`
			: "To change one, open this page on that Mac.";
	});

	const unpairCopy = $derived.by(() => {
		if (!device) return "";
		if (platform === "agent") return "The agent stops reaching your server at once. What it already wrote stays.";
		const who = device.is_current ? `This ${word}` : device.label;
		const stops = collector ? `${who} and its collector stop` : `${who} stops`;
		const signsOut = device.is_current ? ", and this window signs out" : "";
		// A Windows or Linux app sends nothing, so there is nothing to keep.
		const sent = collector ? " What they already sent stays." : platform === "ios" ? " What it already sent stays." : "";
		const again = device.is_current ? ` To use Virtues here again, pair this ${word} again.` : " To use it again, pair it again.";
		return `${stops} reaching your server at once${signsOut}.${sent}${again}`;
	});

	async function unpair() {
		if (!device) return;
		const done = await revokeDeviceFlow(device, {
			verb: platform === "agent" ? "Disconnect" : "Unpair",
			with: collector ? [collector] : [],
		});
		if (done) windowShellStore.navigate("/virtues/devices", { label: "Settings · Devices" });
	}

	const back = $derived(
		deviceId === "this" ? { route: "/virtues", label: "Settings" } : { route: "/virtues/devices", label: "Devices" },
	);
</script>

{#snippet fact(label: string, value: string, note = "", warn = false)}
	<div class="fact">
		<span class="label">{label}</span>
		<span class="value-cell">
			<span class="value" class:warn>{value}</span>
			{#if note}<span class="note">{note}</span>{/if}
		</span>
	</div>
{/snippet}

{#if res.error}
	<SettingsPage title="Devices" {back}>
		<LoadError message="Your server couldn't list your devices." onretry={() => void res.reload()} />
	</SettingsPage>
{:else if res.loading && !res.data}
	<SettingsPage title={deviceId === "this" ? "This device" : "Devices"} {back}>
		<p class="quiet">Loading…</p>
	</SettingsPage>
{:else if !device}
	<SettingsPage title="No such device" lede="It may have been unpaired." {back} />
{:else}
	<SettingsPage title={deviceName(device)} {lede} {back}>
		{#if localMac}
			<!-- ── This Mac: what only the app can read ──────────────────── -->
			<section class="group" id="setting-collecting">
				<h2 class="settings-label">Collecting</h2>
				<div class="fact">
					<span class="label">This Mac</span>
					<span class="value-cell">
						<span class="with-action">
							<span class="value">{collecting}</span>
							<Button variant="secondary" disabled={!status?.running || toggling} onclick={togglePause}>
								{status?.paused ? "Resume" : "Pause"}
							</Button>
						</span>
						<span class="note">Pause stops this Mac collecting until you resume it.</span>
					</span>
				</div>
			</section>

			<section class="group" id="setting-permissions">
				<h2 class="settings-label">What it may read</h2>
				<p class="lead-note">Checked on this Mac, and again when you come back from System Settings.</p>
				<div class="mac-permissions"><MacPermissions {status} onRecheck={recheck} /></div>
			</section>

			<section class="group" id="setting-the-app">
				<h2 class="settings-label">The app</h2>
				<div class="fact">
					<span class="label">App</span>
					<span class="value-cell">
						<span class="with-action">
							<span class="value">{appState.value}</span>
							{#if upd?.stagedVersion}
								<Button variant="secondary" onclick={relaunch}>Relaunch</Button>
							{:else}
								<Button variant="secondary" loading={checking} onclick={checkApp}>Check again</Button>
							{/if}
						</span>
						{#if appState.note}<span class="note" class:warn-note={appState.warn}>{appState.note}</span>{/if}
					</span>
				</div>
				{#if collectorVersion}
					{@render fact("Collector", collectorVersion, collectorBehind ? "Older than the app. Relaunch the app to update it." : "", collectorBehind)}
				{/if}
				{@render fact("Screens", buildLabel(BUILD))}
				<LinkRow id="appearance-here" label="Appearance on this Mac" help="Glass, reading font, text size and motion, for this device only." href="/virtues/appearance" />
			</section>
		{:else if local}
			<!-- ── This Windows or Linux computer: no collector, no updater ── -->
			<section class="group" id="setting-the-app">
				<h2 class="settings-label">The app</h2>
				{@render fact("App", appVersion ?? "Not reported")}
				{@render fact("Screens", buildLabel(BUILD), "They come from your server.")}
				<LinkRow id="appearance-here" label="Appearance on this computer" help="Glass, reading font, text size and motion, for this device only." href="/virtues/appearance" />
			</section>
		{:else}
			<!-- ── What your server knows ────────────────────────────────── -->
			<section class="group">
				<h2 class="settings-label">Reaching your server</h2>
				{@render fact("Last reached", lastReached)}
				{#if platform === "ios"}
					{#if device.push_address_at}
						{@render fact("Notifications", "Your server can notify this phone.", `Confirmed ${reachedAgo(device.push_address_at)}.`)}
					{:else}
						{@render fact("Notifications", "Your server can't notify this phone.", "Open the app on the phone and allow notifications.", true)}
					{/if}
					<p class="lead-note spaced">What this phone sends and what it may read are on the phone itself. Open Settings there, under This device.</p>
				{/if}
				{#if platform === "console"}
					<p class="lead-note spaced">
						This stands for anything that reaches your server from the server itself: the virtues command, or a browser
						over SSH. It can't be unpaired.
					</p>
				{/if}
			</section>

			{#if platform === "mac"}
				<section class="group" id="setting-permissions">
					<h2 class="settings-label">What it may read</h2>
					{#if reported.length && reported.some((p) => p.granted !== null)}
						<p class="lead-note">{permissionsNote}</p>
						{#each reported as p (p.key)}
							{@render fact(
								p.label,
								p.granted === true ? "Allowed" : p.granted === false ? "Off" : "Not reported",
								p.granted === false ? `${p.costs.charAt(0).toUpperCase()}${p.costs.slice(1)}.` : "",
								p.granted === false,
							)}
						{/each}
						{#if reporter?.permissions?.stale}
							<p class="lead-note spaced warn-note">Its collector hasn't reported lately. It may have stopped.</p>
						{/if}
					{:else}
						<p class="lead-note">Its collector hasn't reported what it may read yet.</p>
					{/if}
				</section>
			{/if}

			{#if platform === "agent"}
				<section class="group">
					<h2 class="settings-label">What it can do</h2>
					<p class="lead-note">It reaches your server only through these. It can't open the app, your settings or your keys.</p>
					<p class="kind">It can read</p>
					{@render fact("Your records", "Run queries against your records and your wiki.")}
					{@render fact("Search", "Search your whole record by meaning.")}
					{@render fact("Your pages", "Read any page.")}
					<p class="kind">It can change</p>
					{@render fact("Your pages", "Create pages and edit them.")}
					{@render fact("Sites", "Set up sites, run them, and write to their own tables. It can't edit or delete one.")}
				</section>
			{/if}

			{#if platform === "mac" || platform === "ios" || platform === "windows" || platform === "linux" || platform === "desktop"}
				<section class="group" id="setting-the-app">
					<h2 class="settings-label">{device.is_current ? "The app" : "Software"}</h2>
					{@render fact("App", appVersion ?? "Not reported", platform === "ios" ? "New versions come from Apple." : "")}
					{#if collectorVersion}
						{@render fact("Collector", collectorVersion, collectorBehind ? "Older than the app. Relaunch the app on that Mac to update it." : "", collectorBehind)}
					{/if}
					{#if platform === "ios" && device.version}
						{@render fact("Screens", device.version, "They update with your server.")}
					{/if}
					{#if device.is_current}
						<LinkRow id="appearance-here" label="Appearance in this browser" help="Glass, reading font, text size and motion, for this browser only." href="/virtues/appearance" />
					{/if}
				</section>
			{/if}
		{/if}

		{#if platform !== "console"}
			<section class="group">
				<h2 class="settings-label">{platform === "agent" ? "Disconnect" : "Unpair"}</h2>
				<p class="lead-note body">{unpairCopy}</p>
				<button type="button" class="danger" onclick={unpair}>
					{platform === "agent" ? `Disconnect ${device.label}` : device.is_current ? `Unpair this ${word}` : `Unpair ${device.label}`}
				</button>
			</section>
		{/if}
	</SettingsPage>
{/if}

<style>
	.group {
		padding: 24px 0 8px;
	}

	.fact {
		display: grid;
		grid-template-columns: 180px minmax(0, 1fr);
		gap: 16px;
		padding: 8px 0;
		font-size: 15px;
	}

	.value-cell {
		min-width: 0;
	}

	.value {
		color: var(--color-foreground-muted);
	}

	.value.warn,
	.warn-note {
		color: var(--color-warning);
	}

	.note {
		display: block;
		margin-top: 2px;
		font-size: 12px;
		line-height: 1.45;
		color: var(--color-foreground-muted);
	}

	.with-action {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.lead-note {
		margin: 0 0 4px;
		max-width: 60ch;
		font-size: 13px;
		line-height: 1.5;
		color: var(--color-foreground-muted);
	}

	.lead-note.spaced {
		margin-top: 12px;
	}

	.lead-note.body {
		font-size: 14px;
		color: var(--color-foreground-muted);
	}

	.kind {
		margin: 12px 0 0;
		font-size: 12px;
		color: var(--color-foreground-muted);
	}

	/* MacPermissions pads its rows for a list inside a box; this page has no
	   box, so its text lines up with the rows above. */
	.mac-permissions {
		margin: 0 -16px;
	}

	.danger {
		height: 34px;
		margin-top: 12px;
		padding: 0 16px;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-surface);
		font: inherit;
		font-size: 14px;
		color: var(--color-error);
		cursor: pointer;
		transition: background var(--motion-hover-out) var(--ease-premium);
	}

	.danger:hover {
		background: color-mix(in srgb, var(--color-error) 6%, var(--color-surface));
		transition-duration: var(--motion-hover-in);
	}

	.quiet {
		margin: 24px 0 0;
		font-size: 14px;
		color: var(--color-foreground-muted);
	}
</style>
