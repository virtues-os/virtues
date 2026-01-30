<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { spaceStore } from "$lib/stores/space.svelte";
	import { Button, Page, Badge } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";
	import { toast } from "svelte-sonner";
	import { OAuthConnectModal, PlaidConnectModal, DevicePairModal } from "$lib/components/sources";

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	// Modal state
	type ModalType = 
		| { type: 'oauth'; provider: string; displayName: string }
		| { type: 'plaid' }
		| { type: 'device'; deviceType: 'ios' | 'mac'; displayName: string }
		| null;
	
	let activeModal = $state<ModalType>(null);

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

	interface LakeStream {
		source_id: string;
		source_name: string;
		source_type: string;
		stream_name: string;
		size_bytes: number;
		record_count: number;
		object_count: number;
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
	let lakeStreams = $state<LakeStream[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(async () => {
		// Check for OAuth return (source_id in URL means OAuth completed)
		const params = new URLSearchParams(window.location.search);
		const sourceId = params.get('source_id');
		const errorParam = params.get('error');
		
		if (sourceId) {
			toast.success('Source connected successfully');
			// Clean URL without reload
			window.history.replaceState({}, '', window.location.pathname);
		} else if (errorParam) {
			toast.error('Failed to connect source');
			window.history.replaceState({}, '', window.location.pathname);
		}
		
		await loadData();
	});

	async function loadData() {
		loading = true;
		error = null;
		try {
			const [sourcesRes, catalogRes, lakeRes] = await Promise.all([
				fetch("/api/sources"),
				fetch("/api/catalog/sources"),
				fetch("/api/lake/streams"),
			]);
			if (!sourcesRes.ok || !catalogRes.ok)
				throw new Error("Failed to load data");
			sources = await sourcesRes.json();
			catalog = await catalogRes.json();
			if (lakeRes.ok) {
				lakeStreams = await lakeRes.json();
			}
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

	function getSourceStats(sourceId: string): { records: number; bytes: number } {
		const streams = lakeStreams.filter((s) => s.source_id === sourceId);
		return {
			records: streams.reduce((sum, s) => sum + s.record_count, 0),
			bytes: streams.reduce((sum, s) => sum + s.size_bytes, 0),
		};
	}

	function formatBytes(bytes: number): string {
		if (bytes === 0) return "—";
		if (bytes < 1024) return `${bytes} B`;
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
		if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
		return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
	}

	function formatNumber(num: number): string {
		if (num === 0) return "—";
		if (num >= 1_000_000) return `${(num / 1_000_000).toFixed(1)}M`;
		if (num >= 1_000) return `${(num / 1_000).toFixed(1)}K`;
		return num.toLocaleString();
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

	const availableCatalog = $derived(
		catalog.filter((s) => s.auth_type !== "none"),
	);

	function getSourceTypeLabel(sourceType: string): string {
		const labels: Record<string, string> = {
			plaid: "Plaid",
			google: "Google",
			ios: "iOS",
			mac: "Mac",
			notion: "Notion",
		};
		return labels[sourceType] || sourceType;
	}

	function handleSourceClick(sourceId: string) {
		spaceStore.openTabFromRoute(`/source/${sourceId}`);
	}

	function handleAddSource(catalogSource: CatalogSource) {
		// Check for Plaid first - it uses Plaid Link, not standard OAuth2
		if (catalogSource.name === 'plaid') {
			activeModal = { type: 'plaid' };
		} else if (catalogSource.auth_type === 'oauth2') {
			activeModal = { 
				type: 'oauth', 
				provider: catalogSource.name, 
				displayName: catalogSource.display_name 
			};
		} else if (catalogSource.auth_type === 'device') {
			activeModal = { 
				type: 'device', 
				deviceType: catalogSource.name as 'ios' | 'mac',
				displayName: catalogSource.display_name
			};
		}
	}

	function handleModalSuccess(_sourceId: string, institutionName?: string) {
		activeModal = null;
		const name = institutionName || 'Source';
		toast.success(`${name} connected successfully`);
		loadData(); // Refresh the sources list
	}

	function closeModal() {
		activeModal = null;
	}
</script>

<Page>
	<div class="max-w-7xl">
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
						<h3 class="text-lg font-medium text-foreground mb-2">
							No sources connected
						</h3>
						<p class="text-foreground-muted">
							Choose from the available sources below to start syncing your personal data.
						</p>
					</div>
				{:else}
					<div class="border border-border rounded-lg overflow-hidden">
						<table class="w-full">
							<thead class="bg-surface-elevated border-b border-border">
								<tr>
									<th class="px-6 py-4 text-left text-xs font-medium text-foreground-subtle uppercase tracking-wide">Source</th>
									<th class="px-6 py-4 text-left text-xs font-medium text-foreground-subtle uppercase tracking-wide">Type</th>
									<th class="px-6 py-4 text-left text-xs font-medium text-foreground-subtle uppercase tracking-wide">Status</th>
									<th class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase tracking-wide">Streams</th>
									<th class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase tracking-wide">Records</th>
									<th class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase tracking-wide">Size</th>
									<th class="px-6 py-4 text-right text-xs font-medium text-foreground-subtle uppercase tracking-wide">Last Sync</th>
								</tr>
							</thead>
							<tbody class="divide-y divide-border">
								{#each filteredSources as source}
									{@const stats = getSourceStats(source.id)}
									<tr 
										class="hover:bg-surface-elevated transition-colors cursor-pointer"
										onclick={() => handleSourceClick(source.id)}
									>
										<td class="px-6 py-4">
											<span class="font-serif text-foreground">{source.name}</span>
										</td>
										<td class="px-6 py-4">
											<Badge variant="muted">{getSourceTypeLabel(source.source)}</Badge>
										</td>
										<td class="px-6 py-4">
											<span class="flex items-center gap-2">
												<span class="w-2 h-2 rounded-full {source.is_active ? 'bg-success' : 'bg-foreground-subtle'}"></span>
												<span class="text-sm text-foreground-muted">{source.is_active ? 'Active' : 'Paused'}</span>
											</span>
										</td>
										<td class="px-6 py-4 text-right">
											<span class="text-sm text-foreground-muted">
												{source.enabled_streams_count}/{source.total_streams_count}
											</span>
										</td>
										<td class="px-6 py-4 text-right">
											<span class="text-sm text-foreground-muted">{formatNumber(stats.records)}</span>
										</td>
										<td class="px-6 py-4 text-right">
											<span class="text-sm text-foreground-muted">{formatBytes(stats.bytes)}</span>
										</td>
										<td class="px-6 py-4 text-right">
											<span class="text-sm text-foreground-muted">{formatRelativeTime(source.last_sync_at)}</span>
										</td>
									</tr>
								{/each}
							</tbody>
						</table>
					</div>
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
					class="grid grid-cols-1 sm:grid-cols-2 gap-4"
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
										<Icon
											icon={catalogSource.icon}
											class="text-2xl text-foreground-subtle"
										/>
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
											<Icon icon="ri:check-line"/>
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
										<Icon icon="ri:database-2-line"/>
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
										onclick={() => spaceStore.openTabFromRoute('/profile/account')}
									>
										Upgrade
									</Button>
								{:else}
									<Button
										variant="primary"
										size="sm"
										onclick={() => handleAddSource(catalogSource)}
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

<!-- Source Connection Modals -->
{#if activeModal?.type === 'oauth'}
	<OAuthConnectModal 
		provider={activeModal.provider}
		displayName={activeModal.displayName}
		open={true}
		onClose={closeModal}
	/>
{:else if activeModal?.type === 'plaid'}
	<PlaidConnectModal
		open={true}
		onClose={closeModal}
		onSuccess={handleModalSuccess}
	/>
{:else if activeModal?.type === 'device'}
	<DevicePairModal
		deviceType={activeModal.deviceType}
		displayName={activeModal.displayName}
		open={true}
		onClose={closeModal}
		onSuccess={(sourceId) => handleModalSuccess(sourceId)}
	/>
{/if}
