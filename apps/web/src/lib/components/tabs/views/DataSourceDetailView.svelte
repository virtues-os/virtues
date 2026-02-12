<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { routeToEntityId } from "$lib/tabs/types";
	import { spaceStore } from "$lib/stores/space.svelte";
	import { Button, Page } from "$lib";
	import { formatTimeAgo } from "$lib/utils/dateUtils";
	import {
		pauseSource,
		resumeSource,
		deleteSource,
		syncStream,
		enableStream,
		getJobStatus,
	} from "$lib/api/client";
	import { toast } from "svelte-sonner";
	import Icon from "$lib/components/Icon.svelte";
	import { onMount, onDestroy } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Extract sourceId from route (e.g., '/source/source_xyz' → 'source_xyz')
	const sourceId = $derived(routeToEntityId(tab.route));

	interface Source {
		id: string;
		name: string;
		source: string;
		type: string;
		is_active: boolean;
		is_internal: boolean;
		enabled_streams_count: number;
		total_streams_count: number;
		last_sync_at: string | null;
		created_at: string | null;
	}

	interface Stream {
		stream_name: string;
		display_name: string;
		description: string;
		is_enabled: boolean;
		last_sync_at: string | null;
		earliest_record_at: string | null;
		latest_record_at: string | null;
		sync_status:
			| "pending"
			| "initial"
			| "incremental"
			| "backfilling"
			| "failed";
	}

	interface CatalogSource {
		name: string;
		description: string;
		auth_type: string;
		stream_count: number;
	}

	interface PlaidAccount {
		account_id: string;
		name: string;
		official_name: string | null;
		account_type: string;
		subtype: string | null;
		mask: string | null;
		balance_current: number | null;
		balance_available: number | null;
		iso_currency_code: string | null;
	}

	let source = $state<Source | null>(null);
	let streams = $state<Stream[]>([]);
	let catalog = $state<CatalogSource[]>([]);
	let plaidAccounts = $state<PlaidAccount[]>([]);
	let loadingAccounts = $state(false);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let isDeleting = $state(false);
	let isPausing = $state(false);
	let syncingStreams = $state(new Map<string, string>()); // stream_name -> job_id
	let enablingStreams = $state(new Set<string>());
	let pollingIntervals = new Map<string, ReturnType<typeof setInterval>>(); // job_id -> interval_id

	onMount(async () => {
		await loadData();
	});

	async function loadData() {
		if (!sourceId) {
			error = "No source ID provided";
			loading = false;
			return;
		}

		loading = true;
		error = null;

		try {
			const [sourceRes, streamsRes, catalogRes] = await Promise.all([
				fetch(`/api/sources/${sourceId}`),
				fetch(`/api/sources/${sourceId}/streams`),
				fetch(`/api/catalog/sources`),
			]);

			if (!sourceRes.ok) {
				throw new Error(
					`Failed to load source: ${sourceRes.statusText}`,
				);
			}
			if (!streamsRes.ok) {
				throw new Error(
					`Failed to load streams: ${streamsRes.statusText}`,
				);
			}
			if (!catalogRes.ok) {
				throw new Error(
					`Failed to load catalog: ${catalogRes.statusText}`,
				);
			}

			source = await sourceRes.json();
			streams = await streamsRes.json();
			const catalogData = await catalogRes.json();
			catalog = catalogData.sources ?? catalogData;

			// Update tab label with source name
			if (source?.name) {
				spaceStore.updateTab(tab.id, { label: source.name });
			}

			// Load Plaid accounts if this is a Plaid source
			if (source?.source === "plaid") {
				await loadPlaidAccounts();
			}
		} catch (e) {
			error =
				e instanceof Error
					? e.message
					: "Failed to load source details";
			console.error("Failed to load source details:", e);
		} finally {
			loading = false;
		}
	}

	// Determine if this source should show sync button (pull-based sources, not Device)
	const shouldShowSyncButton = $derived(() => {
		if (!catalog || !source) return false;
		const sourceName = source.source;
		const catalogSource = catalog.find((c) => c.name === sourceName);
		const authType = catalogSource?.auth_type;
		// Show sync button for pull-based sources (OAuth2, None), but not for Device
		// Plaid uses OAuth2 auth_type in the registry
		return authType === "oauth2" || authType === "none";
	});

	function formatRelativeTime(timestamp: string | null): string {
		return formatTimeAgo(timestamp);
	}

	async function handleTogglePause() {
		if (isPausing || !source) return;
		isPausing = true;

		try {
			if (source.is_active) {
				const updated = await pauseSource(source.id);
				source = updated;
			} else {
				const updated = await resumeSource(source.id);
				source = updated;
			}
		} catch (err) {
			console.error("Failed to toggle source:", err);
			toast.error("Failed to toggle source. Please try again.");
		} finally {
			isPausing = false;
		}
	}

	async function handleDelete() {
		if (isDeleting || !source) return;

		const confirmed = confirm(
			`Are you sure you want to delete "${source.name}"? This will permanently delete the source, all its streams, and all synced data. This action cannot be undone.`,
		);
		if (!confirmed) return;

		isDeleting = true;

		try {
			await deleteSource(source.id);
			// Navigate back to sources list
			spaceStore.openTabFromRoute("/sources");
		} catch (err) {
			console.error("Failed to delete source:", err);
			toast.error("Failed to delete source. Please try again.");
			isDeleting = false;
		}
	}

	async function handleSyncStream(
		streamName: string,
		mode: "incremental" | "backfill" = "incremental",
	) {
		if (syncingStreams.has(streamName) || !source) return;

		try {
			// Build sync request
			const request: any = { sync_mode: mode };
			if (mode === "backfill") {
				// Default backfill to last 1 year if not specified
				const end = new Date();
				const start = new Date();
				start.setFullYear(end.getFullYear() - 1);
				request.start_date = start.toISOString();
				request.end_date = end.toISOString();
			}

			// Trigger the sync job (returns immediately with job_id)
			const response = await syncStream(source.id, streamName, request);

			// Track the job
			syncingStreams.set(streamName, response.job_id);
			syncingStreams = new Map(syncingStreams); // Trigger reactivity

			// Start polling for job status
			startPollingJob(response.job_id, streamName);
		} catch (err) {
			console.error("Failed to start sync:", err);
			toast.error(
				`Failed to start sync: ${err instanceof Error ? err.message : "Unknown error"}`,
			);
		}
	}

	function startPollingJob(jobId: string, streamName: string) {
		// Poll every 2 seconds
		const intervalId = setInterval(async () => {
			try {
				const job = await getJobStatus(jobId);

				// Check if job is complete
				if (
					job.status === "succeeded" ||
					job.status === "failed" ||
					job.status === "cancelled"
				) {
					// Stop polling
					stopPollingJob(jobId, streamName);

					// Update stream data
					const streamIndex = streams.findIndex(
						(s) => s.stream_name === streamName,
					);
					if (streamIndex !== -1 && job.status === "succeeded") {
						// Refresh the page data to get updated last_sync_at
						streams[streamIndex].last_sync_at =
							job.completed_at || new Date().toISOString();
						streams = [...streams]; // Trigger reactivity
					}

					// Show result
					if (job.status === "succeeded") {
						toast.success(
							`Sync completed! Processed ${job.records_processed} records.`,
						);
					} else if (job.status === "failed") {
						toast.error(
							`Sync failed: ${job.error_message || "Unknown error"}`,
						);
					} else if (job.status === "cancelled") {
						toast.warning("Sync was cancelled.");
					}
				}
			} catch (err) {
				console.error("Failed to poll job status:", err);
				stopPollingJob(jobId, streamName);
				toast.error(
					`Error checking sync status: ${err instanceof Error ? err.message : "Unknown error"}`,
				);
			}
		}, 2000);

		pollingIntervals.set(jobId, intervalId);
	}

	function stopPollingJob(jobId: string, streamName: string) {
		// Clear interval
		const intervalId = pollingIntervals.get(jobId);
		if (intervalId) {
			clearInterval(intervalId);
			pollingIntervals.delete(jobId);
		}

		// Remove from syncing streams
		syncingStreams.delete(streamName);
		syncingStreams = new Map(syncingStreams); // Trigger reactivity
	}

	// Cleanup on component destroy
	onDestroy(() => {
		// Clear all polling intervals
		for (const intervalId of pollingIntervals.values()) {
			clearInterval(intervalId);
		}
		pollingIntervals.clear();
	});

	async function handleEnableStream(streamName: string) {
		if (enablingStreams.has(streamName) || !source) return;

		enablingStreams = new Set([...enablingStreams, streamName]);

		try {
			await enableStream(source.id, streamName);

			// Update the stream's is_enabled in the data
			const streamIndex = streams.findIndex(
				(s) => s.stream_name === streamName,
			);
			if (streamIndex !== -1) {
				streams[streamIndex].is_enabled = true;
				streams = [...streams]; // Trigger reactivity
			}

			toast.success("Stream enabled successfully!");
		} catch (err) {
			console.error("Failed to enable stream:", err);
			toast.error(
				`Failed to enable stream: ${err instanceof Error ? err.message : "Unknown error"}`,
			);
		} finally {
			const newSet = new Set(enablingStreams);
			newSet.delete(streamName);
			enablingStreams = newSet;
		}
	}

	function handleBackToSources() {
		spaceStore.openTabFromRoute("/sources");
	}

	async function loadPlaidAccounts() {
		if (!source || source.source !== "plaid") return;

		loadingAccounts = true;
		try {
			const res = await fetch(`/api/plaid/${source.id}/accounts`);
			if (res.ok) {
				plaidAccounts = await res.json();
			}
		} catch (e) {
			console.error("Failed to load Plaid accounts:", e);
		} finally {
			loadingAccounts = false;
		}
	}

	function formatCurrency(
		amount: number | null,
		currencyCode: string | null,
	): string {
		if (amount === null) return "—";
		return new Intl.NumberFormat("en-US", {
			style: "currency",
			currency: currencyCode || "USD",
		}).format(amount);
	}

	function getAccountTypeIcon(type: string): string {
		switch (type.toLowerCase()) {
			case "depository":
				return "ri:bank-line";
			case "credit":
				return "ri:bank-card-line";
			case "investment":
			case "brokerage":
				return "ri:line-chart-line";
			case "loan":
				return "ri:money-dollar-circle-line";
			default:
				return "ri:wallet-line";
		}
	}
