<script lang="ts">
	import { Page, Button, Badge } from "$lib";
	import type { Job } from "$lib/api/client";
	import { goto, invalidateAll } from "$app/navigation";
	import { cancelJob } from "$lib/api/client";
	import "iconify-icon";
	import type { PageData } from "./$types";

	export let data: PageData;

	$: jobs = data.jobs;

	function formatRelativeTime(timestamp: string): string {
		const date = new Date(timestamp);
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffMins = Math.floor(diffMs / 1000 / 60);
		const diffHours = Math.floor(diffMins / 60);
		const diffDays = Math.floor(diffHours / 24);

		if (diffMins < 1) return "Just now";
		if (diffMins < 60) return `${diffMins}m ago`;
		if (diffHours < 24) return `${diffHours}h ago`;
		if (diffDays < 7) return `${diffDays}d ago`;

		return date.toLocaleDateString();
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
			await invalidateAll();
		} catch (err) {
			alert(err instanceof Error ? err.message : "Failed to cancel job");
		}
	}

	async function handleRefresh() {
		await invalidateAll();
	}

	$: hasActiveJobs = jobs.some(
		(j) => j.status === "pending" || j.status === "running",
	);

	// Compute summary statistics from jobs
	$: jobStats = {
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
	};

	$: successRate =
		jobStats.total > 0
			? Math.round(
					(jobStats.succeeded /
						(jobStats.succeeded + jobStats.failed)) *
						100,
				) || 0
			: 0;

	$: successRateColor =
		successRate >= 95
			? "text-success"
			: successRate >= 80
				? "text-warning"
				: "text-error";

	function formatNumber(num: number): string {
		if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
		if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
		return num.toLocaleString();
	}
</script>

