<script lang="ts">
	import { Button, Page } from "$lib";

	import "iconify-icon";
	import type { PageData } from "./$types";

	export let data: PageData;

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
		return data.sources.some((s: any) => s.type === catalogSourceName);
	}
</script>

<Page>
	<div class="max-w-7xl">
		<!-- Header -->
		<div class=" flex items-center justify-between">
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
			<a href="/data/sources/add">
				<Button variant="secondary" class="flex items-center gap-2">
					<iconify-icon icon="ri:add-line" class="text-lg"
					></iconify-icon>
					<span>Add Source</span>
				</Button>
			</a>
		</div>

		<!-- Connected Sources Section -->
		<div class="mt-8">
			<h2 class="text-xl font-serif font-medium text-foreground mb-4">
				Connected Sources
				{#if data.sources.length > 0}
					<span class="text-foreground-subtle text-sm font-normal"
						>( {data.sources.length} )</span
					>
				{/if}
			</h2>

			{#if data.sources.length === 0}
				<!-- Empty State -->
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
					<a href="/data/sources/add">
						<Button variant="primary">Add Your First Source</Button>
					</a>
				</div>
			{:else}
				<!-- Connected Sources Cards -->
				<div
					class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4"
				>
					{#each data.sources as source}
						<a
							href="/data/sources/{source.id}"
							class="block p-6 bg-surface border border-border rounded-lg hover:border-border-subtle hover:bg-surface-elevated transition-all duration-200 cursor-pointer"
						>
							<!-- Header with Icon and Status -->
							<div class="flex items-start justify-between mb-4">
								<div class="flex items-center gap-3">
									<h3 class="font-medium text-foreground">
										{source.name}
									</h3>
								</div>
								{#if source.is_active}
									<span
										class="inline-block w-2 h-2 bg-success rounded-full"
										title="Active"
									></span>
								{:else}
									<span
										class="inline-block w-2 h-2 bg-foreground-subtle rounded-full"
										title="Inactive"
									></span>
								{/if}
							</div>

							<!-- Stats -->
							<div class="space-y-2 text-sm">
								<div class="flex items-center justify-between">
									<span class="text-foreground-muted">Streams</span
									>
									<span class="font-medium text-foreground">
										{source.enabled_streams_count} of {source.total_streams_count}
										enabled
									</span>
								</div>
								<div class="flex items-center justify-between">
									<span class="text-foreground-muted"
										>Last sync</span
									>
									<span class="font-medium text-foreground">
										{formatRelativeTime(
											source.last_sync_at,
										)}
									</span>
								</div>
							</div>

							<!-- Footer -->
							<div class="mt-4 pt-4 border-t border-border">
								<span class="text-xs text-foreground-subtle">
									View streams and sync history →
								</span>
							</div>
						</a>
					{/each}
				</div>
			{/if}
		</div>

		<!-- Available Sources Catalog -->
		<div class="pt-8">
			<h2 class="text-xl font-serif font-medium text-foreground mb-2">
				Available Sources <span class="text-foreground-subtle font-normal">
					<span class="text-foreground-subtle text-sm font-normal">
						( {data.catalog.length} )
					</span>
				</span>
			</h2>

			<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
				{#each data.catalog as catalogSource}
					{@const connected = isSourceConnected(catalogSource.name)}
					<div
						class="p-6 bg-surface border border-border rounded-lg hover:border-border-subtle transition-all duration-200"
					>
						<!-- Header with Icon/Title -->
						<div class="flex items-center justify-between mb-3">
							<div class="flex items-center gap-3">
								<h3
									class="font-medium text-foreground capitalize"
								>
									{catalogSource.name}
								</h3>
							</div>

							<!-- Connected Badge -->
							{#if connected}
								<span
									class="inline-flex items-center gap-1 px-2 py-1 text-xs font-medium bg-success-subtle text-success rounded-full"
								>
									<iconify-icon icon="ri:check-line"
									></iconify-icon>
									Connected
								</span>
							{/if}
						</div>

						<p class="text-sm text-foreground-muted mb-4 line-clamp-2">
							{catalogSource.description}
						</p>

						<!-- Metadata -->
						<div class="flex items-center justify-between">
							<div
								class="flex items-center gap-3 text-xs text-foreground-subtle"
							>
								<span class="flex items-center gap-1">
									<iconify-icon icon="ri:database-2-line"
									></iconify-icon>
									{catalogSource.stream_count}
									{catalogSource.stream_count === 1
										? "stream"
										: "streams"}
								</span>
								<span
									class="inline-block px-2 py-0.5 bg-surface-elevated text-foreground-muted rounded-full capitalize"
								>
									{catalogSource.auth_type === "oauth2"
										? "OAuth"
										: catalogSource.auth_type}
								</span>
							</div>
							<a
								href="/data/sources/add?type={catalogSource.name}"
							>
								<Button variant="primary" size="sm">
									{connected ? "Add Another" : "Add"}
								</Button>
							</a>
						</div>
					</div>
				{/each}
			</div>
		</div>
	</div>
</Page>
