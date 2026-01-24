<script lang="ts">
	import type { Citation } from "$lib/types/Citation";
	import "iconify-icon";

	let {
		citation = null,
		open = false,
		onClose,
	} = $props<{
		citation: Citation | null;
		open: boolean;
		onClose: () => void;
	}>();

	let panelEl: HTMLElement | null = $state(null);
	let closeButtonEl: HTMLButtonElement | null = $state(null);

	// Focus the close button when panel opens
	$effect(() => {
		if (open && closeButtonEl) {
			// Small delay to ensure DOM is ready
			requestAnimationFrame(() => {
				closeButtonEl?.focus();
			});
		}
	});

	// Format source type for display
	function formatSourceType(type: string): string {
		switch (type) {
			case "ontology":
				return "Personal Data";

			case "web_search":
				return "Web Search";
			case "narratives":
				return "Life Narratives";
			case "location":
				return "Location Data";
			default:
				return "Data Source";
		}
	}

	// Format tool name for display
	function formatToolName(name: string): string {
		return name
			.replace("virtues_", "")
			.replace(/_/g, " ")
			.replace(/\b\w/g, (l) => l.toUpperCase());
	}

	// Check if data is tabular (has rows)
	function isTabularData(
		data: unknown,
	): data is { rows: Record<string, unknown>[] } {
		if (!data || typeof data !== "object") return false;
		const d = data as Record<string, unknown>;
		return Array.isArray(d.rows) && d.rows.length > 0;
	}

	// Check if data is web search results
	function isWebSearchData(data: unknown): data is {
		results: Array<{ title: string; url: string; summary?: string }>;
	} {
		if (!data || typeof data !== "object") return false;
		const d = data as Record<string, unknown>;
		return Array.isArray(d.results) && d.results.length > 0;
	}

	// Get table headers from first row
	function getTableHeaders(rows: Record<string, unknown>[]): string[] {
		if (rows.length === 0) return [];
		return Object.keys(rows[0]).filter((key) => !key.startsWith("_"));
	}

	// Format cell value for display
	function formatCellValue(value: unknown): string {
		if (value === null || value === undefined) return "-";
		if (typeof value === "boolean") return value ? "Yes" : "No";
		if (typeof value === "number") {
			// Format numbers nicely
			if (Number.isInteger(value)) return value.toLocaleString();
			return value.toFixed(2);
		}
		if (value instanceof Date) return value.toLocaleDateString();
		if (typeof value === "string") {
			// Check if it's an ISO date
			if (/^\d{4}-\d{2}-\d{2}T/.test(value)) {
				return new Date(value).toLocaleString();
			}
			// Truncate long strings
			if (value.length > 100) return value.slice(0, 100) + "...";
			return value;
		}
		if (typeof value === "object")
			return JSON.stringify(value).slice(0, 100);
		return String(value);
	}

	// Handle backdrop click
	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			onClose();
		}
	}

	// Handle keyboard navigation (Escape to close, Tab to trap focus)
	function handleKeyDown(e: KeyboardEvent) {
		if (e.key === "Escape") {
			onClose();
			return;
		}

		// Focus trap: keep Tab within panel
		if (e.key === "Tab" && panelEl) {
			const focusableElements = panelEl.querySelectorAll<HTMLElement>(
				'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
			);
			const firstElement = focusableElements[0];
			const lastElement = focusableElements[focusableElements.length - 1];

			if (e.shiftKey && document.activeElement === firstElement) {
				e.preventDefault();
				lastElement?.focus();
			} else if (!e.shiftKey && document.activeElement === lastElement) {
				e.preventDefault();
				firstElement?.focus();
			}
		}
	}

	// Validate URL to prevent XSS via javascript: protocol
	function isValidUrl(url: string): boolean {
		try {
			const parsed = new URL(url);
			return ["http:", "https:"].includes(parsed.protocol);
		} catch {
			return false;
		}
	}
</script>

<svelte:window onkeydown={handleKeyDown} />

