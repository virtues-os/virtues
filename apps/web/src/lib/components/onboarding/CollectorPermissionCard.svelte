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

	// DONE gate (locked): daemon running + Full Disk Access. Accessibility is a
	// bonus, not a blocker.
	const isDone = $derived(!!status?.running && !!status?.hasFullDiskAccess);

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
			const ok = await installCollector(token);
			if (!ok) throw new Error("The collector failed to install.");
			await refresh();
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to start the collector.";
		} finally {
			installing = false;
		}
	}
</script>

{#if !isTauri}
	<!-- Browser can't drive a local daemon. -->
	<div class="rounded-lg border border-border p-4">
		<p class="font-serif text-base text-foreground mb-1">Set up this Mac</p>
		<p class="text-sm text-foreground-muted">
			Open the <strong>Virtues desktop app</strong> on this Mac to let it remember
			what happens here — the docs you open, the people you message, your calendar.
			It all stays on your box.
		</p>
	</div>
{:else}
	<div class="rounded-lg border border-border p-4 space-y-4">
		<div>
			<p class="font-serif text-base text-foreground mb-1">This Mac</p>
			<p class="text-sm text-foreground-muted">
				Virtues remembers what happens on this machine so you can ask your box
				about your own life later. It runs in the background and never leaves
				this Mac except to sync to your box.
			</p>
		</div>

		{#if !status?.running}
			<!-- Step 1: install + pair the daemon. -->
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
						<span class="text-foreground-subtle">— read your Messages history</span>
						{#if !status.hasFullDiskAccess}
							<p class="text-xs text-foreground-muted mt-1">
								macOS only allows this from System Settings. Flip <strong>Virtues</strong>
								to on, then come back — this updates on its own.
							</p>
							<button
								class="text-xs text-primary hover:underline mt-1"
								onclick={() => openFullDiskAccess()}
							>
								Open System Settings
							</button>
						{/if}
					</div>
				</li>

				<!-- Accessibility — optional, never blocks. -->
				<li class="flex items-start gap-2 text-sm">
					<Icon
						icon={status.hasAccessibility ? "ri:checkbox-circle-fill" : "ri:checkbox-blank-circle-line"}
						width="18"
						class={`shrink-0 mt-0.5 ${status.hasAccessibility ? "text-success" : "text-foreground-subtle"}`}
					/>
					<div class="flex-1">
						<span class="text-foreground">Accessibility</span>
						<span class="text-foreground-subtle">— see what's on your screen (optional)</span>
						{#if !status.hasAccessibility}
							<button
								class="text-xs text-primary hover:underline mt-1 block"
								onclick={() => openAccessibilitySettings()}
							>
								Open System Settings
							</button>
						{/if}
					</div>
				</li>
			</ul>

			{#if isDone}
				<p class="text-sm text-success">This Mac is collecting. You can move on — add your phone next.</p>
			{:else}
				<p class="text-xs text-foreground-muted">
					Grant Full Disk Access to finish — everything else is optional.
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
