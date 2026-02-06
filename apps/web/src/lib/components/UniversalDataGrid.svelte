<!--
	UniversalDataGrid.svelte

	A reusable data grid component with table/card view modes.
	Uses CSS transitions for smooth morphing between layouts.
-->

<script lang="ts" module>
	export interface Column<T> {
		key: keyof T;
		label: string;
		icon?: string;
		width?: string;
		minWidth?: string;
		hideOnMobile?: boolean;
		format?: 'text' | 'badge' | 'date' | 'relative-date' | 'avatar' | 'number';
		badgeColors?: Record<string, string>;
		getValue?: (item: T) => string | number | null | undefined;
	}
</script>

<script lang="ts" generics="T extends { id: string }">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { dataGridPrefs, type ViewMode } from '$lib/stores/dataGridPrefs.svelte';

	interface Props {
		items: T[];
		columns: Column<T>[];
		entityType: string;
		loading?: boolean;
		error?: string | null;
		emptyIcon?: string;
		emptyMessage?: string;
		loadingMessage?: string;
		searchPlaceholder?: string;
		pageSize?: number;
		onItemClick?: (item: T) => void;
		onRetry?: () => void;
		// Custom renderers
		tableRow?: Snippet<[T]>;
		card?: Snippet<[T]>;
	}

	let {
		items,
		columns,
		entityType,
		loading = false,
		error = null,
		emptyIcon = 'ri:database-2-line',
		emptyMessage = 'No items yet',
		loadingMessage = 'Loading...',
		searchPlaceholder = 'Search...',
		pageSize = 16,
		onItemClick,
		onRetry,
		tableRow,
		card,
	}: Props = $props();

	// Search state
	let searchQuery = $state('');

	// Helper to get value from item for a column
	function getValue(item: T, col: Column<T>): string {
		if (col.getValue) {
			const val = col.getValue(item);
			return val != null ? String(val) : '';
		}
		const val = item[col.key];
		return val != null ? String(val) : '';
	}

	// Filter items by search query (searches all columns)
	const filteredItems = $derived.by(() => {
		if (!searchQuery.trim()) return items;
		const q = searchQuery.toLowerCase();
		return items.filter(item => {
			for (const col of columns) {
				const val = getValue(item, col);
				if (val && val.toLowerCase().includes(q)) {
					return true;
				}
			}
			return false;
		});
	});

	// Apply page size limit
	const displayedItems = $derived(filteredItems.slice(0, pageSize));
	const totalCount = $derived(items.length);
	const filteredCount = $derived(filteredItems.length);
	const displayedCount = $derived(displayedItems.length);

	// View mode: 'table' or 'grid' only
	// Initialize to 'table', then sync from preferences in effect
	let viewMode = $state<ViewMode>('table');

	// Sync view mode when entityType changes
	$effect(() => {
		viewMode = dataGridPrefs.getViewMode(entityType);
	});

	function setViewMode(mode: ViewMode) {
		// Normalize 'list' to 'table' since we only support 2 modes now
		const normalized = mode === 'list' ? 'table' : mode;
		viewMode = normalized;
		dataGridPrefs.setViewMode(entityType, normalized);
	}

	function handleKeyDown(e: KeyboardEvent, item: T) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			onItemClick?.(item);
		}
	}

	function getItemLabel(item: T): string {
		if (columns.length > 0) {
			return getValue(item, columns[0]) || 'Item';
		}
		return 'Item';
	}

	function getBadgeClass(value: string, col: Column<T>): string {
		if (!col.badgeColors) return 'badge-gray';
		return col.badgeColors[value.toLowerCase()] || 'badge-gray';
	}

	const isTable = $derived(viewMode === 'table');
	const isGrid = $derived(viewMode === 'grid');
</script>

