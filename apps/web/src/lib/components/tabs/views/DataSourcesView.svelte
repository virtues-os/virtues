<script lang="ts">
	import type { Tab } from '$lib/stores/windowTabs.svelte';
	import { windowTabs } from '$lib/stores/windowTabs.svelte';
	import { Button, Page, Badge } from '$lib';
	import 'iconify-icon';
	import { onMount } from 'svelte';

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	interface Source {
		id: string;
		name: string;
		type: string;
		is_active: boolean;
		enabled_streams_count: number;
		total_streams_count: number;
		last_sync_at: string | null;
	}

	interface CatalogSource {
		name: string;
		description: string;
		auth_type: string;
		stream_count: number;
	}

	let sources = $state<Source[]>([]);
	let catalog = $state<CatalogSource[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(async () => {
		await loadData();
	});

	async function loadData() {
		loading = true;
		error = null;
		try {
			const [sourcesRes, catalogRes] = await Promise.all([
				fetch('/api/sources'),
				fetch('/api/catalog/sources')
			]);
			if (!sourcesRes.ok || !catalogRes.ok) throw new Error('Failed to load data');
			sources = await sourcesRes.json();
			catalog = await catalogRes.json();
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load data';
		} finally {
			loading = false;
		}
	}

	function formatRelativeTime(timestamp: string | null): string {
		if (!timestamp) return 'Never';

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

	function isSourceConnected(catalogSourceName: string): boolean {
		return sources.some((s) => s.type === catalogSourceName);
	}

	const availableCatalog = $derived(catalog.filter((s) => s.auth_type !== 'none'));

	function handleSourceClick(sourceId: string) {
		windowTabs.openTabFromRoute(`/data/sources/${sourceId}`);
	}

	function handleAddSource(type?: string) {
		const route = type ? `/data/sources/add?type=${type}` : '/data/sources/add';
		windowTabs.openTabFromRoute(route);
	}
</script>

<Page>
	<div class="max-w-7xl">
		<div class="flex items-center justify-between">
			<div>
				<h1 class="text-3xl font-serif font-medium text-foreground mb-2">Sources</h1>
				<p class="text-foreground-muted">
					Manage your connected data sources and their sync schedules
				</p>
			</div>
			<Button variant="secondary" class="flex items-center gap-2" onclick={() => handleAddSource()}>
				<iconify-icon icon="ri:add-line" class="text-lg"></iconify-icon>
				<span>Add Source</span>
			</Button>
		</div>

		{#if loading}
			<div class="mt-8 text-center py-12 text-foreground-muted">Loading...</div>
		{:else if error}
			<div class="mt-8 p-4 bg-error-subtle border border-error rounded-lg text-error">
				{error}
			</div>
		{:else}
			<div class="mt-8">
				<h2 class="text-xl font-serif font-medium text-foreground mb-4">
					Connected Sources
					{#if sources.length > 0}
						<span class="text-foreground-subtle text-sm font-normal">( {sources.length} )</span>
					{/if}
				</h2>

				{#if sources.length === 0}
					<div class="border border-border rounded-lg p-12 text-center bg-surface-elevated">
						<iconify-icon icon="ri:plug-line" class="text-6xl text-foreground-subtle mb-4"
						></iconify-icon>
						<h3 class="text-lg font-medium text-foreground mb-2">No sources connected</h3>
						<p class="text-foreground-muted mb-4">
							Connect your first data source to start syncing your personal data
						</p>
						<Button variant="primary" onclick={() => handleAddSource()}>Add Your First Source</Button>
					</div>
				{:else}
					<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
						{#each sources as source}
							<button
								onclick={() => handleSourceClick(source.id)}
								class="text-left block p-6 bg-surface border border-border rounded-lg hover:border-border-subtle hover:bg-surface-elevated transition-all duration-200 cursor-pointer"
							>
								<div class="flex items-start justify-between mb-4">
									<div class="flex items-center gap-3">
										<h3 class="font-medium text-foreground">{source.name}</h3>
									</div>
									{#if source.is_active}
										<span class="inline-block w-2 h-2 bg-success rounded-full" title="Active"
										></span>
									{:else}
										<span
											class="inline-block w-2 h-2 bg-foreground-subtle rounded-full"
											title="Inactive"
										></span>
									{/if}
								</div>

								<div class="space-y-2 text-sm">
									<div class="flex items-center justify-between">
										<span class="text-foreground-muted">Streams</span>
										<span class="font-medium text-foreground">
											{source.enabled_streams_count} of {source.total_streams_count} enabled
										</span>
									</div>
									<div class="flex items-center justify-between">
										<span class="text-foreground-muted">Last sync</span>
										<span class="font-medium text-foreground">
											{formatRelativeTime(source.last_sync_at)}
										</span>
									</div>
								</div>

								<div class="mt-4 pt-4 border-t border-border">
									<span class="text-xs text-foreground-subtle"> View streams and sync history </span>
								</div>
							</button>
						{/each}
					</div>
				{/if}
			</div>

			<div class="pt-8">
				<h2 class="text-xl font-serif font-medium text-foreground mb-2">
					Available Sources
					<span class="text-foreground-subtle text-sm font-normal">( {availableCatalog.length} )</span>
				</h2>

				<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
					{#each availableCatalog as catalogSource}
						{@const connected = isSourceConnected(catalogSource.name)}
						<div
							class="p-6 bg-surface border border-border rounded-lg hover:border-border-subtle transition-all duration-200"
						>
							<div class="flex items-center justify-between mb-3">
								<div class="flex items-center gap-3">
									<h3 class="font-medium text-foreground capitalize">{catalogSource.name}</h3>
								</div>
								{#if connected}
									<Badge variant="success">
										<iconify-icon icon="ri:check-line"></iconify-icon>
										Connected
									</Badge>
								{/if}
							</div>

							<p class="text-sm text-foreground-muted mb-4 line-clamp-2">
								{catalogSource.description}
							</p>

							<div class="flex items-center justify-between">
								<div class="flex items-center gap-3 text-xs text-foreground-subtle">
									<span class="flex items-center gap-1">
										<iconify-icon icon="ri:database-2-line"></iconify-icon>
										{catalogSource.stream_count}
										{catalogSource.stream_count === 1 ? 'stream' : 'streams'}
									</span>
									<Badge class="capitalize">
										{catalogSource.auth_type === 'oauth2' ? 'OAuth' : catalogSource.auth_type}
									</Badge>
								</div>
								<Button variant="primary" size="sm" onclick={() => handleAddSource(catalogSource.name)}>
									{connected ? 'Add Another' : 'Add'}
								</Button>
							</div>
						</div>
					{/each}
				</div>
			</div>
		{/if}
	</div>
</Page>
