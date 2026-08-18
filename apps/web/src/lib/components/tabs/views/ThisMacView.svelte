<!--
  This Mac — the desktop analog of the iOS "This device" screen
  (MobileDeviceScreen). One place for the Mac collector's health, permissions,
  and streams:
    - Collector: running/paused (+ pause/resume), queue depth, last sync.
    - Permissions: Full Disk Access (Messages) + Accessibility (on-screen
      context) — live-polled truth + deep-links to the System Settings pane
      (macOS forbids programmatic prompts for these).
    - Streams: what this Mac feeds the box (app usage, messages…), each row
      reflecting whether its permission is granted.
    - Disconnect this Mac: clear the pairing + relaunch (same as Devices).
  Truth-polled every ~2s via the collector daemon (cheap local IPC).
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page, Button } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { onDestroy, onMount } from "svelte";
	import { isTauri, isMacOS, thisComputerLabel } from "$lib/utils/platform";
	import { backToDevices } from "$lib/devices/shared";
	import * as api from "$lib/api/client";
	import {
		getCollectorStatus,
		probeCollectorStatus,
		installCollector,
		pauseCollector,
		resumeCollector,
		openFullDiskAccess,
		openAccessibilitySettings,
		forgetPairing,
		restartApp,
		type CollectorStatus,
	} from "$lib/tauri/bridge";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let status = $state<CollectorStatus | null>(null);
	let loading = $state(true);
	/**
	 * Why the last read failed, when it did and we have never had a good one.
	 *
	 * Without this the page had one waiting state and no way out of it: a Mac
	 * whose `virtues-collector status` exits non-zero answers "error" forever,
	 * and "Checking this Mac…" is a sentence that promises it will resolve.
	 * Kept null once any read succeeds — a running collector's transient blip
	 * must not flash the panel below (the reason `refresh` discards failures
	 * rather than clearing `status`).
	 */
	let probeError = $state<string | null>(null);
	let installing = $state(false);
	let toggling = $state(false);
	let error = $state<string | null>(null);
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	const queued = $derived((status?.pendingEvents ?? 0) + (status?.pendingMessages ?? 0));

	onMount(() => {
		if (isTauri) {
			void refresh();
			pollTimer = setInterval(refresh, 2000);
		} else {
			loading = false;
		}
	});
	onDestroy(() => {
		if (pollTimer) clearInterval(pollTimer);
	});

	async function refresh() {
		const probe = await probeCollectorStatus();
		if (probe.kind === 'ok') {
			status = probe.status;
			probeError = null;
		} else if (!status) {
			// Only when we have nothing good to show. A blip mid-session keeps
			// the last-good panel rather than replacing it with a failure.
			probeError =
				probe.kind === 'error' ? probe.message : "This Mac's collector didn't answer.";
		}
		loading = false;
	}

	async function turnOn() {
		installing = true;
		error = null;
		try {
			const { token } = await api.mintCollectorToken();
			await installCollector(token);
			await waitForRunning(12_000);
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to start the collector.";
		} finally {
			installing = false;
		}
	}

	async function waitForRunning(timeoutMs: number): Promise<boolean> {
		const deadline = Date.now() + timeoutMs;
		while (Date.now() < deadline) {
			const s = await getCollectorStatus();
			if (s?.running) {
				status = s;
				return true;
			}
			await new Promise((r) => setTimeout(r, 1000));
		}
		return false;
	}

	async function togglePause() {
		toggling = true;
		error = null;
		try {
			if (status?.paused) {
				await resumeCollector();
			} else {
				await pauseCollector();
			}
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to toggle collection.";
		} finally {
			toggling = false;
		}
	}

	let disconnectArmed = $state(false);
	let disconnecting = $state(false);
	async function disconnectThisMac() {
		disconnecting = true;
		await forgetPairing();
		await restartApp();
	}
</script>

<Page
	title="This Mac"
	description={`${thisComputerLabel} — what this computer remembers, kept on your box.`}
	maxWidth="prose"
