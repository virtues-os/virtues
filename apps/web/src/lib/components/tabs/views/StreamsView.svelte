<script lang="ts">
	// Streams — the raw evidence floor, read-only.
	//
	// Every record a device ever sent, archived exactly as it arrived, plus the
	// blobs those records point at (the microphone's audio). The ontology tables
	// are a LOSSY projection of this; the doctrine is that you can re-derive
	// stories from evidence but never evidence from stories. So there is
	// deliberately no upload, no rename, no delete here — a floor you can edit
	// isn't a floor. Removal happens only through a retention policy.
	import Icon from "$lib/components/Icon.svelte";
	import { LoadingState, ErrorState } from "$lib";
	import { getLakeSummary, getLakeStreams } from "$lib/api/client";
	import { createResource } from "$lib/utils/resource.svelte";
	import { formatDate } from "$lib/utils/dateUtils";

	interface LakeSummary {
		total_bytes: number;
		object_count: number;
		record_count: number;
		stream_count: number;
	}

	interface LakeStream {
		source_id: string;
		source_name: string;
		/** kind: 'raw_stream' | 'media' | 'drive' */
		source_type: string;
		stream_name: string;
		size_bytes: number;
		record_count: number;
		object_count: number;
		earliest_at: string | null;
		latest_at: string | null;
	}

	const res = createResource(
		() =>
			Promise.all([
				getLakeSummary<LakeSummary>(),
				getLakeStreams<LakeStream[]>(),
			]).then(([summary, streams]) => ({ summary, streams })),
		{ errorMessage: "Failed to load the lake" },
	);

	const summary = $derived(res.data?.summary ?? null);
	const streams = $derived(res.data?.streams ?? []);

	const archives = $derived(streams.filter((s) => s.source_type === "raw_stream"));
	const blobs = $derived(streams.filter((s) => s.source_type === "media"));

	function formatBytes(bytes: number): string {
		if (!bytes) return "0 B";
		const k = 1024;
		const sizes = ["B", "KB", "MB", "GB", "TB"];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
	}

	function formatRange(earliest: string | null, latest: string | null): string {
		if (!earliest && !latest) return "—";
		const fmt = (d: string) => formatDate(d, { month: "short", day: "numeric" });
		const start = earliest ? fmt(earliest) : "—";
		const end = latest ? fmt(latest) : "—";
		return start === end ? start : `${start} → ${end}`;
	}
</script>

<div class="flex h-full w-full flex-col">
	<div class="flex flex-1 flex-col overflow-auto p-6">
		{#if res.loading}
			<LoadingState class="h-full" />
		{:else if res.error}
			<ErrorState
				title="Failed to load the lake"
				message={res.error}
				onRetry={res.reload}
			/>
		{:else if summary}
			<div class="mb-6 grid grid-cols-3 gap-4">
				<div class="rounded-lg border border-border bg-surface p-4">
					<div class="text-2xl font-semibold text-foreground">
						{formatBytes(summary.total_bytes)}
					</div>
					<div class="mt-1 text-xs text-foreground-muted">On disk</div>
				</div>
				<div class="rounded-lg border border-border bg-surface p-4">
					<div class="text-2xl font-semibold text-foreground">
						{summary.record_count.toLocaleString()}
					</div>
					<div class="mt-1 text-xs text-foreground-muted">Records archived</div>
				</div>
				<div class="rounded-lg border border-border bg-surface p-4">
					<div class="text-2xl font-semibold text-foreground">
						{summary.object_count.toLocaleString()}
					</div>
					<div class="mt-1 text-xs text-foreground-muted">Objects</div>
				</div>
			</div>

			<!-- Say plainly why there are no buttons here. -->
			<div
				class="mb-6 flex items-start gap-2.5 rounded-md border border-border bg-surface px-3.5 py-2.5"
			>
				<Icon icon="ri:lock-2-line" class="mt-0.5 flex-none text-foreground-muted" />
				<p class="text-xs leading-relaxed text-foreground-muted">
					<span class="text-foreground">Read-only.</span> This is the raw record of what your
					devices sent — everything else is derived from it. Data can be re-derived from
					evidence, never the other way round, so nothing here can be edited or deleted by
					hand.
				</p>
			</div>

			{#each [{ title: "Archives", hint: "Raw records, exactly as received", rows: archives }, { title: "Media", hint: "Blobs the records point at", rows: blobs }] as group}
				{#if group.rows.length}
					<div class="mb-6">
						<div class="mb-2 flex items-baseline gap-2">
							<h2 class="text-xs font-medium text-foreground">{group.title}</h2>
							<span class="text-xs text-foreground-muted">{group.hint}</span>
						</div>
						<div class="overflow-hidden rounded-lg border border-border">
							<table class="w-full text-xs">
								<thead class="bg-surface text-foreground-muted">
									<tr>
										<th class="px-3 py-2 text-left font-normal">Stream</th>
										<th class="px-3 py-2 text-right font-normal">Records</th>
										<th class="px-3 py-2 text-right font-normal">Objects</th>
										<th class="px-3 py-2 text-right font-normal">Size</th>
										<th class="px-3 py-2 text-right font-normal">Range</th>
									</tr>
								</thead>
								<tbody>
									{#each group.rows as s}
										<tr class="border-t border-border">
											<td class="px-3 py-2 text-foreground">
												<span class="text-foreground-muted">{s.source_name}</span>
												<span class="text-foreground-muted">/</span>
												{s.stream_name}
											</td>
											<td class="px-3 py-2 text-right tabular-nums text-foreground-muted">
												{s.record_count ? s.record_count.toLocaleString() : "—"}
											</td>
											<td class="px-3 py-2 text-right tabular-nums text-foreground-muted">
												{s.object_count.toLocaleString()}
											</td>
											<td class="px-3 py-2 text-right tabular-nums text-foreground">
												{formatBytes(s.size_bytes)}
											</td>
											<td class="px-3 py-2 text-right text-foreground-muted">
												{formatRange(s.earliest_at, s.latest_at)}
											</td>
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					</div>
				{/if}
			{/each}
		{/if}
	</div>
</div>