{#if open && citation}
	<!-- Backdrop -->
	<div
		class="panel-backdrop"
		onclick={handleBackdropClick}
		role="presentation"
	>
		<!-- Panel -->
		<div
			bind:this={panelEl}
			class="citation-panel"
			role="dialog"
			aria-label="Citation details"
			aria-modal="true"
		>
			<!-- Header -->
			<header class="panel-header">
				<div class="header-content">
					<iconify-icon
						icon={citation.icon}
						class={citation.color}
						width="24"
						height="24"
					></iconify-icon>
					<div class="header-text">
						<h2 class="panel-title">{citation.label}</h2>
						<span class="panel-subtitle"
							>{formatSourceType(citation.source_type)}</span
						>
					</div>
				</div>
				<button
					bind:this={closeButtonEl}
					class="close-button"
					onclick={onClose}
					aria-label="Close panel"
				>
					<iconify-icon icon="ri:close-line" width="20" height="20"
					></iconify-icon>
				</button>
			</header>

			<!-- Content -->
			<div class="panel-content">
				<!-- Preview -->
				<div class="section">
					<h3 class="section-title">Summary</h3>
					<p class="preview-text">{citation.preview}</p>
				</div>

				<!-- Tool info (for debugging/power users) -->
				<div class="section">
					<h3 class="section-title">Source Details</h3>
					<div class="detail-grid">
						<div class="detail-item">
							<span class="detail-label">Tool</span>
							<span class="detail-value"
								>{formatToolName(citation.tool_name)}</span
							>
						</div>
						{#if citation.timestamp}
							<div class="detail-item">
								<span class="detail-label">Retrieved</span>
								<span class="detail-value"
									>{new Date(
										citation.timestamp,
									).toLocaleTimeString()}</span
								>
							</div>
						{/if}
					</div>
				</div>

				<!-- Web search results -->
				{#if isWebSearchData(citation.data)}
					<div class="section">
						<h3 class="section-title">Search Results</h3>
						<ul class="search-results">
							{#each citation.data.results as result, i}
								<li class="search-result">
									{#if isValidUrl(result.url)}
										<a
											href={result.url}
											target="_blank"
											rel="noopener noreferrer"
										>
											<span class="result-title"
												>{result.title}</span
											>
											<span class="result-url"
												>{result.url}</span
											>
										</a>
									{:else}
										<div class="invalid-url">
											<span class="result-title"
												>{result.title}</span
											>
											<span class="result-url invalid"
												>Invalid URL</span
											>
										</div>
									{/if}
									{#if result.summary}
										<p class="result-summary">
											{result.summary}
										</p>
									{/if}
								</li>
							{/each}
						</ul>
					</div>
				{/if}

				<!-- Tabular data -->
				{#if isTabularData(citation.data)}
					{@const headers = getTableHeaders(citation.data.rows)}
					{@const rows = citation.data.rows.slice(0, 20)}
					<div class="section">
						<h3 class="section-title">
							Data ({citation.data.rows.length} record{citation
								.data.rows.length !== 1
								? "s"
								: ""})
						</h3>
						<div class="table-wrapper">
							<table class="data-table">
								<thead>
									<tr>
										{#each headers as header}
											<th>{header.replace(/_/g, " ")}</th>
										{/each}
									</tr>
								</thead>
								<tbody>
									{#each rows as row}
										<tr>
											{#each headers as header}
												<td
													>{formatCellValue(
														row[header],
													)}</td
												>
											{/each}
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
						{#if citation.data.rows.length > 20}
							<p class="table-note">
								Showing first 20 of {citation.data.rows.length} records
							</p>
						{/if}
					</div>
				{/if}

				<!-- Raw query (for debugging) -->
				{#if citation.args?.query}
					<details class="section query-section">
						<summary class="section-title clickable"
							>Query Used</summary
						>
						<pre class="query-code">{citation.args.query}</pre>
					</details>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	.panel-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.3);
		z-index: 100;
		display: flex;
		justify-content: flex-end;
		animation: backdrop-fade-in 0.2s ease-out;
	}

	@keyframes backdrop-fade-in {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}

	.citation-panel {
		width: 100%;
		max-width: 480px;
		height: 100%;
		background: var(--color-surface);
		border-left: 1px solid var(--color-border);
		display: flex;
		flex-direction: column;
		animation: panel-slide-in 0.25s ease-out;
		overflow: hidden;
	}

	@keyframes panel-slide-in {
		from {
			transform: translateX(100%);
		}
		to {
			transform: translateX(0);
		}
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px 20px;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-surface-elevated);
	}

	.header-content {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.header-text {
		display: flex;
		flex-direction: column;
	}

	.panel-title {
		font-size: 1rem;
		font-weight: 600;
		color: var(--color-foreground);
		margin: 0;
	}

	.panel-subtitle {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		text-transform: uppercase;
		letter-spacing: 0.025em;
	}

	.close-button {
		padding: 8px;
		border: none;
		background: transparent;
		color: var(--color-foreground-muted);
		cursor: pointer;
		border-radius: 6px;
		transition: all 0.15s;
	}

	.close-button:hover {
		background: var(--color-border-subtle);
		color: var(--color-foreground);
	}

	.panel-content {
		flex: 1;
		overflow-y: auto;
		padding: 20px;
	}

	.section {
		margin-bottom: 24px;
	}

	.section-title {
		font-size: 0.75rem;
		font-weight: 600;
		color: var(--color-foreground-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin: 0 0 8px 0;
	}

	.section-title.clickable {
		cursor: pointer;
	}

	.preview-text {
		font-size: 0.875rem;
		color: var(--color-foreground);
		line-height: 1.6;
		margin: 0;
	}

	.detail-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: 12px;
	}

	.detail-item {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.detail-label {
		font-size: 0.6875rem;
		color: var(--color-foreground-subtle);
		text-transform: uppercase;
		letter-spacing: 0.025em;
	}

	.detail-value {
		font-size: 0.8125rem;
		color: var(--color-foreground);
	}

	/* Search results */
	.search-results {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.search-result {
		padding: 12px;
		background: var(--color-surface-elevated);
		border-radius: 8px;
		border: 1px solid var(--color-border);
	}

	.search-result a {
		text-decoration: none;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.result-title {
		font-size: 0.875rem;
		font-weight: 500;
		color: var(--color-primary);
	}

	.result-url {
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		word-break: break-all;
	}

	.result-url.invalid {
		color: var(--color-error);
	}

	.invalid-url {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.result-summary {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		margin: 8px 0 0 0;
		line-height: 1.5;
	}

	/* Table */
	.table-wrapper {
		overflow-x: auto;
		border: 1px solid var(--color-border);
		border-radius: 8px;
	}

	.data-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8125rem;
	}

	.data-table th,
	.data-table td {
		padding: 8px 12px;
		text-align: left;
		border-bottom: 1px solid var(--color-border);
		white-space: nowrap;
	}

	.data-table th {
		background: var(--color-surface-elevated);
		font-weight: 600;
		font-size: 0.75rem;
		text-transform: capitalize;
		color: var(--color-foreground-muted);
	}

	.data-table tr:last-child td {
		border-bottom: none;
	}

	.table-note {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		margin: 8px 0 0 0;
		text-align: center;
	}

	/* Query section */
	.query-section {
		border: 1px solid var(--color-border);
		border-radius: 8px;
		padding: 12px;
	}

	.query-section summary {
		margin-bottom: 0;
	}

	.query-section[open] summary {
		margin-bottom: 12px;
	}

	.query-code {
		font-family: "IBM Plex Mono", monospace;
		font-size: 0.75rem;
		background: var(--color-surface-elevated);
		padding: 12px;
		border-radius: 6px;
		overflow-x: auto;
		margin: 0;
		white-space: pre-wrap;
		word-break: break-word;
	}
</style>
