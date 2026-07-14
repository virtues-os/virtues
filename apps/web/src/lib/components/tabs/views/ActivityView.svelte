<!--
  Auth activity log — the append-only record of pairings, revocations, sudo
  events. Most-recent-first. Surfaced so the user can spot anything they
  didn't do (an unfamiliar pair from a strange IP, a sudo request they didn't
  trigger). The backend serves at /api/audit/auth.
-->
<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page, EmptyState, LoadingState, ErrorState } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	type Event = {
		id: number;
		device_id: string | null;
		event_type: string;
		detail: Record<string, unknown>;
		ip: string | null;
		user_agent: string | null;
		occurred_at: string;
	};

	let events = $state<Event[]>([]);
	let loading = $state(true);
	let errorMessage = $state<string | null>(null);

	onMount(load);

	async function load() {
		loading = true;
		errorMessage = null;
		try {
			const resp = await fetch("/api/audit/auth");
			if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
			const data = await resp.json();
			events = data.events ?? [];
		} catch (e) {
			errorMessage = e instanceof Error ? e.message : "Failed to load activity";
		} finally {
			loading = false;
		}
	}

	function iconFor(t: string): string {
		if (t === "paired") return "ri:link";
		if (t === "revoked") return "ri:close-circle-line";
		if (t.startsWith("sudo_")) return "ri:shield-keyhole-line";
		if (t.startsWith("pair_token_")) return "ri:key-2-line";
		if (t === "session_started") return "ri:login-circle-line";
		if (t === "session_ended") return "ri:logout-circle-line";
		if (t === "idle_logout") return "ri:zzz-line";
		return "ri:history-line";
	}

	function humanType(t: string): string {
		return t.replace(/_/g, " ");
	}

	function timeAgo(iso: string): string {
		const then = new Date(iso).getTime();
		const sec = Math.max(0, Math.floor((Date.now() - then) / 1000));
		if (sec < 60) return "just now";
		const min = Math.floor(sec / 60);
		if (min < 60) return `${min}m ago`;
		const hr = Math.floor(min / 60);
		if (hr < 24) return `${hr}h ago`;
		const d = Math.floor(hr / 24);
		return `${d}d ago`;
	}
</script>

<Page
	title="Activity"
	description="Recent pairings, revocations, and sensitive-action confirmations on this box. If something here doesn't look like you, revoke the relevant device."
	maxWidth="prose"
>
	{#if loading}
		<LoadingState />
	{:else if errorMessage}
		<ErrorState message={errorMessage} />
	{:else if events.length === 0}
		<EmptyState
			icon="ri:history-line"
			title="No activity yet"
			message="Pair a device or run a sensitive action to see entries here."
		/>
	{:else}
		<ul class="divide-y divide-border rounded-lg border border-border bg-surface">
			{#each events as ev (ev.id)}
				<li class="p-3 flex items-start gap-3">
					<div
						class="flex-shrink-0 w-8 h-8 rounded-md bg-surface-alt border border-border flex items-center justify-center"
					>
						<Icon icon={iconFor(ev.event_type)} class="text-foreground-muted" />
					</div>
					<div class="flex-1 min-w-0">
						<div class="text-sm">
							<span class="font-medium text-foreground">{humanType(ev.event_type)}</span>
							<span class="text-foreground-muted">· {timeAgo(ev.occurred_at)}</span>
						</div>
						<div class="text-xs text-foreground-muted mt-0.5 flex flex-wrap gap-x-3 gap-y-0.5">
							{#if ev.ip}<span>IP: {ev.ip}</span>{/if}
							{#if ev.device_id}<span class="font-mono">{ev.device_id.slice(0, 12)}…</span>{/if}
						</div>
						{#if Object.keys(ev.detail).length > 0}
							<details class="text-xs mt-1">
								<summary class="cursor-pointer text-foreground-muted hover:text-foreground">
									Details
								</summary>
								<pre class="mt-1 p-2 bg-surface-alt rounded text-[10px] overflow-x-auto">
{JSON.stringify(ev.detail, null, 2)}
								</pre>
							</details>
						{/if}
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</Page>
