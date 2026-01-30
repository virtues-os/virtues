<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	interface LakeSummary {
		total_bytes: number;
		compressed_bytes: number;
		compression_ratio: number;
		encrypted: boolean;
		stream_count: number;
		object_count: number;
		record_count: number;
	}

	interface LakeStream {
		source_id: string;
		source_name: string;
		source_type: string;
		stream_name: string;
		size_bytes: number;
		record_count: number;
		object_count: number;
		earliest_at: string | null;
		latest_at: string | null;
	}

	let summary: LakeSummary | null = $state(null);
	let streams: LakeStream[] = $state([]);
	let loading = $state(true);
	let error: string | null = $state(null);

	onMount(async () => {
		await loadData();
	});

	async function loadData() {
		loading = true;
		error = null;

		try {
			const [summaryRes, streamsRes] = await Promise.all([
				fetch("/api/lake/summary"),
				fetch("/api/lake/streams"),
			]);

			if (!summaryRes.ok) {
				throw new Error(`Failed to load lake summary: ${summaryRes.statusText}`);
			}
			if (!streamsRes.ok) {
				throw new Error(`Failed to load lake streams: ${streamsRes.statusText}`);
			}

			summary = await summaryRes.json();
			streams = await streamsRes.json();
		} catch (e: any) {
			console.error("Failed to load lake data", e);
			error = e.message;
		} finally {
			loading = false;
		}
	}

	function formatBytes(bytes: number): string {
		if (bytes === 0) return "0 B";
		const k = 1024;
		const sizes = ["B", "KB", "MB", "GB", "TB", "PB"];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
	}

	function formatNumber(n: number): string {
		return n.toLocaleString();
	}

	function formatDate(dateStr: string | null): string {
		if (!dateStr) return "—";
		const date = new Date(dateStr);
		return date.getFullYear().toString();
	}

	function formatDateRange(earliest: string | null, latest: string | null): string {
		if (!earliest && !latest) return "—";
		const start = formatDate(earliest);
		const end = latest ? (new Date(latest).getFullYear() === new Date().getFullYear() ? "now" : formatDate(latest)) : "now";
		return `${start} → ${end}`;
	}

	function getCompressionPercent(ratio: number): string {
		return `${Math.round((1 - ratio) * 100)}%`;
	}
</script>

<div class="flex h-full w-full flex-col bg-background">
	<!-- Header -->
	<div class="flex h-[53px] flex-none items-center justify-between border-b border-border px-6">
		<div class="flex items-center gap-3">
			<Icon icon="ri:database-2-line" class="text-lg text-primary" />
			<div>
				<h1 class="text-sm font-medium text-foreground">Lake</h1>
				<p class="text-xs text-foreground-muted">Immutable data archive</p>
			</div>
		</div>
		<button
			onclick={loadData}
			disabled={loading}
			class="flex items-center gap-1.5 rounded px-2 py-1 text-xs text-foreground-muted hover:bg-surface-elevated hover:text-foreground disabled:opacity-50"
		>
			<Icon icon="ri:refresh-line" class={loading ? "animate-spin" : ""} />
			Refresh
		</button>
	</div>

	<!-- Content -->
	<div class="flex-1 overflow-auto p-6">
		{#if loading && !summary}
			<div class="flex h-full items-center justify-center">
				<div class="flex items-center gap-2 text-foreground-muted">
					<Icon icon="ri:loader-4-line" class="animate-spin" />
					Loading lake data...
				</div>
			</div>
		{:else if error}
			<div class="rounded-md border border-destructive/20 bg-destructive/10 p-4 text-destructive">
				<div class="flex items-center gap-2 font-medium">
					<Icon icon="ri:error-warning-line" />
					Failed to load lake data
				</div>
				<p class="mt-1 text-xs">{error}</p>
			</div>
		{:else if summary}
			<!-- Stats Cards -->
			<div class="mb-8 grid grid-cols-3 gap-4">
				<div class="rounded-lg border border-border bg-surface p-4">
					<div class="text-2xl font-semibold text-foreground">
						{formatBytes(summary.total_bytes)}
					</div>
					<div class="mt-1 text-xs text-foreground-muted">Archived</div>
				</div>
				<div class="rounded-lg border border-border bg-surface p-4">
					<div class="text-2xl font-semibold text-foreground">
						{formatBytes(summary.compressed_bytes)}
					</div>
					<div class="mt-1 text-xs text-foreground-muted">
						Compressed ({getCompressionPercent(summary.compression_ratio)} saved)
					</div>
				</div>
				<div class="rounded-lg border border-border bg-surface p-4">
					<div class="flex items-center gap-2">
						<Icon icon="ri:lock-line" class="text-xl text-success" />
						<span class="text-lg font-medium text-foreground">Encrypted</span>
					</div>
					<div class="mt-1 text-xs text-foreground-muted">At rest</div>
				</div>
			</div>

			<!-- Streams Table -->
			<div class="rounded-lg border border-border bg-surface">
				<div class="flex items-center justify-between border-b border-border px-4 py-3">
					<h2 class="text-sm font-medium text-foreground">Streams</h2>
					<span class="text-xs text-foreground-muted">
						{summary.stream_count} streams · {formatNumber(summary.record_count)} records
					</span>
				</div>

				{#if streams.length === 0}
					<div class="flex flex-col items-center justify-center py-12 text-foreground-muted">
						<Icon icon="ri:inbox-line" class="text-3xl opacity-50" />
						<p class="mt-2 text-sm">No data archived yet</p>
						<p class="mt-1 text-xs">Connect a source to start archiving</p>
					</div>
				{:else}
					<div class="overflow-x-auto">
						<table class="w-full">
							<thead>
								<tr class="border-b border-border text-left text-xs text-foreground-muted">
									<th class="px-4 py-2 font-medium">Source</th>
									<th class="px-4 py-2 font-medium">Stream</th>
									<th class="px-4 py-2 font-medium text-right">Size</th>
									<th class="px-4 py-2 font-medium text-right">Records</th>
									<th class="px-4 py-2 font-medium">Coverage</th>
								</tr>
							</thead>
							<tbody class="divide-y divide-border">
								{#each streams as stream}
									<tr class="hover:bg-surface-elevated/50">
										<td class="px-4 py-2.5">
											<div class="flex items-center gap-2">
												<span class="text-sm text-foreground">{stream.source_name}</span>
												<span class="rounded bg-surface-elevated px-1.5 py-0.5 text-[10px] text-foreground-muted">
													{stream.source_type}
												</span>
											</div>
										</td>
										<td class="px-4 py-2.5">
											<span class="font-mono text-xs text-foreground-muted">{stream.stream_name}</span>
										</td>
										<td class="px-4 py-2.5 text-right">
											<span class="text-sm text-foreground">{formatBytes(stream.size_bytes)}</span>
										</td>
										<td class="px-4 py-2.5 text-right">
											<span class="text-sm text-foreground-muted">{formatNumber(stream.record_count)}</span>
										</td>
										<td class="px-4 py-2.5">
											<span class="text-xs text-foreground-muted">
												{formatDateRange(stream.earliest_at, stream.latest_at)}
											</span>
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
				{/if}
			</div>
		{/if}
	</div>
</div>