<div class="datagrid-wrapper">
	<!-- Toolbar -->
	<div class="datagrid-toolbar">
		<div class="toolbar-left">
			<div class="search-container">
				<Icon icon="ri:search-line" width="16" />
				<input
					type="text"
					bind:value={searchQuery}
					placeholder={searchPlaceholder}
					class="search-input"
				/>
				{#if searchQuery}
					<button class="search-clear" onclick={() => searchQuery = ''}>
						<Icon icon="ri:close-line" width="16" />
					</button>
				{/if}
			</div>
			<span class="item-count">
				{#if searchQuery && filteredCount !== totalCount}
					{displayedCount} of {filteredCount} results
				{:else if displayedCount < totalCount}
					{displayedCount} of {totalCount}
				{:else}
					{totalCount} {totalCount === 1 ? 'item' : 'items'}
				{/if}
			</span>
		</div>

		<div class="view-toggle" role="group" aria-label="View options">
			<button
				class="view-btn"
				class:active={isTable}
				onclick={() => setViewMode('table')}
				aria-pressed={isTable}
				aria-label="Table view"
			>
				<Icon icon="ri:list-check-2" width="16" />
			</button>
			<button
				class="view-btn"
				class:active={isGrid}
				onclick={() => setViewMode('grid')}
				aria-pressed={isGrid}
				aria-label="Card view"
			>
				<Icon icon="ri:layout-grid-line" width="16" />
			</button>
		</div>
	</div>

	{#if loading}
		<div class="loading-state" role="status" aria-live="polite">
			<Icon icon="ri:loader-4-line" width="24" />
			<span>{loadingMessage}</span>
		</div>
	{:else if error}
		<div class="error-state" role="alert">
			<Icon icon="ri:error-warning-line" width="24" />
			<span>{error}</span>
			{#if onRetry}
				<button class="retry-btn" onclick={onRetry}>Retry</button>
			{/if}
		</div>
	{:else if items.length === 0}
		<div class="empty-state">
			<Icon icon={emptyIcon} width="32" />
			<p>{emptyMessage}</p>
		</div>
	{:else if displayedItems.length === 0}
		<div class="empty-state">
			<Icon icon="ri:search-line" width="32" />
			<p>No results for "{searchQuery}"</p>
			<button class="clear-search-btn" onclick={() => searchQuery = ''}>Clear search</button>
		</div>
	{:else}
		<!-- Table View -->
		{#if isTable}
			<div class="table-view">
				<table class="data-table">
					<thead>
						<tr>
							{#each columns as col}
								<th
									class:hide-mobile={col.hideOnMobile}
									style:width={col.width}
									style:min-width={col.minWidth}
								>
									<span class="th-content">
										{#if col.icon}
											<Icon icon={col.icon} width="14" />
										{/if}
										<span>{col.label}</span>
									</span>
								</th>
							{/each}
						</tr>
					</thead>
					<tbody>
						{#each displayedItems as item (item.id)}
							<tr
								class="data-row"
								onclick={() => onItemClick?.(item)}
								onkeydown={(e) => handleKeyDown(e, item)}
								tabindex="0"
								role="button"
								aria-label={`Open ${getItemLabel(item)}`}
							>
								{#if tableRow}
									{@render tableRow(item)}
								{:else}
									{#each columns as col}
										<td class:hide-mobile={col.hideOnMobile}>
											{#if col.format === 'badge'}
												{@const value = getValue(item, col)}
												{#if value}
													<span class="badge {getBadgeClass(value, col)}">{value}</span>
												{:else}
													<span class="empty-cell">—</span>
												{/if}
											{:else}
												{@const value = getValue(item, col)}
												{#if value}
													<span class="cell-text">{value}</span>
												{:else}
													<span class="empty-cell">—</span>
												{/if}
											{/if}
										</td>
									{/each}
								{/if}
							</tr>
						{/each}
					</tbody>
				</table>
			</div>

		<!-- Card Grid View -->
		{:else}
			<div class="card-grid">
				{#each displayedItems as item (item.id)}
					<button
						class="card"
						onclick={() => onItemClick?.(item)}
						onkeydown={(e) => handleKeyDown(e, item)}
						aria-label={`Open ${getItemLabel(item)}`}
					>
						{#if card}
							{@render card(item)}
						{:else}
							<div class="card-content">
								{#each columns.slice(0, 2) as col, i}
									{@const value = getValue(item, col)}
									{#if i === 0}
										<span class="card-title">{value || '—'}</span>
									{:else if col.format === 'badge' && value}
										<span class="badge {getBadgeClass(value, col)}">{value}</span>
									{:else if value}
										<span class="card-meta">{value}</span>
									{/if}
								{/each}
							</div>
						{/if}
					</button>
				{/each}
			</div>
		{/if}
	{/if}
</div>

<style>
	.datagrid-wrapper {
		width: 100%;
		padding: 0 2rem;
	}

	/* Toolbar */
	.datagrid-toolbar {
		display: flex;
		justify-content: flex-end;
		align-items: center;
		padding: 0.5rem 0;
		position: relative;
	}

	.datagrid-toolbar::after {
		content: '';
		position: absolute;
		left: -2rem;
		right: -2rem;
		bottom: 0;
		height: 1px;
		background: var(--color-border);
	}

	.toolbar-left {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	/* Search */
	.search-container {
		position: relative;
		display: flex;
		align-items: center;
		max-width: 240px;
	}

	.search-container > :global(svg:first-child) {
		position: absolute;
		left: 0.625rem;
		color: var(--color-foreground-subtle);
		pointer-events: none;
	}

	.search-input {
		width: 100%;
		padding: 0.5rem 2rem 0.5rem 2rem;
		background: var(--color-background-secondary);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		color: var(--color-foreground);
		font-size: 0.8125rem;
		transition: border-color 0.15s ease, box-shadow 0.15s ease;
	}

	.search-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.search-input:focus {
		outline: none;
		border-color: var(--color-primary);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-primary) 20%, transparent);
	}

	.search-clear {
		position: absolute;
		right: 0.375rem;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 20px;
		height: 20px;
		background: transparent;
		border: none;
		border-radius: 4px;
		color: var(--color-foreground-subtle);
		cursor: pointer;
	}

	.search-clear:hover {
		color: var(--color-foreground);
		background: var(--color-background-hover);
	}

	.item-count {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		white-space: nowrap;
	}

	.clear-search-btn {
		margin-top: 0.5rem;
		padding: 0.375rem 0.75rem;
		font-size: 0.8125rem;
		color: var(--color-primary);
		background: transparent;
		border: 1px solid var(--color-primary);
		border-radius: 6px;
		cursor: pointer;
	}

	.clear-search-btn:hover {
		background: color-mix(in srgb, var(--color-primary) 10%, transparent);
	}

	/* View Toggle */
	.view-toggle {
		display: flex;
		gap: 2px;
		background: var(--color-background-secondary);
		border-radius: 6px;
		padding: 2px;
	}

	.view-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		background: transparent;
		border: none;
		border-radius: 4px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.view-btn:hover {
		color: var(--color-foreground);
		background: var(--color-background-hover);
	}

	.view-btn:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	.view-btn.active {
		background: var(--color-background);
		color: var(--color-foreground);
		box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
	}

	/* States */
	.loading-state,
	.error-state,
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 0.75rem;
		padding: 3rem 2rem;
		color: var(--color-foreground-muted);
	}

	.loading-state :global(svg) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	.retry-btn {
		padding: 0.375rem 0.75rem;
		font-size: 0.8125rem;
		color: var(--color-primary);
		background: transparent;
		border: 1px solid var(--color-primary);
		border-radius: 6px;
		cursor: pointer;
	}

	.retry-btn:hover {
		background: color-mix(in srgb, var(--color-primary) 10%, transparent);
	}

	.empty-state :global(svg) {
		opacity: 0.5;
	}

	.empty-state p {
		margin: 0;
		font-size: 0.875rem;
	}

	/* ============================================
	   TABLE VIEW
	   ============================================ */
	.table-view {
		animation: fadeIn 0.15s ease;
	}

	@keyframes fadeIn {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	.data-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.8125rem;
	}

	thead tr {
		position: relative;
	}

	thead tr::after {
		content: '';
		position: absolute;
		left: -2rem;
		right: -2rem;
		bottom: 0;
		height: 1px;
		background: var(--color-border);
	}

	th {
		text-align: left;
		font-weight: 500;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		padding: 0.625rem 0.75rem;
		white-space: nowrap;
	}

	th:first-child {
		padding-left: 0;
	}

	th:last-child {
		padding-right: 0;
	}

	/* Fix: inline-flex for icon + label */
	.th-content {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
	}

	.th-content :global(svg) {
		opacity: 0.7;
		flex-shrink: 0;
	}

	td {
		padding: 0.625rem 0.75rem;
		color: var(--color-foreground);
		vertical-align: middle;
	}

	td:first-child {
		padding-left: 0;
	}

	td:last-child {
		padding-right: 0;
	}

	.data-row {
		cursor: pointer;
		position: relative;
		transition: background-color 0.1s ease;
	}

	.data-row::after {
		content: '';
		position: absolute;
		left: -2rem;
		right: -2rem;
		bottom: 0;
		height: 1px;
		background: var(--color-border);
	}

	.data-row:hover {
		background: var(--color-background-hover);
	}

	.data-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
		background: var(--color-background-hover);
	}

	.cell-text {
		color: var(--color-foreground);
	}

	td:first-child .cell-text {
		font-weight: 500;
	}

	/* ============================================
	   CARD GRID VIEW
	   ============================================ */
	.card-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
		gap: 1rem;
		padding-top: 1rem;
		animation: fadeIn 0.15s ease;
	}

	.card {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		min-height: 140px;
		padding: 1.5rem 1.25rem;
		background: var(--color-background-secondary);
		border: 1px solid var(--color-border);
		border-radius: 12px;
		cursor: pointer;
		transition: all 0.2s ease;
		text-align: left;
		width: 100%;
		font: inherit;
		color: inherit;
	}

	.card:hover {
		background: var(--color-background-hover);
		border-color: var(--color-border-strong);
		transform: translateY(-2px);
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.08);
	}

	.card:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	.card:active {
		transform: translateY(0);
	}

	.card-content {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		text-align: center;
		width: 100%;
	}

	.card-title {
		font-weight: 600;
		font-size: 0.9375rem;
		color: var(--color-foreground);
		line-height: 1.3;
	}

	.card-meta {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}

	/* ============================================
	   SHARED STYLES
	   ============================================ */
	.badge {
		display: inline-flex;
		align-items: center;
		padding: 0.125rem 0.5rem;
		font-size: 0.75rem;
		font-weight: 500;
		border-radius: 9999px;
		white-space: nowrap;
		text-transform: capitalize;
	}

	.badge-gray {
		background: color-mix(in srgb, var(--color-foreground) 10%, transparent);
		color: var(--color-foreground-muted);
	}

	.badge-blue {
		background: color-mix(in srgb, #3b82f6 15%, transparent);
		color: #2563eb;
	}

	.badge-green {
		background: color-mix(in srgb, #22c55e 15%, transparent);
		color: #16a34a;
	}

	.badge-purple {
		background: color-mix(in srgb, #8b5cf6 15%, transparent);
		color: #7c3aed;
	}

	.badge-orange {
		background: color-mix(in srgb, #f97316 15%, transparent);
		color: #ea580c;
	}

	.badge-pink {
		background: color-mix(in srgb, #ec4899 15%, transparent);
		color: #db2777;
	}

	.empty-cell {
		color: var(--color-foreground-subtle);
	}

	/* Responsive */
	.hide-mobile {
		display: table-cell;
	}

	@media (max-width: 768px) {
		.hide-mobile {
			display: none;
		}

		.card-grid {
			grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
		}
	}
</style>
