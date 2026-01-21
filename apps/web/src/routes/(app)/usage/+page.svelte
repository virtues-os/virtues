<script lang="ts">
	import { Page, Badge } from "$lib";
	import "iconify-icon";
	import type { PageData } from "./$types";

	export let data: PageData;

	function formatNumber(num: number): string {
		return num.toLocaleString();
	}

	function clampPercent(percentage: number): number {
		if (!Number.isFinite(percentage)) return 0;
		return Math.max(0, Math.min(percentage, 100));
	}

	function getProgressColor(percentage: number): string {
		if (percentage < 50) return "var(--color-success)";
		if (percentage < 75) return "var(--color-warning)";
		if (percentage < 90) return "var(--color-warning)";
		return "var(--color-error)";
	}

	function formatResetDate(dateStr: string | null): string {
		if (!dateStr) return "Next month";
		const date = new Date(dateStr);
		return date.toLocaleDateString("en-US", {
			month: "short",
			day: "numeric",
		});
	}

	const serviceNames: Record<string, string> = {
		ai_gateway: "AI Gateway",
		google_places: "Google Places",
		exa: "Web Search",
	};
</script>

<svelte:head>
	<title>Usage - Virtues</title>
</svelte:head>

<Page>
	<div class="max-w-7xl">
		<!-- Header -->
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">
				Usage
			</h1>
			<p class="text-foreground-muted">
				Monitor API usage, rate limits, and estimated costs
			</p>
		</div>

		<!-- Daily usage -->
		<div class="grid grid-cols-1 md:grid-cols-2 gap-4">
			<div class="bg-surface border border-border rounded-lg p-6">
				<div class="flex items-start justify-between gap-3 mb-4">
					<div>
						<div
							class="text-xs font-medium tracking-wide uppercase text-foreground-muted"
						>
							Daily requests
						</div>
						<div class="text-xs text-foreground-subtle mt-1">
							Resets at midnight UTC
						</div>
					</div>
					<iconify-icon
						icon="ri:swap-line"
						class="text-foreground-subtle text-lg"
					></iconify-icon>
				</div>

				<div class="flex items-baseline gap-2 mb-3">
					<div class="text-3xl font-medium text-foreground">
						{formatNumber(data.usage.daily.requests)}
					</div>
					<div class="text-sm text-foreground-subtle">
						/ {formatNumber(data.usage.daily.requestsLimit)}
					</div>
				</div>

				<div
					class="h-2 w-full bg-surface-elevated rounded-full overflow-hidden"
				>
					<div
						class="h-full"
						style="width: {clampPercent(
							data.usage.daily.requestsPercentage,
						)}%; background-color: {getProgressColor(
							data.usage.daily.requestsPercentage,
						)}"
					></div>
				</div>
				<div class="mt-2 text-xs text-foreground-muted">
					{data.usage.daily.requestsPercentage}% used
				</div>
			</div>

			<div class="bg-surface border border-border rounded-lg p-6">
				<div class="flex items-start justify-between gap-3 mb-4">
					<div>
						<div
							class="text-xs font-medium tracking-wide uppercase text-foreground-muted"
						>
							Daily tokens
						</div>
						<div class="text-xs text-foreground-subtle mt-1">
							Input + output tokens
						</div>
					</div>
					<iconify-icon
						icon="ri:cpu-line"
						class="text-foreground-subtle text-lg"
					></iconify-icon>
				</div>

				<div class="flex items-baseline gap-2 mb-3">
					<div class="text-3xl font-medium text-foreground">
						{formatNumber(data.usage.daily.tokens)}
					</div>
					<div class="text-sm text-foreground-subtle">
						/ {formatNumber(data.usage.daily.tokensLimit)}
					</div>
				</div>

				<div
					class="h-2 w-full bg-surface-elevated rounded-full overflow-hidden"
				>
					<div
						class="h-full"
						style="width: {clampPercent(
							data.usage.daily.tokensPercentage,
						)}%; background-color: {getProgressColor(
							data.usage.daily.tokensPercentage,
						)}"
					></div>
				</div>
				<div class="mt-2 text-xs text-foreground-muted">
					{data.usage.daily.tokensPercentage}% used
				</div>
			</div>
		</div>

		<!-- Monthly service usage -->
		<div class="mt-10">
			<div class="flex flex-wrap items-center gap-3 mb-4">
				<h2 class="text-xl font-serif font-medium text-foreground">
					Monthly service usage
				</h2>
				{#if data.monthly}
					<Badge>{data.monthly.tier}</Badge>
					<span class="text-sm text-foreground-subtle">
						Resets {formatResetDate(data.monthly.resetsAt)}
					</span>
				{/if}
			</div>

			{#if data.monthly?.services}
				<div
					class="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4"
				>
					{#each Object.entries(data.monthly.services) as [key, service]}
						<div
							class="bg-surface border border-border rounded-lg p-5"
						>
							<div
								class="flex items-start justify-between gap-3 mb-3"
							>
								<div>
									<div
										class="text-sm font-medium text-foreground"
									>
										{serviceNames[key] || key}
									</div>
									<div class="text-xs text-foreground-subtle">
										{service.unit}
									</div>
								</div>
								{#if service.limitType === "soft"}
									<Badge uppercase>Soft</Badge>
								{:else}
									<Badge uppercase>Hard</Badge>
								{/if}
							</div>

							<div class="flex items-baseline gap-2 mb-2">
								<div
									class="text-2xl font-medium text-foreground"
								>
									{formatNumber(service.used)}
								</div>
								<div class="text-sm text-foreground-subtle">
									/ {formatNumber(service.limit)}
								</div>
							</div>

							<div
								class="h-2 w-full bg-surface-elevated rounded-full overflow-hidden"
							>
								<div
									class="h-full"
									style="width: {clampPercent(
										service.percentage,
									)}%; background-color: {getProgressColor(
										service.percentage,
									)}"
								></div>
							</div>
							<div class="mt-2 text-xs text-foreground-muted">
								{service.percentage}% used
								{#if service.percentage > 100 && service.limitType === "soft"}
									<span class="text-warning ml-1"
										>(over budget)</span
									>
								{/if}
							</div>
						</div>
					{/each}
				</div>
			{:else}
				<div
					class="border border-dashed border-border rounded-lg p-6 bg-surface text-sm text-foreground-muted"
				>
					No monthly service usage available yet.
				</div>
			{/if}
		</div>

		<!-- Rate limits -->
		<div class="mt-10">
			<h2 class="text-xl font-serif font-medium text-foreground mb-4">
				Rate limits
			</h2>
			<div
				class="grid grid-cols-1 md:grid-cols-3 gap-4 bg-surface border border-border rounded-lg p-6"
			>
				<div class="space-y-1">
					<div class="text-sm text-foreground-muted">
						Chat requests (daily)
					</div>
					<div class="text-lg font-medium text-foreground">
						{formatNumber(data.usage.limits.chatRequestsPerDay)}
					</div>
				</div>
				<div class="space-y-1">
					<div class="text-sm text-foreground-muted">
						Tokens (daily)
					</div>
					<div class="text-lg font-medium text-foreground">
						{formatNumber(data.usage.limits.chatTokensPerDay)}
					</div>
				</div>
				<div class="space-y-1">
					<div class="text-sm text-foreground-muted">
						Background jobs (daily)
					</div>
					<div class="text-lg font-medium text-foreground">
						{formatNumber(data.usage.limits.backgroundJobsPerDay)}
					</div>
				</div>
			</div>
		</div>

		<!-- About -->
		<div class="mt-10">
			<h2 class="text-xl font-serif font-medium text-foreground mb-3">
				About rate limiting
			</h2>
			<div
				class="bg-surface border border-border rounded-lg p-6 space-y-3"
			>
				<p class="text-foreground-muted">
					These limits protect against excessive API costs and ensure
					fair usage. Limits are enforced per deployment.
				</p>
				<ul
					class="list-disc list-inside space-y-2 text-foreground-muted text-sm"
				>
					<li>Daily limits reset at midnight (UTC).</li>
					<li>Token limits include both input and output tokens.</li>
					<li>
						Cost estimates are based on current Claude and OpenAI
						pricing.
					</li>
				</ul>
			</div>
		</div>
	</div>
</Page>
