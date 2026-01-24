<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import { Button, Page, Badge } from "$lib";
	import "iconify-icon";
	import { onMount } from "svelte";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	interface Source {
		id: string;
		name: string;
		source: string;
		is_active: boolean;
		is_internal: boolean;
		enabled_streams_count: number;
		total_streams_count: number;
		last_sync_at: string | null;
	}

	interface ConnectionLimits {
		free: number;
		starter: number;
		pro: number;
	}

	interface CatalogSource {
		name: string;
		display_name: string;
		description: string;
		auth_type: string;
		stream_count: number;
		icon: string | null;
		is_multi_instance: boolean;
		connection_limits?: ConnectionLimits;
		current_connections?: number;
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
				fetch("/api/sources"),
				fetch("/api/catalog/sources"),
			]);
			if (!sourcesRes.ok || !catalogRes.ok)
				throw new Error("Failed to load data");
			sources = await sourcesRes.json();
			catalog = await catalogRes.json();
		} catch (e) {
			error = e instanceof Error ? e.message : "Failed to load data";
		} finally {
			loading = false;
		}
	}

	function formatRelativeTime(timestamp: string | null): string {
		if (!timestamp) return "Never";

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

	function isSourceConnected(catalogSourceName: string): boolean {
		return sources.some((s) => s.source === catalogSourceName);
	}

	function getConnectionCount(catalogSourceName: string): number {
		return sources.filter((s) => s.source === catalogSourceName).length;
	}

	// For free tier - adjust when tier system is integrated
	const userTier = "free";

	function getConnectionLimit(limits: ConnectionLimits | undefined): number {
		if (!limits) return 1;
		return limits[userTier as keyof ConnectionLimits] ?? limits.free;
	}

	function canAddMore(catalogSource: CatalogSource): boolean {
		if (!catalogSource.is_multi_instance) {
			return !isSourceConnected(catalogSource.name);
		}
		const current = catalogSource.current_connections ?? getConnectionCount(catalogSource.name);
		const limit = getConnectionLimit(catalogSource.connection_limits);
		return current < limit;
	}

	const filteredSources = $derived(
		sources.filter((s) => s.source !== "virtues-app" && !s.is_internal),
	);

	// Group sources by type for display
	const groupedSources = $derived(() => {
		const plaidSources = filteredSources.filter((s) => s.source === "plaid");
		const otherSources = filteredSources.filter((s) => s.source !== "plaid");
		return { plaid: plaidSources, other: otherSources };
	});

	const availableCatalog = $derived(
		catalog.filter((s) => s.auth_type !== "none"),
	);

	function handleSourceClick(sourceId: string) {
		workspaceStore.openTabFromRoute(`/data/sources/${sourceId}`);
	}

	function handleAddSource(type?: string) {
		const route = type
			? `/data/sources/add?type=${type}`
			: "/data/sources/add";
		workspaceStore.openTabFromRoute(route);
	}
</script>

<Page>
	<div class="max-w-7xl">
		<div class="flex items-center justify-between">
			<div>
				<h1
					class="text-3xl font-serif font-medium text-foreground mb-2"
				>
					Sources
				</h1>
				<p class="text-foreground-muted">
					Manage your connected data sources and their sync schedules
				</p>
			</div>
			<Button
				variant="secondary"
				class="flex items-center gap-2"
				onclick={() => handleAddSource()}
			>
				<iconify-icon icon="ri:add-line" class="text-lg"></iconify-icon>
				<span>Add Source</span>
			</Button>
		</div>

		{#if loading}
			<div class="mt-8 text-center py-12 text-foreground-muted">
				Loading...
			</div>
		{:else if error}
			<div
				class="mt-8 p-4 bg-error-subtle border border-error rounded-lg text-error"
			>
				{error}
			</div>
		{:else}
			<div class="mt-8">
				<h2 class="text-xl font-serif font-medium text-foreground mb-4">
					Connected Sources
					{#if filteredSources.length > 0}
						<span class="text-foreground-subtle text-sm font-normal"
							>( {filteredSources.length} )</span
						>
					{/if}
				</h2>

				{#if filteredSources.length === 0}
					<div
						class="border border-border rounded-lg p-12 text-center bg-surface-elevated"
					>
						<iconify-icon
							icon="ri:plug-line"
							class="text-6xl text-foreground-subtle mb-4"
						></iconify-icon>
						<h3 class="text-lg font-medium text-foreground mb-2">
							No sources connected
						</h3>
						<p class="text-foreground-muted mb-4">
							Connect your first data source to start syncing your
							personal data
						</p>
						<Button
							variant="primary"
							onclick={() => handleAddSource()}
							>Add Your First Source</Button
						>
					</div>
				{:else}
					{@const groups = groupedSources()}
					
					<!-- Banking (Plaid) Sources -->
					{#if groups.plaid.length > 0}
						<div class="mb-6">
							<div class="flex items-center gap-2 mb-3">
								<iconify-icon icon="ri:bank-line" class="text-lg text-foreground-muted"></iconify-icon>
								<h3 class="text-sm font-medium text-foreground-muted uppercase tracking-wide">
									Banking
								</h3>
								<span class="text-xs text-foreground-subtle">
									{groups.plaid.length} connected
								</span>
							</div>
							<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
								{#each groups.plaid as source}
									<button
										onclick={() => handleSourceClick(source.id)}
										class="text-left block p-6 bg-surface border border-border rounded-lg hover:border-border-subtle hover:bg-surface-elevated transition-all duration-200 cursor-pointer"
									>
										<div class="flex items-start justify-between mb-4">
											<div class="flex items-center gap-3">
												<iconify-icon icon="ri:bank-card-line" class="text-xl text-foreground-subtle"></iconify-icon>
												<h3 class="font-medium text-foreground">
													{source.name}
												</h3>
											</div>
											{#if source.is_active}
												<span class="inline-block w-2 h-2 bg-success rounded-full" title="Active"></span>
											{:else}
												<span class="inline-block w-2 h-2 bg-foreground-subtle rounded-full" title="Inactive"></span>
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
											<span class="text-xs text-foreground-subtle">
												View accounts and transactions
											</span>
										</div>
									</button>
								{/each}
							</div>
						</div>
					{/if}

					<!-- Other Sources -->
					{#if groups.other.length > 0}
						{#if groups.plaid.length > 0}
							<div class="flex items-center gap-2 mb-3">
								<iconify-icon icon="ri:apps-line" class="text-lg text-foreground-muted"></iconify-icon>
								<h3 class="text-sm font-medium text-foreground-muted uppercase tracking-wide">
									Other Sources
								</h3>
							</div>
						{/if}
						<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
							{#each groups.other as source}
								<button
									onclick={() => handleSourceClick(source.id)}
									class="text-left block p-6 bg-surface border border-border rounded-lg hover:border-border-subtle hover:bg-surface-elevated transition-all duration-200 cursor-pointer"
								>
									<div class="flex items-start justify-between mb-4">
										<div class="flex items-center gap-3">
											<h3 class="font-medium text-foreground">
												{source.name}
											</h3>
										</div>
										{#if source.is_active}
											<span class="inline-block w-2 h-2 bg-success rounded-full" title="Active"></span>
										{:else}
											<span class="inline-block w-2 h-2 bg-foreground-subtle rounded-full" title="Inactive"></span>
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
										<span class="text-xs text-foreground-subtle">
											View streams and sync history
										</span>
									</div>
								</button>
							{/each}
						</div>
					{/if}
				{/if}
			</div>

			<div class="pt-8">
				<h2 class="text-xl font-serif font-medium text-foreground mb-2">
					Available Sources
					<span class="text-foreground-subtle text-sm font-normal"
						>( {availableCatalog.length} )</span
					>
				</h2>

				<div
					class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4"
				>
					{#each availableCatalog as catalogSource}
						{@const connected = isSourceConnected(catalogSource.name)}
						{@const connectionCount = catalogSource.current_connections ?? getConnectionCount(catalogSource.name)}
						{@const limit = getConnectionLimit(catalogSource.connection_limits)}
						{@const canAdd = canAddMore(catalogSource)}
						{@const atLimit = catalogSource.is_multi_instance && connectionCount >= limit}
						<div
							class="p-6 bg-surface border border-border rounded-lg hover:border-border-subtle transition-all duration-200"
						>
							<div class="flex items-center justify-between mb-3">
								<div class="flex items-center gap-3">
									{#if catalogSource.icon}
										<iconify-icon
											icon={catalogSource.icon}
											class="text-2xl text-foreground-subtle"
										></iconify-icon>
									{/if}
									<h3 class="font-medium text-foreground">
										{catalogSource.display_name}
									</h3>
								</div>
								<div class="flex items-center gap-2">
									{#if catalogSource.is_multi_instance && connected}
										<span class="text-xs text-foreground-subtle">
											{connectionCount}/{limit}
										</span>
									{/if}
									{#if connected}
										<Badge variant="success">
											<iconify-icon icon="ri:check-line"></iconify-icon>
											Connected
										</Badge>
									{/if}
								</div>
							</div>

							<p
								class="text-sm text-foreground-muted mb-4 line-clamp-2"
							>
								{catalogSource.description}
							</p>

							<div class="flex items-center justify-between">
								<div
									class="flex items-center gap-3 text-xs text-foreground-subtle"
								>
									<span class="flex items-center gap-1">
										<iconify-icon icon="ri:database-2-line"></iconify-icon>
										{catalogSource.stream_count}
										{catalogSource.stream_count === 1 ? "stream" : "streams"}
									</span>
									<Badge class="capitalize">
										{catalogSource.auth_type === "oauth2"
											? "OAuth"
											: catalogSource.auth_type}
									</Badge>
								</div>
								{#if atLimit}
									<Button
										variant="secondary"
										size="sm"
										onclick={() => workspaceStore.openTabFromRoute('/profile/account')}
									>
										Upgrade
									</Button>
								{:else}
									<Button
										variant="primary"
										size="sm"
										onclick={() => handleAddSource(catalogSource.name)}
									>
										{connected ? "Add Another" : "Add"}
									</Button>
								{/if}
							</div>
						</div>
					{/each}
				</div>
			</div>
		{/if}
	</div>
</Page>