<Page>
	<div class="max-w-7xl">
		<!-- Header -->
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
			<Button onclick={handleRefresh} variant="secondary">
				<iconify-icon icon="ri:refresh-line" class="text-lg"
				></iconify-icon>
				Refresh
			</Button>
		</div>

		<!-- Summary Metrics Cards -->
		{#if jobs.length > 0}
			<div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
				<!-- Success Rate -->
				<div class="bg-surface border border-border rounded-lg p-4">
					<div
						class="text-xs font-medium text-foreground-subtle uppercase tracking-wide mb-1"
					>
						Success Rate
					</div>
					<div class="flex items-baseline gap-1">
						<span class="text-2xl font-semibold {successRateColor}">
							{successRate}%
						</span>
						{#if jobStats.failed > 0}
							<span class="text-xs text-foreground-subtle">
								({jobStats.failed} failed)
							</span>
						{/if}
					</div>
				</div>

				<!-- Jobs Completed -->
				<div class="bg-surface border border-border rounded-lg p-4">
					<div
						class="text-xs font-medium text-foreground-subtle uppercase tracking-wide mb-1"
					>
						Jobs Completed
					</div>
					<div class="flex items-baseline gap-1">
						<span class="text-2xl font-semibold text-foreground">
							{jobStats.succeeded}
						</span>
						<span class="text-sm text-foreground-subtle">
							/ {jobStats.total}
						</span>
					</div>
				</div>

				<!-- Records Processed -->
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

				<!-- Active Jobs -->
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
							<iconify-icon
								icon="ri:loader-4-line"
								class="text-primary animate-spin"
							></iconify-icon>
						{/if}
					</div>
				</div>
			</div>
		{/if}

		<!-- Time Window Comparison -->
		{#if data.metrics?.time_windows}
			<div class="mb-8">
				<h3 class="text-sm font-medium text-foreground-muted mb-3">
					Trends
				</h3>
				<div class="grid grid-cols-3 gap-4">
					{#each [{ label: "Last 24h", stats: data.metrics.time_windows.last_24h }, { label: "Last 7d", stats: data.metrics.time_windows.last_7d }, { label: "Last 30d", stats: data.metrics.time_windows.last_30d }] as period}
						<div
							class="bg-surface border border-border rounded-lg p-4"
						>
							<div class="text-xs text-foreground-subtle mb-2">
								{period.label}
							</div>
							<div class="text-lg font-semibold text-foreground">
								{period.stats.jobs_completed} jobs
							</div>
							<div class="text-sm text-foreground-muted">
								{period.stats.success_rate_percent.toFixed(0)}%
								success · {formatNumber(
									period.stats.records_processed,
								)} records
							</div>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		<!-- Job Type Breakdown -->
		{#if data.metrics?.by_job_type && data.metrics.by_job_type.length > 0}
			<div class="mb-8">
				<h3 class="text-sm font-medium text-foreground-muted mb-3">
					By Job Type
				</h3>
				<div class="grid grid-cols-2 md:grid-cols-3 gap-4">
					{#each data.metrics.by_job_type as typeStats}
						<div
							class="bg-surface border border-border rounded-lg p-4"
						>
							<div
								class="text-sm font-medium capitalize text-foreground-muted"
							>
								{typeStats.job_type}
							</div>
							<div class="text-2xl font-semibold text-foreground">
								{typeStats.total}
							</div>
							<div class="text-xs text-foreground-subtle">
								{typeStats.succeeded} succeeded · {typeStats.failed}
								failed
							</div>
							{#if typeStats.avg_duration_seconds}
								<div
									class="text-xs text-foreground-subtle mt-1"
								>
									Avg: {Math.round(
										typeStats.avg_duration_seconds,
									)}s
								</div>
							{/if}
						</div>
					{/each}
				</div>
			</div>
		{/if}

		<!-- Recent Errors -->
		{#if data.metrics?.recent_errors && data.metrics.recent_errors.length > 0}
			<div class="mb-8">
				<h3 class="text-sm font-medium text-foreground-muted mb-3">
					Recent Errors
				</h3>
				<div
					class="bg-error-subtle border border-error rounded-lg divide-y divide-error"
				>
					{#each data.metrics.recent_errors.slice(0, 5) as error}
						<div class="p-3">
							<div class="flex items-center justify-between">
								<div class="text-sm text-error font-medium">
									{error.job_type}
									{error.stream_name
										? `· ${error.stream_name}`
										: ""}
								</div>
								<div class="text-xs text-error/70">
									{formatRelativeTime(error.failed_at)}
								</div>
							</div>
							<div
								class="text-xs text-error/80 truncate mt-1"
								title={error.error_message}
							>
								{error.error_message}
							</div>
						</div>
					{/each}
				</div>
			</div>
		{/if}

		<!-- Activity Table -->
		{#if jobs.length === 0}
			<div
				class="border-2 border-dashed border-border rounded-lg p-12 text-center bg-surface-elevated"
			>
				<iconify-icon
					icon="ri:history-line"
					class="text-6xl text-foreground-subtle mb-4"
				></iconify-icon>
				<h3 class="text-lg font-medium text-foreground mb-2">
					No activity yet
				</h3>
				<p class="text-foreground-muted">
					System activity will appear here once your sources start
					syncing
				</p>
			</div>
		{:else}
			<div class=" border border-border rounded-lg overflow-hidden">
				<table class="w-full">
					<thead class="bg-surface-elevated border-b border-border">
						<tr>
							<th
								class="px-6 py-4 text-left text-xs font-medium text-foreground-subtle uppercase"
							>
								Type
							</th>
							<th
								class="px-6 py-4 text-left text-xs font-medium text-foreground-subtle uppercase"
							>
								Source
							</th>
							<th
								class="px-6 py-4 text-left text-xs font-medium text-foreground-subtle uppercase"
							>
								Status
							</th>
							<th
								class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase"
							>
								Duration
							</th>
							<th
								class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase"
							>
								Records
							</th>
							<th
								class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase"
							>
								Time
							</th>
							<th
								class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase"
							>
								Actions
							</th>
						</tr>
					</thead>
					<tbody class="divide-y divide-border">
						{#each jobs as job}
							<tr
								class="hover:bg-surface-elevated transition-colors"
							>
								<!-- Type -->
								<td class="px-6 py-4 whitespace-nowrap">
									<span class="text-sm text-foreground">
										{getJobTypeLabel(
											job.job_type,
											job.stream_name || undefined,
										)}
									</span>
								</td>

								<!-- Source -->
								<td class="px-6 py-4 whitespace-nowrap">
									{#if job.source_id}
										<a
											href="/data/sources/{job.source_id}"
											class="text-sm text-foreground-muted hover:text-foreground hover:underline"
										>
											{job.source_name || job.source_id}
										</a>
									{:else}
										<span
											class="text-sm text-foreground-subtle"
											>—</span
										>
									{/if}
								</td>

								<!-- Status -->
								<td class="px-6 py-4 whitespace-nowrap">
									<Badge
										variant={getStatusVariant(job.status)}
										outline
										class="capitalize"
									>
										{#if job.status === "failed" && job.error_message}
											<iconify-icon
												icon="ri:error-warning-line"
												class="text-xs"
											></iconify-icon>
										{/if}
										{#if job.status === "running"}
											<iconify-icon
												icon="ri:loader-4-line"
												class="text-xs animate-spin"
											></iconify-icon>
										{/if}
										{job.status.replace("_", " ")}
									</Badge>
								</td>

								<!-- Duration -->
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

								<!-- Records -->
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

								<!-- Time -->
								<td
									class="px-6 py-4 whitespace-nowrap text-right"
								>
									<span class="text-sm text-foreground-muted">
										{formatRelativeTime(job.started_at)}
									</span>
								</td>

								<!-- Actions -->
								<td
									class="px-6 py-4 whitespace-nowrap text-right"
								>
									<div
										class="flex items-center justify-end gap-2"
									>
										{#if job.status === "pending" || job.status === "running"}
											<button
												on:click={() =>
													handleCancelJob(job.id)}
												class="text-xs text-error hover:text-error/80 hover:underline"
											>
												Cancel
											</button>
										{/if}
										{#if job.status === "failed" && job.error_message}
											<button
												on:click={() =>
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

			<!-- Summary -->
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