</script>

{#if loading}
	<div class="flex items-center justify-center h-full">
		<Icon icon="ri:loader-4-line" width="20" class="spin" />
	</div>
{:else if error}
	<Page>
		<div
			class="p-4 bg-error-subtle border border-error rounded-lg text-error"
		>
			{error}
		</div>
	</Page>
{:else if !source}
	<Page>
		<div class="text-center py-12">
			<h1 class="text-2xl font-serif font-medium text-foreground mb-2">
				Source not found
			</h1>
			<p class="text-foreground-muted mb-4">
				The source you're looking for doesn't exist.
			</p>
			<button
				onclick={handleBackToSources}
				class="inline-flex items-center gap-2 text-foreground hover:underline"
			>
				<Icon icon="ri:arrow-left-line" />
				Back to Sources
			</button>
		</div>
	</Page>
{:else}
	<Page>
		<div class="max-w-7xl">
			<!-- Header -->
			<div class="mb-8">
				<div class="flex items-start justify-between">
					<div class="flex items-center gap-4">
						<div>
							<div class="flex items-center gap-3 mb-1">
								<h1
									class="text-3xl font-serif font-medium text-foreground"
								>
									{source.name}
								</h1>
								{#if source.is_active}
									<span
										class="inline-block px-2 py-1 text-xs font-medium bg-success-subtle text-success rounded-full"
									>
										Active
									</span>
								{:else}
									<span
										class="inline-block px-2 py-1 text-xs font-medium bg-surface-elevated text-foreground-subtle rounded-full"
									>
										Paused
									</span>
								{/if}
							</div>
							<p class="text-foreground-muted capitalize">
								{source.type} Source
							</p>
						</div>
					</div>

					{#if !source.is_internal}
						<div class="flex items-center gap-2">
							<Button
								variant="ghost"
								class="flex items-center gap-2 border border-border"
								onclick={handleTogglePause}
								disabled={isPausing}
							>
								<Icon
									icon={source.is_active
										? "ri:pause-line"
										: "ri:play-line"}
								/>
								<span
									>{source.is_active
										? "Pause"
										: "Resume"}</span
								>
							</Button>
							<Button
								variant="danger"
								class="flex items-center gap-2"
								onclick={handleDelete}
								disabled={isDeleting}
							>
								<Icon icon="ri:delete-bin-line" />
								<span>Delete</span>
							</Button>
						</div>
					{/if}
				</div>
			</div>

			<!-- Key Attributes -->
			<div class="mb-8 text-foreground-muted space-y-1">
				<p>
					Created {formatRelativeTime(
						source.created_at,
					).toLowerCase()}
				</p>
				<p>
					Last synced {formatRelativeTime(
						source.last_sync_at,
					).toLowerCase()}
				</p>
				<p>
					{source.enabled_streams_count} of {source.total_streams_count}
					streams enabled
				</p>
			</div>

			<!-- Plaid Accounts Section (only for Plaid sources) -->
			{#if source.source === "plaid"}
				<div class="mb-8">
					<h2
						class="text-xl font-serif font-medium text-foreground mb-4"
					>
						Connected Accounts
						{#if plaidAccounts.length > 0}
							<span
								class="text-foreground-subtle text-sm font-normal"
							>
								({plaidAccounts.length})
							</span>
						{/if}
					</h2>

					{#if loadingAccounts}
						<div class="text-foreground-muted py-4">
							Loading accounts...
						</div>
					{:else if plaidAccounts.length === 0}
						<div
							class="border border-border rounded-lg p-6 text-center bg-surface-elevated"
						>
							<p class="text-foreground-muted">
								No accounts found for this connection.
							</p>
						</div>
					{:else}
						<div
							class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4"
						>
							{#each plaidAccounts as account}
								<div
									class="p-4 bg-surface border border-border rounded-lg"
								>
									<div class="flex items-start gap-3 mb-3">
										<Icon
											icon={getAccountTypeIcon(
												account.account_type,
											)}
											class="text-xl text-foreground-subtle mt-0.5"
										/>
										<div class="flex-1 min-w-0">
											<h4
												class="font-medium text-foreground truncate"
											>
												{account.name}
											</h4>
											{#if account.official_name && account.official_name !== account.name}
												<p
													class="text-xs text-foreground-subtle truncate"
												>
													{account.official_name}
												</p>
											{/if}
											<p
												class="text-xs text-foreground-muted capitalize"
											>
												{account.subtype ||
													account.account_type}
												{#if account.mask}
													<span
														class="text-foreground-subtle"
													>
														••{account.mask}
													</span>
												{/if}
											</p>
										</div>
									</div>
									<div class="space-y-1 text-sm">
										{#if account.balance_current !== null}
											<div
												class="flex items-center justify-between"
											>
												<span
													class="text-foreground-muted"
													>Current</span
												>
												<span
													class="font-medium text-foreground"
												>
													{formatCurrency(
														account.balance_current,
														account.iso_currency_code,
													)}
												</span>
											</div>
										{/if}
										{#if account.balance_available !== null && account.account_type === "depository"}
											<div
												class="flex items-center justify-between"
											>
												<span
													class="text-foreground-muted"
													>Available</span
												>
												<span class="text-foreground">
													{formatCurrency(
														account.balance_available,
														account.iso_currency_code,
													)}
												</span>
											</div>
										{/if}
									</div>
								</div>
							{/each}
						</div>
					{/if}
				</div>
			{/if}

			<!-- Streams Section -->
			<div class="mb-8">
				<h2 class="text-xl font-serif font-medium text-foreground mb-4">
					Streams
				</h2>

				<div
					class="bg-surface border border-border rounded-lg overflow-hidden"
				>
					<table class="w-full">
						<thead
							class="bg-surface-elevated border-b border-border"
						>
							<tr>
								<th
									class="px-6 py-3 text-left text-xs font-medium text-foreground-muted uppercase tracking-wider"
								>
									Stream
								</th>
								<th
									class="px-6 py-3 text-left text-xs font-medium text-foreground-muted uppercase tracking-wider"
								>
									Status
								</th>
								<th
									class="px-6 py-3 text-left text-xs font-medium text-foreground-muted uppercase tracking-wider"
								>
									Last Sync
								</th>
								{#if shouldShowSyncButton()}
									<th
										class="px-6 py-3 text-right text-xs font-medium text-foreground-muted uppercase tracking-wider"
									>
										Actions
									</th>
								{/if}
							</tr>
						</thead>
						<tbody class="divide-y divide-border">
							{#each streams as stream}
								<tr
									class="hover:bg-surface-elevated transition-colors"
								>
									<td class="px-6 py-4">
										<div>
											<div
												class="font-medium text-foreground"
											>
												{stream.display_name}
											</div>
											<div
												class="text-sm text-foreground-subtle"
											>
												{stream.description}
											</div>
										</div>
									</td>
									<td class="px-6 py-4">
										{#if stream.is_enabled}
											<span
												class="inline-block px-2 py-1 text-xs font-medium bg-success-subtle text-success rounded-full"
											>
												Enabled
											</span>
										{:else}
											<span
												class="inline-block px-2 py-1 text-xs font-medium bg-surface-elevated text-foreground-subtle rounded-full"
											>
												Disabled
											</span>
										{/if}
									</td>
									<td class="px-6 py-4">
										<div class="flex flex-col">
											<span
												class="text-sm text-foreground"
											>
												{formatRelativeTime(
													stream.last_sync_at,
												)}
											</span>
											{#if stream.earliest_record_at}
												<span
													class="text-[10px] text-foreground-subtle uppercase tracking-tight"
												>
													From {new Date(
														stream.earliest_record_at,
													).toLocaleDateString()}
												</span>
											{/if}
										</div>
									</td>
									{#if shouldShowSyncButton()}
										<td class="px-6 py-4 text-right">
											{#if stream.is_enabled}
												<button
													onclick={() =>
														handleSyncStream(
															stream.stream_name,
														)}
													disabled={syncingStreams.has(
														stream.stream_name,
													)}
													class="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-foreground-muted bg-surface border border-border rounded-md hover:bg-surface-elevated focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-primary disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
												>
													{#if syncingStreams.has(stream.stream_name)}
														Syncing...
													{:else}
														Sync Now
													{/if}
												</button>
											{:else}
												<button
													onclick={() =>
														handleEnableStream(
															stream.stream_name,
														)}
													disabled={enablingStreams.has(
														stream.stream_name,
													)}
													class="inline-flex items-center gap-1.5 px-3 py-1.5 text-sm font-medium text-foreground-muted bg-surface border border-border rounded-md hover:bg-surface-elevated focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-primary disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
												>
													{#if enablingStreams.has(stream.stream_name)}
														Enabling...
													{:else}
														Enable
													{/if}
												</button>
											{/if}
										</td>
									{/if}
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</div>
		</div>
	</Page>
{/if}