>
	<!--
		This is `devices/this` now, reached by clicking a row in the Devices
		list — so it needs the way back that every other device page has. It was
		a top-level Settings section until 2026-08-17 and had no use for one.
	-->
	{#snippet actions()}
		<Button variant="ghost" onclick={backToDevices}>
			<Icon icon="ri:arrow-left-line" />
			Devices
		</Button>
	{/snippet}

	{#if !isTauri || !isMacOS}
		<!-- Browser / non-Mac: can't drive a local daemon. -->
		<div class="rounded-lg border border-border bg-surface p-4">
			<p class="text-base text-foreground mb-1">Open the Virtues desktop app</p>
			<p class="text-sm text-foreground-muted">
				Run the <strong>Virtues app on your Mac</strong> to let it collect the docs you open,
				the people you message, and your on-screen activity — all stored on your box.
			</p>
		</div>
	{:else if !status && loading}
		<!-- First read in flight. Don't render this as "off" — that's how a
		     running collector wrongly showed "isn't collecting yet." -->
		<div class="text-sm text-foreground-muted">Checking this Mac…</div>
	{:else if !status && probeError}
		<!-- The daemon was asked and couldn't answer. Say so and offer the two
		     things that fix it, rather than waiting on a read that has already
		     failed and will keep failing. -->
		<div class="rounded-lg border border-border bg-surface p-4 space-y-3">
			<div>
				<p class="text-base text-foreground mb-1">Can't read this Mac's collector</p>
				<p class="text-sm text-foreground-muted">
					The collector is installed but its status couldn't be read, so what this Mac is
					sending can't be shown. Setting it up again usually clears this.
				</p>
				<p class="text-xs text-foreground-subtle font-mono mt-2 break-words">{probeError}</p>
			</div>
			{#if error}
				<p class="text-sm text-error">{error}</p>
			{/if}
			<div class="flex items-center gap-2">
				<Button variant="primary" onclick={turnOn} disabled={installing}>
					{installing ? "Starting…" : "Set up this Mac"}
				</Button>
				<Button variant="secondary" onclick={() => void refresh()}>Retry</Button>
			</div>
		</div>
	{:else if !status}
		<!-- Neither loading nor a recorded failure: only reachable if a read
		     resolved to nothing at all. -->
		<div class="text-sm text-foreground-muted">Checking this Mac…</div>
	{:else if !status.running}
		<!-- Collector not running: offer to set it up. -->
		<div class="rounded-lg border border-border bg-surface p-4 space-y-3">
			<div>
				<p class="text-base text-foreground mb-1">This Mac isn't collecting yet</p>
				<p class="text-sm text-foreground-muted">
					Turn it on to start remembering what happens on this computer.
				</p>
			</div>
			<Button variant="primary" onclick={turnOn} disabled={installing}>
				{installing ? "Starting…" : "Turn on this Mac"}
			</Button>
		</div>
	{:else}
		<!-- ── Collector ─────────────────────────────────────────────── -->
		<div class="text-xs font-medium uppercase tracking-wide text-foreground-subtle mb-2">
			Collector
		</div>
		<div class="rounded-lg border border-border bg-surface p-4 mb-6">
			<div class="flex items-center gap-3">
				<span
					class={`w-2.5 h-2.5 rounded-full flex-none ${
						status.paused ? "bg-warning" : "bg-success"
					}`}
				></span>
				<div class="flex-1 min-w-0">
					<div class="text-sm font-medium text-foreground">
						{status.paused ? "Paused" : "Collecting"}
					</div>
					<div class="text-xs text-foreground-muted mt-0.5">
						{#if queued > 0}
							{queued} queued{status.lastSync ? ` · synced ${status.lastSync}` : ""}
						{:else if status.lastSync}
							Synced {status.lastSync}
						{:else}
							Uploaded to your box over your private link
						{/if}
					</div>
				</div>
				<Button variant="secondary" onclick={togglePause} disabled={toggling}>
					{toggling ? "…" : status.paused ? "Resume" : "Pause"}
				</Button>
			</div>
		</div>

		<!-- ── Permissions ───────────────────────────────────────────── -->
		<div class="text-xs font-medium uppercase tracking-wide text-foreground-subtle mb-2">
			Permissions
		</div>
		{#if !status.permissionsReportedByDaemon}
			<!-- These ticks describe whichever process answered, and macOS grants
			     access per process — so without the daemon's own report they can
			     say "granted" while the daemon is being denied and collection is
			     silently dead. Say so rather than showing a confident wrong tick. -->
			<div
				class="mb-2 flex items-start gap-2 rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs"
			>
				<Icon icon="ri:information-line" width="16" class="text-warning shrink-0 mt-0.5" />
				<div>
					<span class="text-foreground">Unverified.</span>
					<span class="text-foreground-muted">
						The collector hasn't reported its own access, so the state below may not match what
						it can actually read.
						{#if status.permissionsCheckedAt}
							It last reported {new Date(status.permissionsCheckedAt).toLocaleString()}.
						{:else}
							It has never reported.
						{/if}
					</span>
				</div>
			</div>
		{/if}
		<ul class="rounded-lg border border-border bg-surface divide-y divide-border mb-6">
			<!-- Full Disk Access — required for Messages. -->
			<li class="p-4 flex items-start gap-3">
				<Icon
					icon={status.hasFullDiskAccess ? "ri:checkbox-circle-fill" : "ri:error-warning-line"}
					width="18"
					class={`shrink-0 mt-0.5 ${status.hasFullDiskAccess ? "text-success" : "text-warning"}`}
				/>
				<div class="flex-1 min-w-0">
					<div class="text-sm text-foreground">Full Disk Access</div>
					<div class="text-xs text-foreground-muted mt-0.5">
						Lets Virtues read Messages locally — never sent to us.
					</div>
					{#if !status.hasFullDiskAccess}
						<button
							class="text-xs text-primary hover:underline mt-1.5 block"
							onclick={() => openFullDiskAccess()}
						>
							Open Full Disk Access → turn on Virtues Collector
						</button>
						<span class="text-xs text-foreground-subtle mt-0.5 block">
							Not listed? Click <strong>+</strong> and add
							<code>~/.virtues/bin/virtues-collector</code>.
						</span>
					{/if}
				</div>
				{#if status.hasFullDiskAccess}
					<span class="text-xs text-success flex-none">On</span>
				{/if}
			</li>

			<!-- Accessibility — on-screen context (window titles / active tab). -->
			<li class="p-4 flex items-start gap-3">
				<Icon
					icon={status.hasAccessibility ? "ri:checkbox-circle-fill" : "ri:error-warning-line"}
					width="18"
					class={`shrink-0 mt-0.5 ${status.hasAccessibility ? "text-success" : "text-warning"}`}
				/>
				<div class="flex-1 min-w-0">
					<div class="text-sm text-foreground">Accessibility</div>
					<div class="text-xs text-foreground-muted mt-0.5">
						What's on your screen — window titles and the active browser tab.
					</div>
					{#if !status.hasAccessibility}
						<button
							class="text-xs text-primary hover:underline mt-1.5 block"
							onclick={() => openAccessibilitySettings()}
						>
							Open Accessibility → turn on Virtues Collector
						</button>
					{/if}
				</div>
				{#if status.hasAccessibility}
					<span class="text-xs text-success flex-none">On</span>
				{/if}
			</li>
		</ul>

		<!-- ── Streams ───────────────────────────────────────────────── -->
		<div class="text-xs font-medium uppercase tracking-wide text-foreground-subtle mb-2">
			Streams
		</div>
		<ul class="rounded-lg border border-border bg-surface divide-y divide-border mb-6">
			<!-- App usage — works without any special permission. -->
			<li class="p-4 flex items-center gap-3">
				<Icon icon="ri:apps-2-line" width="18" class="text-foreground-muted flex-none" />
				<div class="flex-1 min-w-0">
					<div class="text-sm text-foreground">App activity</div>
					<div class="text-xs text-foreground-muted mt-0.5">
						Which apps you use, and for how long.
					</div>
				</div>
				<span class="text-xs {status.paused ? 'text-foreground-subtle' : 'text-success'} flex-none">
					{status.paused ? "Paused" : "On"}
				</span>
			</li>

			<!-- Messages — gated on Full Disk Access. -->
			<li class="p-4 flex items-center gap-3">
				<Icon icon="ri:message-3-line" width="18" class="text-foreground-muted flex-none" />
				<div class="flex-1 min-w-0">
					<div class="text-sm text-foreground">Messages</div>
					<div class="text-xs text-foreground-muted mt-0.5">
						Your iMessage history, read locally.
					</div>
				</div>
				{#if status.hasFullDiskAccess}
					<span class="text-xs {status.paused ? 'text-foreground-subtle' : 'text-success'} flex-none">
						{status.paused ? "Paused" : "On"}
					</span>
				{:else}
					<span class="text-xs text-warning flex-none">Needs Full Disk Access</span>
				{/if}
			</li>
		</ul>

		{#if error}
			<div class="p-3 bg-error-subtle border border-error rounded-lg mb-6">
				<p class="text-sm text-error">{error}</p>
			</div>
		{/if}

		<!-- ── Disconnect ────────────────────────────────────────────── -->
		<div class="rounded-lg border border-border bg-surface p-4">
			{#if !disconnectArmed}
				<button
					class="text-sm font-medium text-error hover:underline"
					onclick={() => (disconnectArmed = true)}
				>
					Disconnect this Mac
				</button>
				<p class="text-xs text-foreground-muted mt-1">
					Clears this Mac's pairing with your box. Your data on the box is untouched.
				</p>
			{:else}
				<p class="text-sm text-foreground mb-2">
					Disconnect this Mac? You'll need to pair again to reconnect.
				</p>
				<div class="flex items-center gap-2">
					<Button variant="danger" onclick={disconnectThisMac} disabled={disconnecting}>
						{disconnecting ? "Disconnecting…" : "Disconnect"}
					</Button>
					<Button variant="secondary" onclick={() => (disconnectArmed = false)} disabled={disconnecting}>
						Cancel
					</Button>
				</div>
			{/if}
		</div>
	{/if}
</Page>
