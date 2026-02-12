<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { spaceStore } from "$lib/stores/space.svelte";
	import { Page, Button, Badge } from "$lib";
	import { formatTimeAgo } from "$lib/utils/dateUtils";
	import type { Job } from "$lib/api/client";
	import { cancelJob } from "$lib/api/client";
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let jobs = $state<Job[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(async () => {
		await loadData();
	});

	async function loadData() {
		loading = true;
		error = null;
		try {
			const res = await fetch("/api/jobs");
			if (!res.ok) throw new Error("Failed to load jobs");
			jobs = await res.json();
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to load jobs";
		} finally {
			loading = false;
		}
	}

	function formatRelativeTime(timestamp: string): string {
		return formatTimeAgo(timestamp);
	}

	function getStatusVariant(
		status: Job["status"],
	): "success" | "error" | "warning" | "primary" | "muted" {
		const variants: Record<
			Job["status"],
			"success" | "error" | "warning" | "primary" | "muted"
		> = {
			succeeded: "success",
			failed: "error",
			cancelled: "warning",
			running: "primary",
			pending: "muted",
		};
		return variants[status] || "muted";
	}

	function getJobTypeLabel(
		jobType: Job["job_type"],
		streamName?: string,
	): string {
		const baseLabel = jobType === "sync" ? "Sync" : "Transform";
		return streamName ? `${baseLabel} · ${streamName}` : baseLabel;
	}

	function calculateDuration(
		startedAt: string,
		completedAt?: string,
	): string {
		const start = new Date(startedAt).getTime();
		const end = completedAt ? new Date(completedAt).getTime() : Date.now();
		const durationMs = end - start;

		const seconds = Math.floor(durationMs / 1000);
		const minutes = Math.floor(seconds / 60);
		const hours = Math.floor(minutes / 60);

		if (hours > 0) return `${hours}h ${minutes % 60}m`;
		if (minutes > 0) return `${minutes}m ${seconds % 60}s`;
		return `${seconds}s`;
	}

	async function handleCancelJob(jobId: string) {
		if (!confirm("Are you sure you want to cancel this job?")) return;

		try {
			await cancelJob(jobId);
			await loadData();
		} catch (err) {
			alert(err instanceof Error ? err.message : "Failed to cancel job");
		}
	}

	function formatNumber(num: number): string {
		if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
		if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
		return num.toLocaleString();
	}

	const hasActiveJobs = $derived(
		jobs.some((j) => j.status === "pending" || j.status === "running"),
	);

	const jobStats = $derived({
		total: jobs.length,
		succeeded: jobs.filter((j) => j.status === "succeeded").length,
		failed: jobs.filter((j) => j.status === "failed").length,
		active: jobs.filter(
			(j) => j.status === "pending" || j.status === "running",
		).length,
		cancelled: jobs.filter((j) => j.status === "cancelled").length,
		recordsProcessed: jobs.reduce(
			(sum, j) => sum + (j.records_processed || 0),
			0,
		),
	});

	const successRate = $derived(
		jobStats.total > 0
			? Math.round(
					(jobStats.succeeded /
						(jobStats.succeeded + jobStats.failed)) *
						100,
				) || 0
			: 0,
	);

	const successRateColor = $derived(
		successRate >= 95
			? "text-success"
			: successRate >= 80
				? "text-warning"
				: "text-error",
	);

	function handleSourceClick(sourceId: string) {
		spaceStore.openTabFromRoute(`/sources/${sourceId}`);
	}
</script>

<Page>
	<div class="max-w-7xl">
		<div class="mb-8 flex items-center justify-between">
			<div>
				<h1
					class="text-3xl font-serif font-medium text-foreground mb-2"
				>
					Activity
				</h1>
				<p class="text-foreground-muted">
					System activity including syncs, transformations, and
					configuration changes
				</p>
			</div>
			<Button onclick={loadData} variant="secondary">
				<Icon icon="ri:refresh-line" class="text-lg" />
				Refresh
			</Button>
		</div>

		{#if loading}
			<div class="flex items-center justify-center h-full">
				<Icon icon="ri:loader-4-line" width="20" class="spin" />
			</div>
		{:else if error}
			<div
				class="p-4 bg-error-subtle border border-error rounded-lg text-error"
			>
				{error}
			</div>
		{:else if jobs.length === 0}
			<div
				class="border-2 border-dashed border-border rounded-lg p-12 text-center bg-surface-elevated"
			>
				<Icon
					icon="ri:history-line"
					class="text-6xl text-foreground-subtle mb-4"
				/>
				<h3 class="text-lg font-medium text-foreground mb-2">
					No activity yet
				</h3>
				<p class="text-foreground-muted">
					System activity will appear here once your sources start
					syncing
				</p>
			</div>
		{:else}
			<div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
				<div class="bg-surface border border-border rounded-lg p-4">
					<div
						class="text-xs font-medium text-foreground-subtle uppercase tracking-wide mb-1"
					>
						Success Rate
					</div>
					<div class="flex items-baseline gap-1">
						<span class="text-2xl font-semibold {successRateColor}"
							>{successRate}%</span
						>
						{#if jobStats.failed > 0}
							<span class="text-xs text-foreground-subtle"
								>({jobStats.failed} failed)</span
							>
						{/if}
					</div>
				</div>

				<div class="bg-surface border border-border rounded-lg p-4">
					<div
						class="text-xs font-medium text-foreground-subtle uppercase tracking-wide mb-1"
					>
						Jobs Completed
					</div>
					<div class="flex items-baseline gap-1">
						<span class="text-2xl font-semibold text-foreground"
							>{jobStats.succeeded}</span
						>
						<span class="text-sm text-foreground-subtle"
							>/ {jobStats.total}</span
						>
					</div>
				</div>

				<div class="bg-surface border border-border rounded-lg p-4">
					<div
						class="text-xs font-medium text-foreground-subtle uppercase tracking-wide mb-1"
					>
						Records Processed
					</div>
					<div class="text-2xl font-semibold text-foreground">
						{formatNumber(jobStats.recordsProcessed)}
					</div>
				</div>

				<div class="bg-surface border border-border rounded-lg p-4">
					<div
						class="text-xs font-medium text-foreground-subtle uppercase tracking-wide mb-1"
					>
						Active Jobs
					</div>
					<div class="flex items-baseline gap-2">
						<span
							class="text-2xl font-semibold {jobStats.active > 0
								? 'text-primary'
								: 'text-foreground'}"
						>
							{jobStats.active}
						</span>
						{#if jobStats.active > 0}
							<Icon
								icon="ri:loader-4-line"
								class="text-primary animate-spin"
							/>
						{/if}
					</div>
				</div>
			</div>

			<div class="border border-border rounded-lg overflow-hidden">
				<table class="w-full">
					<thead class="bg-surface-elevated border-b border-border">
						<tr>
							<th
								class="px-6 py-4 text-left text-xs font-medium text-foreground-subtle uppercase"
								>Type</th
							>
							<th
								class="px-6 py-4 text-left text-xs font-medium text-foreground-subtle uppercase"
								>Source</th
							>
							<th
								class="px-6 py-4 text-left text-xs font-medium text-foreground-subtle uppercase"
								>Status</th
							>
							<th
								class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase"
								>Duration</th
							>
							<th
								class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase"
								>Records</th
							>
							<th
								class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase"
								>Time</th
							>
							<th
								class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase"
								>Actions</th
							>
						</tr>
					</thead>
					<tbody class="divide-y divide-border">
						{#each jobs as job}
							<tr
								class="hover:bg-surface-elevated transition-colors"
							>
								<td class="px-6 py-4 whitespace-nowrap">
									<span class="text-sm text-foreground">
										{getJobTypeLabel(
											job.job_type,
											job.stream_name || undefined,
										)}
									</span>
								</td>
								<td class="px-6 py-4 whitespace-nowrap">
									{#if job.source_id}
										<button
											onclick={() =>
												handleSourceClick(
													job.source_id!,
												)}
											class="text-sm text-foreground-muted hover:text-foreground hover:underline"
										>
											{job.source_name || job.source_id}
										</button>
									{:else}
										<span
											class="text-sm text-foreground-subtle"
											>—</span
										>
									{/if}
								</td>
								<td class="px-6 py-4 whitespace-nowrap">
									<Badge
										variant={getStatusVariant(job.status)}
										outline
										class="capitalize"
									>
										{#if job.status === "failed" && job.error_message}
											<Icon
												icon="ri:error-warning-line"
												class="text-xs"
											/>
										{/if}
										{#if job.status === "running"}
											<Icon
												icon="ri:loader-4-line"
												class="text-xs animate-spin"
											/>
										{/if}
										{job.status.replace("_", " ")}
									</Badge>
								</td>
								<td
									class="px-6 py-4 whitespace-nowrap text-right"
								>
									<span class="text-sm text-foreground-muted">
										{calculateDuration(
											job.started_at,
											job.completed_at,
										)}
									</span>
								</td>
								<td
									class="px-6 py-4 whitespace-nowrap text-right"
								>
									{#if job.records_processed > 0}
										<span
											class="text-sm text-foreground-muted"
										>
											{job.records_processed.toLocaleString()}
										</span>
									{:else}
										<span
											class="text-sm text-foreground-subtle"
											>—</span
										>
									{/if}
								</td>
								<td
									class="px-6 py-4 whitespace-nowrap text-right"
								>
									<span class="text-sm text-foreground-muted">
										{formatRelativeTime(job.started_at)}
									</span>
								</td>
								<td
									class="px-6 py-4 whitespace-nowrap text-right"
								>
									<div
										class="flex items-center justify-end gap-2"
									>
										{#if job.status === "pending" || job.status === "running"}
											<button
												onclick={() =>
													handleCancelJob(job.id)}
												class="text-xs text-error hover:text-error/80 hover:underline"
											>
												Cancel
											</button>
										{/if}
										{#if job.status === "failed" && job.error_message}
											<button
												onclick={() =>
													alert(
														`Error: ${job.error_message}`,
													)}
												class="text-xs text-foreground-muted hover:text-foreground hover:underline"
												title={job.error_message}
											>
												View Error
											</button>
										{/if}
									</div>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>

			<div class="mt-4 flex items-center justify-between">
				<div class="text-sm text-foreground-muted">
					Showing {jobs.length} job{jobs.length !== 1 ? "s" : ""}
					{#if hasActiveJobs}
						<span class="ml-2 text-primary">
							· {jobs.filter(
								(j) =>
									j.status === "pending" ||
									j.status === "running",
							).length} active
						</span>
					{/if}
				</div>
			</div>
		{/if}
	</div>
</Page>
