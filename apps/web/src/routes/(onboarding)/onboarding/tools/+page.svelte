<script lang="ts">
	import { getContext, onMount } from "svelte";
	import { toast } from "svelte-sonner";

	import { page } from "$app/stores";
	import * as api from "$lib/api/client";
	import SourceConnector from "$lib/components/SourceConnector.svelte";

	interface CatalogSource {
		name: string;
		display_name: string;
		description: string;
		auth_type: string;
		stream_count: number;
		icon?: string;
	}

	interface ConnectedSource {
		id: string;
		source: string; // Source type: "google", "ios", "notion", etc.
		name: string; // User-given instance name
	}

	// Get onboarding context to control continue button
	const { setCanContinue } = getContext<{
		setCanContinue: (value: boolean) => void;
	}>("onboarding");

	let catalog = $state<CatalogSource[]>([]);
	let connectedSources = $state<ConnectedSource[]>([]);
	let isLoading = $state(true);

	// TODO: Re-enable requirements as we publish more sources
	// // Check if minimum connection is met: iOS (mobile)
	// let hasMobile = $derived(connectedSources.some((s) => s.source === "ios"));

	// Always allow continue for now (only Google available)
	$effect(() => {
		setCanContinue(true);
	});

	onMount(async () => {
		try {
			const [catalogData, sourcesData] = await Promise.all([
				api.listCatalogSources(),
				api.listSources(),
			]);
			// Filter out internal sources (auth_type: none)
			catalog = catalogData.filter(
				(s: CatalogSource) => s.auth_type !== "none",
			);
			connectedSources = sourcesData;

			// Check if we just returned from OAuth
			const params = $page.url.searchParams;
			if (params.get("connected") === "true") {
				toast.success("Source connected successfully");
				// Clean up URL params
				const url = new URL(window.location.href);
				url.searchParams.delete("connected");
				url.searchParams.delete("source_id");
				window.history.replaceState({}, "", url.pathname);
			}
		} catch (e) {
			console.error("Failed to load sources:", e);
		} finally {
			isLoading = false;
		}
	});

	async function handleSourceConnected(
		_sourceId: string,
		sourceName: string,
	) {
		// Show success toast
		toast.success(`${sourceName} connected`);

		// Refresh connected sources
		try {
			const sources = await api.listSources();
			connectedSources = sources;
		} catch (e) {
			console.error("Failed to refresh sources:", e);
		}
	}
</script>

<div class="tools-step max-w-xl mx-auto">
	<header class="step-header">
		<h1>Build Your Library</h1>
		<p class="step-description">
			Your life leaves traces across many places—your phone, your
			calendar, your conversations. Each connection adds another chapter
			to the library of your life, helping your Personal AI know you more
			completely.
		</p>
	</header>

	{#if isLoading}
		<div class="loading-state">
			<span class="loading-text">Loading sources...</span>
		</div>
	{:else}
		<SourceConnector
			{catalog}
			{connectedSources}
			variant="manifest"
			onSourceConnected={handleSourceConnected}
		/>

		<!-- TODO: Re-enable requirement indicator as we publish more sources -->
		<!-- <div class="requirement-section">
			<div class="requirement-status">
				<span class="status-text">
					Connect your mobile device to continue. At a minimum, you
					should connect your calendar, laptop, phone, task manager
					apps, and email.
				</span>
				<div class="requirement-checklist">
					<div class="checklist-item" class:checked={hasMobile}>
						<span class="check-icon">{hasMobile ? "✓" : "○"}</span>
						<span>Mobile device (iOS)</span>
					</div>
				</div>
			</div>
		</div> -->
	{/if}
</div>

<style>
	.tools-step {
		width: 100%;
		margin: 0 auto;
	}

	.step-header {
		margin-bottom: 32px;
	}

	h1 {
		font-family: var(--font-serif);
		font-size: 28px;
		font-weight: 400;
		letter-spacing: -0.02em;
		color: var(--foreground);
		margin-bottom: 12px;
	}

	.step-description {
		font-family: var(--font-sans);
		font-size: 15px;
		color: var(--foreground-muted);
		line-height: 1.6;
	}

	.loading-state {
		padding: 48px 0;
		text-align: center;
	}

	.loading-text {
		font-family: var(--font-sans);
		font-size: 14px;
		color: var(--foreground-subtle);
	}

	/* Requirement section */
	.requirement-section {
		margin-top: 32px;
		padding-top: 24px;
		border-top: 1px solid var(--border);
	}

	.requirement-status {
		text-align: center;
	}

	.status-text {
		font-family: var(--font-sans);
		font-size: 14px;
		color: var(--foreground-muted);
		line-height: 1.5;
	}

	.requirement-checklist {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin-top: 16px;
		padding: 16px;
		background: var(--surface-elevated);
		border-radius: 8px;
	}

	.checklist-item {
		display: flex;
		align-items: center;
		gap: 10px;
		font-family: var(--font-sans);
		font-size: 14px;
		color: var(--foreground-muted);
	}

	.checklist-item.checked {
		color: var(--success);
	}

	.check-icon {
		font-family: var(--font-mono);
		font-size: 14px;
		width: 18px;
		text-align: center;
	}
</style>
