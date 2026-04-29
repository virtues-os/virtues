<script lang="ts">
	import type { Tab } from "$lib/tabs/types";
	import { Page } from "$lib";
	import Icon from "$lib/components/Icon.svelte";
	import { onMount } from "svelte";


	// @ts-ignore — Vite compile-time constant (see vite.config.ts + app.d.ts)
	const BUILD_COMMIT: string = __BUILD_COMMIT__;

	let { tab, active }: { tab: Tab; active: boolean } = $props();

	let loading = $state(true);

	// Health endpoint data
	let serverStatus = $state("unknown");
	let version = $state("");
	let commit = $state("");
	let builtAt = $state("");
	let database = $state("unknown");
	let poolSize = $state(0);
	let poolIdle = $state(0);

	function formatBuildTime(iso: string): string {
		if (!iso) return "";
		const date = new Date(iso);
		return date.toLocaleDateString([], {
			year: "numeric",
			month: "long",
			day: "numeric",
			hour: "numeric",
			minute: "2-digit",
		});
	}

	onMount(async () => {
		await Promise.all([loadHealth(), loadProfile()]);
		loading = false;
	});

	async function loadHealth() {
		try {
			const response = await fetch("/health");
			if (response.ok) {
				const data = await response.json();
				serverStatus = data.status || "unknown";
				version = data.version || "";
				commit = data.commit || BUILD_COMMIT;
				builtAt = data.built_at || "";
				database = data.database || "unknown";
				poolSize = data.pool?.size ?? 0;
				poolIdle = data.pool?.idle ?? 0;
			}
		} catch (error) {
			console.error("Failed to load health info:", error);
		}
	}

	async function loadProfile() {
		try {
			const response = await fetch("/api/profile");
			if (!response.ok) {
				console.error("Failed to load profile:", response.statusText);
			}
		} catch (error) {
			console.error("Failed to load profile:", error);
		}
	}

</script>

<Page>
	<div class="max-w-3xl">
		<div class="mb-8">
			<h1 class="text-3xl font-serif font-medium text-foreground mb-2">
				System
			</h1>
			<p class="text-foreground-muted">
				Server version, status, and update preferences
			</p>
		</div>

		{#if loading}
			<div class="flex items-center justify-center h-64">
				<Icon icon="ri:loader-4-line" width="20" class="spin" />
			</div>
		{:else}
			<div class="space-y-8">
				<!-- Version -->
				<section>
					<h2 class="section-title">Version</h2>
					<div class="info-rows">
						<div class="info-row">
							<span class="info-label">Commit</span>
							<code class="info-value mono">{commit}</code>
						</div>
						{#if builtAt}
							<div class="info-row">
								<span class="info-label">Built</span>
								<span class="info-value"
									>{formatBuildTime(builtAt)}</span
								>
							</div>
						{/if}
						{#if version}
							<div class="info-row">
								<span class="info-label">Package</span>
								<span class="info-value">{version}</span>
							</div>
						{/if}
					</div>
				</section>

				<!-- Server -->
				<section>
					<h2 class="section-title">Server</h2>
					<div class="info-rows">
						<div class="info-row">
							<span class="info-label">Status</span>
							<span
								class="info-value status"
								class:healthy={serverStatus === "healthy"}
								class:unhealthy={serverStatus !== "healthy"}
							>
								{serverStatus}
							</span>
						</div>
						<div class="info-row">
							<span class="info-label">Database</span>
							<span
								class="info-value status"
								class:healthy={database === "connected"}
								class:unhealthy={database !== "connected"}
							>
								{database}
							</span>
						</div>
						<div class="info-row">
							<span class="info-label">Connections</span>
							<span class="info-value"
								>{poolIdle} idle / {poolSize} total</span
							>
						</div>
					</div>
				</section>
			</div>
		{/if}
	</div>
</Page>

<style>
	.section-title {
		font-size: 14px;
		font-weight: 500;
		color: var(--foreground-muted);
		margin-bottom: 12px;
	}

	.info-rows {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.info-row {
		display: flex;
		align-items: baseline;
		gap: 12px;
	}

	.info-label {
		font-size: 13px;
		font-weight: 500;
		color: var(--foreground);
		min-width: 100px;
		flex-shrink: 0;
	}

	.info-value {
		font-size: 13px;
		color: var(--foreground-muted);
		user-select: text;
	}

	.info-value.mono {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		word-break: break-all;
	}

	.info-value.status.healthy {
		color: var(--color-success, #22c55e);
	}

	.info-value.status.unhealthy {
		color: var(--color-error, #ef4444);
	}

	.fields {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.field label {
		font-size: 13px;
		font-weight: 500;
		color: var(--foreground);
	}

	.field-hint {
		font-size: 12px;
		color: var(--foreground-subtle);
	}

</style>
