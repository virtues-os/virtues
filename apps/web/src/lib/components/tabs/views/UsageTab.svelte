<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { formatMicrosUSD, formatMicrosPrecise } from "$lib/utils/currency";
	import { onMount } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// ─── Wallet headline (cloud billing state, proxied from virtues-api) ──────
	type Wallet = {
		balance_micros: number;
		month_to_date_micros: number;
		expires_at: string | null;
		error?: string;
	};
	// ─── Box-local spend breakdown (app_ai_calls) ─────────────────────────────
	type Bucket = {
		label: string;
		calls: number;
		prompt_tokens: number;
		completion_tokens: number;
		cost_micros: number;
	};
	type Summary = { month_start: string; by_feature: Bucket[]; by_model: Bucket[] };

	let wallet = $state<Wallet | null>(null);
	let summary = $state<Summary | null>(null);
	let loading = $state(true);

	onMount(load);

	async function load() {
		loading = true;
		const [w, s] = await Promise.allSettled([
			fetch("/api/billing/usage").then((r) => r.json()),
			fetch("/api/usage/summary").then((r) => r.json()),
		]);
		if (w.status === "fulfilled" && !w.value.error) wallet = w.value;
		if (s.status === "fulfilled") summary = s.value;
		loading = false;
	}

	// Friendly labels for the coarse feature buckets.
	const featureLabel: Record<string, string> = {
		chat: "Chat & assistant",
		transcription: "Audio transcription",
		agent: "Background agents",
		search: "Search",
		embedding: "Embeddings",
		other: "Other",
	};
	const featureIcon: Record<string, string> = {
		chat: "ri:chat-3-line",
		transcription: "ri:mic-line",
		agent: "ri:robot-line",
		search: "ri:search-line",
		embedding: "ri:node-tree",
		other: "ri:more-line",
	};

	const totalSpent = $derived(
		(summary?.by_feature ?? []).reduce((a, b) => a + b.cost_micros, 0),
	);
	const renewsLabel = $derived(
		wallet?.expires_at
			? new Date(wallet.expires_at).toLocaleDateString(undefined, {
					month: "long",
					day: "numeric",
				})
			: null,
	);
	function pct(part: number): number {
		return totalSpent > 0 ? Math.round((part / totalSpent) * 100) : 0;
	}
</script>

<Page
	title="Usage"
	description="What your Virtues wallet paid for this month — all figures stay on your box."
	maxWidth="full"
>
	{#if loading}
		<div class="flex items-center justify-center h-40">
			<Icon icon="ri:loader-4-line" width="20" class="spin" />
		</div>
	{:else}
		<!-- Wallet headline -->
		{#if wallet}
			<div class="border border-border rounded-lg p-6 mb-6">
				<div class="flex items-baseline justify-between mb-1">
					<h2 class="text-lg font-medium text-foreground">Balance</h2>
					{#if renewsLabel}
						<span class="text-xs text-foreground-muted">Renews {renewsLabel}</span>
					{/if}
				</div>
				<div class="flex items-baseline gap-2 mb-4">
					<span class="text-3xl font-semibold text-foreground tabular-nums"
						>{formatMicrosUSD(wallet.balance_micros)}</span
					>
					<span class="text-foreground-muted text-sm">available</span>
				</div>
				<div class="flex justify-between text-sm">
					<span class="text-foreground-muted">Spent this month</span>
					<span class="text-foreground tabular-nums"
						>{formatMicrosUSD(wallet.month_to_date_micros)}</span
					>
				</div>
			</div>
		{:else}
			<div class="border border-border rounded-lg p-6 mb-6 text-foreground-muted text-sm">
				Connect your subscription to see your balance.
			</div>
		{/if}

		<!-- Where the money went, by feature -->
		<div class="border border-border rounded-lg p-6 mb-6">
			<div class="text-xs uppercase tracking-wide text-foreground-muted mb-4">
				Where your money went
			</div>
			{#if (summary?.by_feature ?? []).length === 0}
				<div class="text-sm text-foreground-subtle">No usage recorded yet this month.</div>
			{:else}
				<div class="space-y-4">
					{#each summary?.by_feature ?? [] as b (b.label)}
						<div>
							<div class="flex items-center justify-between text-sm mb-1.5">
								<span class="flex items-center gap-2 text-foreground">
									<Icon
										icon={featureIcon[b.label] ?? "ri:more-line"}
										class="text-foreground-subtle"
									/>
									{featureLabel[b.label] ?? b.label}
									<span class="text-foreground-subtle text-xs">· {b.calls} calls</span>
								</span>
								<span class="text-foreground tabular-nums">{formatMicrosPrecise(b.cost_micros)}</span>
							</div>
							<div class="h-1.5 w-full rounded-full bg-surface-elevated overflow-hidden">
								<div
									class="h-full rounded-full bg-foreground transition-all duration-500"
									style="width: {Math.max(pct(b.cost_micros), b.cost_micros > 0 ? 2 : 0)}%"
								></div>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</div>

		<!-- By model -->
		{#if (summary?.by_model ?? []).length > 0}
			<div class="border border-border rounded-lg p-6">
				<div class="text-xs uppercase tracking-wide text-foreground-muted mb-3">By model</div>
				<div class="divide-y divide-border-subtle">
					{#each summary?.by_model ?? [] as m (m.label)}
						<div class="flex items-center justify-between py-2 text-sm">
							<span class="text-foreground font-mono text-xs">{m.label}</span>
							<span class="flex items-center gap-3">
								<span class="text-foreground-subtle text-xs tabular-nums"
									>{m.calls} calls</span
								>
								<span class="text-foreground tabular-nums">{formatMicrosPrecise(m.cost_micros)}</span>
							</span>
						</div>
					{/each}
				</div>
			</div>
		{/if}
	{/if}
</Page>
