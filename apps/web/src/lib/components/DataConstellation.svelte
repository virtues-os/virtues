<script lang="ts">
	/**
	 * DataConstellation - Sources × Domains Grid
	 *
	 * A GitHub commit graph-style visualization showing which sources
	 * provide data for which life domains.
	 */

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

	interface Props {
		catalog: CatalogSource[];
		connectedSources: ConnectedSource[];
	}

	let { catalog, connectedSources }: Props = $props();

	// Static mapping of which sources provide which domains
	const SOURCE_DOMAIN_MAP: Record<string, string[]> = {
		google: ["social", "praxis"], // Gmail, Calendar
		ios: ["health", "location", "speech"], // HealthKit, Location, Microphone
		mac: ["social", "activity"], // iMessage, App/Browser usage
		plaid: ["financial"], // Transactions
		notion: ["knowledge"], // Documents
	};

	const DOMAINS = [
		{ id: "health", name: "Health", description: "Sleep, heart rate, workouts" },
		{ id: "location", name: "Location", description: "Places you visit" },
		{ id: "speech", name: "Speech", description: "Voice transcriptions" },
		{ id: "social", name: "Social", description: "Email, messages" },
		{ id: "activity", name: "Activity", description: "App & web usage" },
		{ id: "praxis", name: "Praxis", description: "Calendar events" },
		{ id: "financial", name: "Financial", description: "Transactions" },
		{ id: "knowledge", name: "Knowledge", description: "Documents, notes" },
	];

	// Filter to only show sources that have domain mappings
	let sources = $derived(catalog.filter((s) => SOURCE_DOMAIN_MAP[s.name]));

	// Check if a source is connected
	function isConnected(source: CatalogSource): boolean {
		return connectedSources.some((c) => c.source === source.name);
	}

	// Calculate coverage stats
	let coverage = $derived.by(() => {
		const connectedSourceTypes = new Set(connectedSources.map((s) => s.source));
		const coveredDomains = new Set<string>();

		for (const [source, domains] of Object.entries(SOURCE_DOMAIN_MAP)) {
			if (connectedSourceTypes.has(source)) {
				domains.forEach((d) => coveredDomains.add(d));
			}
		}

		return {
			covered: coveredDomains.size,
			total: DOMAINS.length,
		};
	});
</script>

<div class="data-grid">
	<header class="grid-header">
		<h2>Your Data Universe</h2>
		<p class="grid-description">
			See how your connected sources illuminate different aspects of your life.
		</p>
		<div class="coverage-summary">
			<span class="coverage-stat">
				{coverage.covered} of {coverage.total} domains covered
			</span>
		</div>
	</header>

	<div class="grid-container">
		<!-- Column headers (Sources) -->
		<div class="grid-row header">
			<div class="grid-cell corner"></div>
			{#each sources as source}
				<div class="grid-cell source-header">
					<span class="source-name">{source.display_name}</span>
					{#if isConnected(source)}
						<span class="status-dot connected"></span>
					{:else}
						<span class="status-dot"></span>
					{/if}
				</div>
			{/each}
		</div>

		<!-- Domain rows -->
		{#each DOMAINS as domain}
			<div class="grid-row">
				<div class="grid-cell domain-label">
					<span class="domain-name">{domain.name}</span>
				</div>
				{#each sources as source}
					{@const provides = SOURCE_DOMAIN_MAP[source.name]?.includes(domain.id)}
					{@const active = provides && isConnected(source)}
					<div class="grid-cell">
						{#if provides}
							<span class="dot" class:active></span>
						{/if}
					</div>
				{/each}
			</div>
		{/each}
	</div>

	<!-- Legend -->
	<div class="grid-legend">
		<div class="legend-item">
			<span class="legend-dot active"></span>
			<span class="legend-label">Connected</span>
		</div>
		<div class="legend-item">
			<span class="legend-dot"></span>
			<span class="legend-label">Available</span>
		</div>
	</div>
</div>

<style>
	.data-grid {
		margin-top: 48px;
		padding-top: 32px;
		border-top: 1px solid var(--border);
	}

	.grid-header {
		margin-bottom: 24px;
		text-align: center;
	}

	.grid-header h2 {
		font-family: var(--font-serif);
		font-size: 20px;
		font-weight: 400;
		color: var(--foreground);
		margin: 0 0 8px 0;
	}

	.grid-description {
		font-family: var(--font-sans);
		font-size: 14px;
		color: var(--foreground-muted);
		line-height: 1.5;
		margin: 0 0 12px 0;
	}

	.coverage-summary {
		display: flex;
		justify-content: center;
		align-items: center;
	}

	.coverage-stat {
		font-family: var(--font-mono);
		font-size: 11px;
		letter-spacing: 0.05em;
		color: var(--foreground-subtle);
		text-transform: uppercase;
	}

	/* Grid Container */
	.grid-container {
		display: flex;
		flex-direction: column;
		gap: 0;
		overflow-x: auto;
	}

	.grid-row {
		display: flex;
		align-items: center;
		border-bottom: 1px solid var(--border);
	}

	.grid-row:last-child {
		border-bottom: none;
	}

	.grid-row.header {
		border-bottom: 1px solid var(--border);
	}

	.grid-cell {
		width: 80px;
		min-width: 80px;
		height: 40px;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.grid-cell.corner {
		width: 100px;
		min-width: 100px;
	}

	.grid-cell.domain-label {
		width: 100px;
		min-width: 100px;
		justify-content: flex-end;
		padding-right: 16px;
	}

	.domain-name {
		font-family: var(--font-mono);
		font-size: 11px;
		letter-spacing: 0.05em;
		color: var(--foreground-muted);
		text-transform: uppercase;
	}

	.grid-cell.source-header {
		flex-direction: column;
		gap: 6px;
		height: 56px;
		padding: 8px 0;
	}

	.source-name {
		font-family: var(--font-mono);
		font-size: 10px;
		letter-spacing: 0.03em;
		color: var(--foreground-muted);
		text-align: center;
		line-height: 1.2;
	}

	.status-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--border);
	}

	.status-dot.connected {
		background: var(--success);
		box-shadow: 0 0 6px var(--success);
	}

	/* Grid dots */
	.dot {
		width: 14px;
		height: 14px;
		border-radius: 50%;
		border: 1.5px solid var(--border);
		background: transparent;
		transition: all 200ms ease;
	}

	.dot.active {
		background: var(--success);
		border-color: var(--success);
		box-shadow: 0 0 8px var(--success);
	}

	/* Legend */
	.grid-legend {
		display: flex;
		justify-content: center;
		gap: 24px;
		margin-top: 16px;
		padding-top: 16px;
		border-top: 1px solid var(--border);
	}

	.legend-item {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.legend-dot {
		width: 12px;
		height: 12px;
		border-radius: 50%;
		border: 1.5px solid var(--border);
		background: transparent;
	}

	.legend-dot.active {
		background: var(--success);
		border-color: var(--success);
		box-shadow: 0 0 6px var(--success);
	}

	.legend-label {
		font-family: var(--font-mono);
		font-size: 10px;
		letter-spacing: 0.05em;
		text-transform: uppercase;
		color: var(--foreground-subtle);
	}

	/* Responsive */
	@media (max-width: 600px) {
		.grid-cell {
			width: 64px;
			min-width: 64px;
		}

		.grid-cell.corner,
		.grid-cell.domain-label {
			width: 80px;
			min-width: 80px;
		}

		.source-name {
			font-size: 9px;
		}

		.domain-name {
			font-size: 10px;
		}

		.dot {
			width: 12px;
			height: 12px;
		}
	}
</style>
