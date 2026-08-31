<!--
  One device — the only page about a device, whichever device it is.

  Replaces three: DeviceDetailView (the box's thin view of any device),
  ThisMacView (the Mac's local instrument panel), and ThisDeviceView (the same
  for iOS). They were three because a device can be looked at from two places —
  what your SERVER knows about it, and what the DEVICE knows about itself — and
  those answer different questions with different data.

  Splitting them by vantage was right; making the reader discover the vantage
  was not. Permissions appeared on two pages with different values, the version
  appeared on two pages in different shapes, and nothing said why they
  disagreed. On 2026-08-28 that cost a day: a collector reporting 1.0.0 beside
  an app reporting 1.0.25, and no screen able to say which number governed what.

  So the vantage is furniture now. Every section states where its facts come
  from — `measured here` or `as your server heard it` — and the local sections
  say so plainly rather than rendering empty panels that read as broken.

  What the box holds per device is `DeviceListItem`: identity, pairing, last
  contact, reported build, self-reported permissions. Everything richer —
  queue depth, live permission probes, the update state — comes from a Tauri
  plugin over local IPC and exists only on the machine it describes.
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page, Button, Card, Section, LoadingState, ErrorState } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { onDestroy, onMount } from "svelte";
	import { createResource } from "$lib/utils/resource.svelte";
	import { formatTimeAgo } from "$lib/utils/dateUtils";
	import { listDevices } from "$lib/api/client";
	import { isTauri, isMacOS } from "$lib/utils/platform";
	import { BUILD, buildLabel } from "$lib/build";
	import {
		kindLabel,
		kindIcon,
		deniedPermissions,
		grantedPermissions,
		revokeDeviceFlow,
		backToDevices,
		type Device,
		type DevicesResponse,
	} from "$lib/devices/shared";
	import {
		getCollectorStatus,
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

	let { deviceId, tab, active }: { deviceId: string; tab?: Tab; active?: boolean } = $props();

	const res = createResource(() => listDevices<DevicesResponse>());
	const devices = $derived(res.data?.devices ?? []);

	// `this` is a stable alias for whichever row is making the request, so a
	// bookmark or the sidebar can point at "this machine" without knowing an id
	// that changes on every re-pair.
	const device = $derived<Device | null>(
		deviceId === "this"
			? (devices.find((d) => d.is_current) ?? null)
			: (devices.find((d) => d.id === deviceId) ?? null),
	);

	// The collector this machine installed, folded in rather than listed apart.
	// `installed_by` is stamped at pairing for exactly this — without it one Mac
	// reads as two unrelated devices, which is how a stale collector version sat
	// next to a current app version with nothing connecting them.
	const collector = $derived<Device | null>(
		device ? (devices.find((d) => d.installed_by === device.id) ?? null) : null,
	);

	/**
	 * Are we standing on the device we are describing?
	 *
	 * The whole page turns on this. Local means a Tauri shell is here to ask;
	 * everything else is what crossed the wire. A plain browser is never local
	 * even on the right machine — there is no daemon to read.
	 */
	const local = $derived(!!device?.is_current && isTauri);
	const localMac = $derived(local && isMacOS);

	const denied = $derived(device ? deniedPermissions(device) : []);
	const granted = $derived(device ? grantedPermissions(device) : []);

	// ── Local instrument readings (localMac only) ────────────────────────────
	let status = $state<CollectorStatus | null>(null);
	let shell = $state<ShellIdentity | null>(null);
	let upd = $state<AppUpdateState | null>(null);
	let toggling = $state(false);
	let poll: ReturnType<typeof setInterval> | null = null;

	async function readLocal() {
		status = await getCollectorStatus();
		upd = await appUpdateState();
	}

	onMount(async () => {
		if (!local) return;
		shell = await shellIdentity();
		await readLocal();
		poll = setInterval(() => void readLocal(), 2000);
	});
	onDestroy(() => {
		if (poll) clearInterval(poll);
	});

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

	const queued = $derived((status?.pendingEvents ?? 0) + (status?.pendingMessages ?? 0));

	/**
	 * The app's freshness, in one phrase.
	 *
	 * "Couldn't check" is deliberately NOT folded into "up to date". A failed
	 * check and a passed one are different facts, and rendering them the same
	 * is the habit that let a box sit ten days behind while every screen looked
	 * healthy.
	 */
	const appFreshness = $derived.by(() => {
		if (!local) return { text: "", tone: "muted" as const };
		if (upd?.stagedVersion) return { text: `v${upd.stagedVersion} ready`, tone: "info" as const };
		if (upd?.lastCheck?.outcome === "failed")
			return { text: "Couldn't check", tone: "warning" as const };
		if (upd?.lastCheck?.outcome === "up_to_date")
			return { text: "Up to date", tone: "muted" as const };
		return { text: "", tone: "muted" as const };
	});

	/**
	 * The collector runs a build the app installs, so a lasting disagreement
	 * means a relaunch has not happened — not that the collector is wrong.
	 * Both numbers are stamped from the same source since 2026-08-28; before
	 * that the collector's was a literal and this line could never be quiet.
	 */
	const appVersion = $derived((local ? shell?.appVersion : device?.app_version) ?? null);
	const collectorBehind = $derived(
		!!(collector?.version && appVersion && collector.version !== appVersion),
	);

	/**
	 * Does this kind of device carry a native shell at all?
	 *
	 * A CLI session and a bare sensor have no app, no interface bundle and no
	 * collector — rendering those rows as "—" invents three absences that are
	 * not facts about the device, only about the template. An empty row reads
	 * as a missing value; the honest thing is not to claim the field exists.
	 */
	const hasApp = $derived(device?.kind === "mobile_app" || device?.kind === "desktop_app");

	const reaching = $derived.by(() => {
		if (local) return { text: "On this device", tone: "muted" as const };
		if (!device?.last_seen_at) return { text: "Never reached your server", tone: "warning" as const };
		return { text: `Last reached ${formatTimeAgo(device.last_seen_at)}`, tone: "muted" as const };
	});
</script>

{#if res.loading}
	<LoadingState />
{:else if res.error}
	<ErrorState message={String(res.error)} />
{:else if !device}
	<Page title="No such device" description="It may have been revoked.">
		<Button variant="secondary" onclick={backToDevices}>Back to devices</Button>
	</Page>
{:else}
	<Page title={device.label} description="What this device runs, and whether it is reaching your server.">
		<!-- ── Header: the two questions people open this page for ─────────── -->
		<Card>
			<div class="flex items-center gap-3">
				<Icon icon={kindIcon(device.kind)} class="text-foreground-muted flex-none" />
				<div class="flex-1 min-w-0">
					<div class="text-sm font-medium text-foreground">
						{kindLabel(device.kind)}
						{#if device.paired_at}
							<span class="text-foreground-subtle font-normal">
								· paired {formatTimeAgo(device.paired_at)}
							</span>
						{/if}
					</div>
					<div
						class="text-xs mt-0.5"
						class:text-warning={reaching.tone === "warning"}
						class:text-foreground-muted={reaching.tone === "muted"}
					>
						{reaching.text}
					</div>
				</div>
			</div>
		</Card>

		<!-- ── Now — live, and only where there is something live to read ──── -->
		{#if localMac}
			<Section title="Now" note="measured here">
			<Card>
				<div class="flex items-center gap-3">
					<span
						class={`w-2.5 h-2.5 rounded-full flex-none ${
							status?.paused ? "bg-warning" : "bg-success"
						}`}
					></span>
					<div class="flex-1 min-w-0">
						<div class="text-sm font-medium text-foreground">
							{status?.paused ? "Paused" : "Collecting"}
						</div>
						<div class="text-xs text-foreground-muted mt-0.5">
							{#if queued > 0}
								{queued} queued{status?.lastSync ? ` · synced ${status.lastSync}` : ""}
							{:else if status?.lastSync}
								Synced {status.lastSync}
							{:else}
								Uploaded to your server over your private link
							{/if}
						</div>
					</div>
					<Button variant="secondary" onclick={togglePause} disabled={toggling}>
						{toggling ? "…" : status?.paused ? "Resume" : "Pause"}
					</Button>
				</div>
			</Card>
			</Section>
		{/if}

		<!-- ── Software — ONE ledger, and each line says what it governs ────
		     Three artifacts move independently and have no reason to agree: the
		     native shell ships through Apple, the interface ships with the
		     server, the collector ships inside the app. Printing three bare
		     numbers is what made "is this current?" unanswerable. -->
		{#if hasApp}
		<Section title="Software" note={local ? "measured here" : "as your server heard it"}>
		<Card list>
		<ul>
			<li class="p-4 flex items-center gap-3">
				<div class="flex-1 min-w-0">
					<div class="text-sm text-foreground">App</div>
					<div class="text-xs text-foreground-subtle mt-0.5">the shell, and what it can do</div>
				</div>
				<div class="text-right">
					<div class="text-xs font-mono text-foreground-muted">
						{appVersion ?? "—"}
					</div>
					{#if appFreshness.text}
						<div
							class="text-[11px] mt-0.5"
							class:text-warning={appFreshness.tone === "warning"}
							class:text-info={appFreshness.tone === "info"}
							class:text-foreground-subtle={appFreshness.tone === "muted"}
						>
							{appFreshness.text}
						</div>
					{/if}
				</div>
				{#if local && upd?.stagedVersion}
					<Button variant="secondary" onclick={() => void applyAppUpdate()}>Relaunch</Button>
				{:else if local}
					<Button variant="ghost" onclick={() => void checkAppUpdate()}>Check</Button>
				{/if}
			</li>

			<li class="p-4 flex items-center gap-3">
				<div class="flex-1 min-w-0">
					<div class="text-sm text-foreground">Interface</div>
					<div class="text-xs text-foreground-subtle mt-0.5">
						the screens you are looking at — served by your server
					</div>
				</div>
				<div class="text-xs font-mono text-foreground-muted">
					{local ? buildLabel(BUILD) : (device.version ?? "—")}
				</div>
			</li>

			{#if collector}
				<!-- Nested, not listed apart. The collector is a part of this
				     machine, and showing it as its own device is what made one
				     Mac read as two. -->
				<li class="p-4 flex items-center gap-3">
					<div class="flex-1 min-w-0">
						<div class="text-sm text-foreground">Collector</div>
						<div class="text-xs text-foreground-subtle mt-0.5">
							the daemon that reads this machine and sends it on
						</div>
					</div>
					<div class="text-right">
						<div class="text-xs font-mono text-foreground-muted">{collector.version ?? "—"}</div>
						{#if collectorBehind}
							<div class="text-[11px] text-warning mt-0.5">behind the app — relaunch</div>
						{/if}
					</div>
				</li>
			{/if}
		</ul>
		</Card>
		</Section>
		{/if}

		<!-- ── Permissions ─────────────────────────────────────────────────
		     Live probe when we are standing here; the device's own last report
		     otherwise. Never silently merged — a granted permission the box was
		     told about months ago is not the same claim as one read a second
		     ago, and the label is what keeps them apart. -->
		{#if denied.length || granted.length}
			<Section title="Permissions" note={local ? "measured here" : "as this device last reported"}>
			<Card list>
			<ul>
				{#each denied as perm (perm.label)}
					<li class="p-4 flex items-center gap-3">
						<Icon icon="ri:error-warning-line" class="text-warning flex-none" />
						<div class="flex-1 min-w-0">
							<div class="text-sm text-foreground">{perm.label}</div>
							<div class="text-xs text-foreground-muted mt-0.5">{perm.costs}</div>
						</div>
						{#if local && perm.open}
							<Button variant="secondary" onclick={() => void perm.open?.()}>Open Settings</Button>
						{/if}
					</li>
				{/each}
				{#each granted as perm (perm.label)}
					<li class="p-4 flex items-center gap-3">
						<Icon icon="ri:checkbox-circle-line" class="text-success flex-none" />
						<div class="flex-1 min-w-0 text-sm text-foreground">{perm.label}</div>
					</li>
				{/each}
			</ul>
			</Card>
			</Section>
		{/if}

		<!-- ── Feeds ───────────────────────────────────────────────────────── -->
		{#if device.source_id}
			<Section title="Feeds" note="as your server heard it">
			<Card class="text-sm text-foreground">
				Sends as <span class="font-mono text-foreground-muted">{device.source_id}</span>
			</Card>
			</Section>
		{/if}

		<!-- ── The honest sentence, where a panel would otherwise look broken ── -->
		{#if !local && hasApp}
			<!-- Only for a device that HAS instruments and is not the one you are
			     holding. Saying it on the local console pointed someone at the
			     browser they were already reading it in. -->
			<p class="text-xs text-foreground-subtle mt-6 leading-relaxed max-w-prose">
				Queue depth, live permission checks and update state are read on the device itself and
				never sent to your server. Open this page on {device.label} to see them.
			</p>
		{/if}

		<div class="mt-8">
			<Button variant="ghost" onclick={() => void revokeDeviceFlow(device)}>
				Revoke this device
			</Button>
		</div>
	</Page>
{/if}
