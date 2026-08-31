<!--
  Tier 0 — "This device" (macOS collector setup).

  The collector↔web bridge already exists end-to-end (Tauri commands in
  apps/web/src-tauri + bridge.ts); this is the missing UI that drives it.

  Choreography (see project_onboarding_tiers doctrine):
    - Mint a one-time pair token (/api/pair/mint-collector) and hand it to
      installCollector(token) — the collector redeems it declaring source="mac".
    - POLL TRUTH via getCollectorStatus(); permission rows go green only when
      the daemon reports them granted — never trust the click.
    - Full Disk Access / Accessibility have NO prompt — deep-link to the
      System Settings pane; the row auto-advances when the toggle flips.
    - DONE = daemon running AND Full Disk Access granted (Messages is the
      marquee Mac data). Accessibility stays optional/amber.
    - Browser (non-Tauri): can't drive a local daemon — nudge the desktop app.
-->
<script lang="ts">
	import Icon from "$lib/components/Icon.svelte";
	import { Button } from "$lib";
	import { isTauri } from "$lib/utils/platform";
	import * as api from "$lib/api/client";
	import {
		getCollectorStatus,
		installCollector,
		openFullDiskAccess,
		openAccessibilitySettings,
		type CollectorStatus,
	} from "$lib/tauri/bridge";
	import { onDestroy, onMount } from "svelte";

	interface Props {
		/** Fired once the daemon is running AND Full Disk Access is granted. */
		onComplete?: () => void;
	}
	let { onComplete }: Props = $props();

	let status = $state<CollectorStatus | null>(null);
	let installing = $state(false);
	let error = $state<string | null>(null);
	let pollTimer: ReturnType<typeof setInterval> | null = null;
	let completed = $state(false);

	// DONE gate: daemon running + BOTH permissions. Full Disk Access (Messages,
	// Mail, etc.) and Accessibility (on-screen context) are both load-bearing
	// for the collector — neither is optional.
	const isDone = $derived(
		!!status?.running && !!status?.hasFullDiskAccess && !!status?.hasAccessibility,
	);

	$effect(() => {
		if (isDone && !completed) {
			completed = true;
			onComplete?.();
		}
	});

	onMount(() => {
		if (isTauri) {
			void refresh();
			// Poll truth ~2s while the card is active (cheap local IPC).
			pollTimer = setInterval(refresh, 2000);
		}
	});
	onDestroy(() => {
		if (pollTimer) clearInterval(pollTimer);
	});

	async function refresh() {
		const s = await getCollectorStatus();
		if (s) status = s;
	}

	async function turnOn() {
		installing = true;
		error = null;
		try {
			const { token } = await api.mintCollectorToken();
			// Throws with the daemon's real stderr if the install itself fails.
			await installCollector(token);
			// Install can return OK while `launchctl bootstrap` silently fails,
			// so don't assume success — wait for the daemon to actually report
			// running, and only then call it done.
			const started = await waitForRunning(12_000);
			await refresh();
			if (!started) {
				error =
					"Installed, but the collector didn't start. Check ~/.virtues/logs/collector.error.log, then try again.";
			}
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to start the collector.";
		} finally {
			installing = false;
		}
	}

	// Poll the daemon's own status until it reports running (or we give up).
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
</script>

{#if !isTauri}
	<!-- Browser can't drive a local daemon. -->
	<div class="rounded-lg border border-border p-4">
		<p class="font-serif text-base text-foreground mb-1">Set up this Mac</p>
		<p class="text-sm text-foreground-muted">
			Open the <strong>Virtues desktop app</strong> on this Mac to let it remember
			what happens here — the docs you open, the people you message, your calendar.
			It all stays on your server.
		</p>
	</div>
{:else}
	<div class="rounded-lg border border-border p-4 space-y-4">
		{#if !status?.running}
			<!-- Step 1: install + pair the daemon. (The step subtitle already says
			     what this does — no second paragraph here.) -->
			<Button variant="primary" onclick={turnOn} disabled={installing}>
				{installing ? "Starting…" : "Turn on this Mac"}
			</Button>
		{:else}
			<!-- Step 2: permission rows, truth-polled. -->
			<ul class="space-y-2">
				<!-- Daemon + init sync (counts ticking, never a dead spinner). -->
				<li class="flex items-start gap-2 text-sm">
					<Icon icon="ri:checkbox-circle-fill" width="18" class="text-success shrink-0 mt-0.5" />
					<div>
						<span class="text-foreground">Collecting</span>
						{#if status.pendingMessages > 0 || status.pendingEvents > 0}
							<span class="text-foreground-subtle">
								· indexing {status.pendingMessages + status.pendingEvents} items…
							</span>
						{:else if status.lastSync}
							<span class="text-foreground-subtle">· synced {status.lastSync}</span>
						{/if}
					</div>
				</li>

				<!-- Full Disk Access — REQUIRED (Messages). Settings-pane grant. -->
				<li class="flex items-start gap-2 text-sm">
					<Icon
						icon={status.hasFullDiskAccess ? "ri:checkbox-circle-fill" : "ri:error-warning-line"}
						width="18"
						class={`shrink-0 mt-0.5 ${status.hasFullDiskAccess ? "text-success" : "text-warning"}`}
					/>
					<div class="flex-1">
						<span class="text-foreground">Full Disk Access</span>
						<span class="text-foreground-subtle">— Messages, read locally, never sent to Virtues</span>
						{#if !status.hasFullDiskAccess}
							<button
								class="text-xs text-primary hover:underline mt-1 block"
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
				</li>

				<!-- Accessibility — REQUIRED (on-screen context). -->
				<li class="flex items-start gap-2 text-sm">
					<Icon
						icon={status.hasAccessibility ? "ri:checkbox-circle-fill" : "ri:error-warning-line"}
						width="18"
						class={`shrink-0 mt-0.5 ${status.hasAccessibility ? "text-success" : "text-warning"}`}
					/>
					<div class="flex-1">
						<span class="text-foreground">Accessibility</span>
						<span class="text-foreground-subtle">— what's on your screen, stays on your server</span>
						{#if !status.hasAccessibility}
							<button
								class="text-xs text-primary hover:underline mt-1 block"
								onclick={() => openAccessibilitySettings()}
							>
								Open Accessibility → turn on Virtues Collector
							</button>
						{/if}
					</div>
				</li>
			</ul>

			{#if isDone}
				<p class="text-sm text-success">This Mac is collecting.</p>
			{:else}
				<p class="text-xs text-foreground-muted">
					Grant both Full Disk Access and Accessibility to finish.
				</p>
			{/if}
		{/if}

		{#if error}
			<div class="p-3 bg-error-subtle border border-error rounded-lg">
				<p class="text-sm text-error">{error}</p>
			</div>
		{/if}
	</div>
{/if}
