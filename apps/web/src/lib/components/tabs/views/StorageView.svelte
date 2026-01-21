<script lang="ts">
	import type { Tab } from '$lib/stores/windowTabs.svelte';
	import { Page } from '$lib';
	import type { StreamObject, ObjectContent } from '$lib/api/client';
	import { getStorageObjectContent } from '$lib/api/client';
	import StorageObjectPanel from '$lib/components/storage/StorageObjectPanel.svelte';
	import 'iconify-icon';
	import { onMount } from 'svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let objects = $state<StreamObject[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let selectedObject: StreamObject | null = $state(null);
	let objectContent: ObjectContent | null = $state(null);
	let contentLoading = $state(false);
	let panelOpen = $state(false);
	let panelError: string | null = $state(null);

	onMount(async () => {
		await loadData();
	});

	async function loadData() {
		loading = true;
		error = null;
		try {
			const response = await fetch('/api/storage/objects');
			if (!response.ok) throw new Error('Failed to load storage objects');
			objects = await response.json();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load storage objects';
		} finally {
			loading = false;
		}
	}

	async function handleObjectClick(obj: StreamObject) {
		selectedObject = obj;
		panelOpen = true;
		contentLoading = true;
		panelError = null;
		objectContent = null;

		try {
			objectContent = await getStorageObjectContent(obj.id);
		} catch (e) {
			panelError = e instanceof Error ? e.message : 'Failed to load content';
		} finally {
			contentLoading = false;
		}
	}

	function handlePanelClose() {
		panelOpen = false;
		selectedObject = null;
		objectContent = null;
		panelError = null;
	}

	function formatRelativeTime(timestamp: string): string {
		const date = new Date(timestamp);
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffMins = Math.floor(diffMs / 1000 / 60);
		const diffHours = Math.floor(diffMins / 60);
		const diffDays = Math.floor(diffHours / 24);

		if (diffMins < 1) return 'Just now';
		if (diffMins < 60) return `${diffMins}m ago`;
		if (diffHours < 24) return `${diffHours}h ago`;
		if (diffDays < 7) return `${diffDays}d ago`;

		return date.toLocaleDateString();
	}

	function formatBytes(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
	}

	function formatTimeRange(min: string | null, max: string | null): string {
		if (!min || !max) return '—';
		const minDate = new Date(min);
		const maxDate = new Date(max);

		if (minDate.toDateString() === maxDate.toDateString()) {
			return minDate.toLocaleDateString();
		}

		return `${minDate.toLocaleDateString()} - ${maxDate.toLocaleDateString()}`;
	}

	const totalRecords = $derived(objects.reduce((sum, obj) => sum + obj.record_count, 0));
	const totalSize = $derived(objects.reduce((sum, obj) => sum + obj.size_bytes, 0));
</script>

<Page>
	<div class="max-w-7xl">
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">Storage</h1>
			<p class="text-foreground-muted">
				Recent data objects stored in S3. Click to view decrypted content.
			</p>
		</div>

		{#if loading}
			<div class="text-center py-12 text-foreground-muted">Loading...</div>
		{:else if error}
			<div class="p-4 bg-error-subtle border border-error rounded-lg text-error">{error}</div>
		{:else if objects.length === 0}
			<div class="border-2 border-dashed border-border rounded-lg p-12 text-center bg-surface-elevated">
				<iconify-icon icon="ri:database-2-line" class="text-6xl text-foreground-subtle mb-4"
				></iconify-icon>
				<h3 class="text-lg font-medium text-foreground mb-2">No storage objects yet</h3>
				<p class="text-foreground-muted">
					Data will appear here once your sources start syncing and archiving
				</p>
			</div>
		{:else}
			<div class="grid grid-cols-3 gap-4 mb-8">
				<div class="bg-surface border border-border rounded-lg p-4">
					<div class="text-xs font-medium text-foreground-subtle uppercase tracking-wide mb-1">
						Objects
					</div>
					<div class="text-2xl font-semibold text-foreground">{objects.length}</div>
				</div>
				<div class="bg-surface border border-border rounded-lg p-4">
					<div class="text-xs font-medium text-foreground-subtle uppercase tracking-wide mb-1">
						Total Records
					</div>
					<div class="text-2xl font-semibold text-foreground">{totalRecords.toLocaleString()}</div>
				</div>
				<div class="bg-surface border border-border rounded-lg p-4">
					<div class="text-xs font-medium text-foreground-subtle uppercase tracking-wide mb-1">
						Total Size
					</div>
					<div class="text-2xl font-semibold text-foreground">{formatBytes(totalSize)}</div>
				</div>
			</div>

			<div class="border border-border rounded-lg overflow-hidden">
				<table class="w-full">
					<thead class="bg-surface-elevated border-b border-border">
						<tr>
							<th class="px-6 py-4 text-left text-xs font-medium text-foreground-subtle uppercase">
								Stream
							</th>
							<th class="px-6 py-4 text-left text-xs font-medium text-foreground-subtle uppercase">
								Source
							</th>
							<th class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase">
								Records
							</th>
							<th class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase">
								Size
							</th>
							<th class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase">
								Time Range
							</th>
							<th class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase">
								Created
							</th>
						</tr>
					</thead>
					<tbody class="divide-y divide-border">
						{#each objects as obj}
							<tr
								class="hover:bg-surface-elevated transition-colors cursor-pointer"
								onclick={() => handleObjectClick(obj)}
								role="button"
								tabindex="0"
								onkeydown={(e) => e.key === 'Enter' && handleObjectClick(obj)}
							>
								<td class="px-6 py-4 whitespace-nowrap">
									<span class="text-sm font-medium text-foreground">{obj.stream_name}</span>
								</td>
								<td class="px-6 py-4 whitespace-nowrap">
									<div class="flex flex-col">
										<span class="text-sm text-foreground-muted">{obj.source_name}</span>
										<span class="text-xs text-foreground-subtle">{obj.source_type}</span>
									</div>
								</td>
								<td class="px-6 py-4 whitespace-nowrap text-right">
									<span class="text-sm text-foreground-muted">{obj.record_count.toLocaleString()}</span>
								</td>
								<td class="px-6 py-4 whitespace-nowrap text-right">
									<span class="text-sm text-foreground-muted">{formatBytes(obj.size_bytes)}</span>
								</td>
								<td class="px-6 py-4 whitespace-nowrap text-right">
									<span class="text-sm text-foreground-muted">
										{formatTimeRange(obj.min_timestamp, obj.max_timestamp)}
									</span>
								</td>
								<td class="px-6 py-4 whitespace-nowrap text-right">
									<span class="text-sm text-foreground-muted">{formatRelativeTime(obj.created_at)}</span>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>

			<div class="mt-4 text-sm text-foreground-muted">
				Showing last {objects.length} object{objects.length !== 1 ? 's' : ''}
			</div>
		{/if}
	</div>
</Page>

<StorageObjectPanel
	object={selectedObject}
	content={objectContent}
	loading={contentLoading}
	error={panelError}
	open={panelOpen}
	onClose={handlePanelClose}
/>
