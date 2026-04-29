<!--
	UniversalDataGrid.svelte

	Reusable data grid with table/card view modes, built-in sort, density,
	and a diagonal top-left → bottom-right opacity stagger on mount.
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
		sortable?: boolean;
	}

	export interface RowMeta {
		rowIndex: number;
		colIndex: number;
		total: number;
	}

	export type SortDir = 'asc' | 'desc' | null;
</script>

<script lang="ts" generics="T extends { id: string }">
	import type { Snippet } from 'svelte';
	import Icon from '$lib/components/Icon.svelte';
	import { dataGridPrefs, type ViewMode, type Density } from '$lib/stores/dataGridPrefs.svelte';
	import DataGridFilterRail from './DataGridFilterRail.svelte';
	import { useClickOutside } from '$lib/floating';
	import type { FilterDef, FilterOption, FilterValue } from './types';
	import { applyFilter, isFilterActive } from './types';

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
		/** Default view mode when no stored preference exists for this entityType. */
		defaultViewMode?: ViewMode;
		/** Minimum card width in grid mode (CSS value). Default: '200px'. */
		gridMinWidth?: string;
		/** If provided, row click toggles an inline detail row instead of firing onItemClick. */
		expandDetail?: Snippet<[T, RowMeta]>;
		/** Auto-refresh interval in ms. If set, shows a toggle in the toolbar. */
		refreshInterval?: number;
		/** Diagonal fade-in stagger on first paint and on dataset identity change. Default true. */
		animateMount?: boolean;
		/** Enable built-in column sort. Default true. Set false to opt out per-grid. */
		sortable?: boolean;
		/** Declarative filters. Renders a chip rail in the toolbar; results are
		 *  filtered client-side via def.predicate (or equality on def.field). */
		filters?: FilterDef<T>[];
		onItemClick?: (item: T) => void;
		onRefresh?: () => void;
		onRetry?: () => void;
		// Custom renderers — receive RowMeta for stagger / index-aware rendering.
		tableRow?: Snippet<[T, RowMeta]>;
		card?: Snippet<[T, RowMeta]>;
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
		defaultViewMode = 'table',
		gridMinWidth = '200px',
		expandDetail,
		refreshInterval,
		animateMount = true,
		sortable = true,
		filters,
		onItemClick,
		onRefresh,
		onRetry,
		tableRow,
		card
	}: Props = $props();

	// ────────────────────────────────────────────────────────────────────────
	// Filters (declarative: filters[] declares; filterValues holds active state)
	// ────────────────────────────────────────────────────────────────────────
	let filterValues = $state<Record<string, FilterValue>>({});
	let asyncOptionsCache = $state<Record<string, FilterOption[]>>({});
	let filtersInitialized = $state(false);

	$effect(() => {
		if (filtersInitialized) return;
		if (!filters) {
			filtersInitialized = true;
			return;
		}
		const next: Record<string, FilterValue> = {};
		for (const f of filters) {
			if (f.defaultValue !== undefined) next[f.id] = f.defaultValue;
		}
		filterValues = next;
		filtersInitialized = true;
	});

	// Add-filter popover state (lives in toolbar, not in the chip rail)
	let addOpen = $state(false);
	let addBtnEl = $state<HTMLElement | null>(null);
	let addPopoverEl = $state<HTMLElement | null>(null);
	let justAddedId = $state<string | null>(null);

	useClickOutside(
		() => [addBtnEl, addPopoverEl],
		() => (addOpen = false),
		() => addOpen
	);

	const availableFilters = $derived.by(() => {
		if (!filters) return [];
		return filters.filter((f) => !(f.id in filterValues) && !f.hidden);
	});

	const activeFilterCount = $derived.by(() => {
		if (!filters) return 0;
		let n = 0;
		for (const f of filters) {
			if (f.id in filterValues && isFilterActive(filterValues[f.id])) n++;
		}
		return n;
	});

	function addFilter(id: string) {
		const def = filters?.find((f) => f.id === id);
		if (!def) return;
		const seed: FilterValue =
			def.defaultValue !== undefined ? def.defaultValue : def.kind === 'multi' ? [] : null;
		filterValues = { ...filterValues, [id]: seed };
	}

	function pickAddFilter(def: FilterDef<T>) {
		justAddedId = def.id;
		addFilter(def.id);
		addOpen = false;
	}

	function changeFilter(id: string, next: FilterValue) {
		filterValues = { ...filterValues, [id]: next };
	}

	function clearFilter(id: string) {
		const next = { ...filterValues };
		delete next[id];
		filterValues = next;
	}

	async function loadAsyncOptions(id: string): Promise<FilterOption[]> {
		if (asyncOptionsCache[id]) return asyncOptionsCache[id];
		const def = filters?.find((f) => f.id === id);
		if (!def || def.kind !== 'async') return [];
		const opts = await def.loadOptions();
		asyncOptionsCache = { ...asyncOptionsCache, [id]: opts };
		return opts;
	}

	const filteredItems = $derived.by(() => {
		if (!filters || filters.length === 0) return items;
		let out = items;
		for (const f of filters) {
			const v = filterValues[f.id];
			if (v === undefined || !isFilterActive(v)) continue;
			out = out.filter((item) => applyFilter(item, f, v));
		}
		return out;
	});

	// ────────────────────────────────────────────────────────────────────────
	// Search
	// ────────────────────────────────────────────────────────────────────────
	let searchQuery = $state('');

	function getValue(item: T, col: Column<T>): string {
		if (col.getValue) {
			const val = col.getValue(item);
			return val != null ? String(val) : '';
		}
		const val = item[col.key];
		return val != null ? String(val) : '';
	}

	function getRawValue(item: T, col: Column<T>): string | number | null | undefined {
		if (col.getValue) return col.getValue(item);
		const val = item[col.key];
		return val as string | number | null | undefined;
	}

	const searchedItems = $derived.by(() => {
		if (!searchQuery.trim()) return filteredItems;
		const q = searchQuery.toLowerCase();
		return filteredItems.filter((item) => {
			for (const col of columns) {
				const val = getValue(item, col);
				if (val && val.toLowerCase().includes(q)) return true;
			}
			return false;
		});
	});

	// ────────────────────────────────────────────────────────────────────────
	// Sort
	// ────────────────────────────────────────────────────────────────────────
	let sortKey = $state<keyof T | null>(null);
	let sortDir = $state<SortDir>(null);

	function compareValues(a: unknown, b: unknown): number {
		if (a == null && b == null) return 0;
		if (a == null) return 1;
		if (b == null) return -1;
		if (typeof a === 'number' && typeof b === 'number') return a - b;
		const an = Number(a);
		const bn = Number(b);
		if (!Number.isNaN(an) && !Number.isNaN(bn) && a !== '' && b !== '') return an - bn;
		return String(a).localeCompare(String(b), undefined, { numeric: true, sensitivity: 'base' });
	}

	const sortedItems = $derived.by(() => {
		if (!sortable || !sortKey || !sortDir) return searchedItems;
		const col = columns.find((c) => c.key === sortKey);
		if (!col) return searchedItems;
		const dir = sortDir === 'asc' ? 1 : -1;
		const out = [...searchedItems];
		out.sort((a, b) => compareValues(getRawValue(a, col), getRawValue(b, col)) * dir);
		return out;
	});

	function onHeaderSort(col: Column<T>) {
		if (!sortable || col.sortable === false) return;
		if (sortKey !== col.key) {
			sortKey = col.key;
			sortDir = 'asc';
			return;
		}
		if (sortDir === 'asc') {
			sortDir = 'desc';
			return;
		}
		sortKey = null;
		sortDir = null;
	}

	// ────────────────────────────────────────────────────────────────────────
	// Pagination
	// ────────────────────────────────────────────────────────────────────────
	let currentPage = $state(1);
	const totalCount = $derived(items.length);
	const filteredCount = $derived(sortedItems.length);
	const totalPages = $derived(Math.max(1, Math.ceil(filteredCount / pageSize)));
	const displayedItems = $derived(
		sortedItems.slice((currentPage - 1) * pageSize, currentPage * pageSize)
	);

	$effect(() => {
		void searchQuery;
		void items;
		void sortKey;
		void sortDir;
		void filterValues;
		currentPage = 1;
	});

	// ────────────────────────────────────────────────────────────────────────
	// View mode + density (persisted per entityType)
	// ────────────────────────────────────────────────────────────────────────
	let viewMode = $state<ViewMode>('table');
	let density = $state<Density>('comfortable');

	$effect(() => {
		viewMode = dataGridPrefs.hasViewMode(entityType)
			? dataGridPrefs.getViewMode(entityType)
			: defaultViewMode;
		density = dataGridPrefs.hasDensity(entityType)
			? dataGridPrefs.getDensity(entityType)
			: 'comfortable';
	});

	function toggleViewMode() {
		const next: ViewMode = viewMode === 'table' ? 'grid' : 'table';
		viewMode = next;
		dataGridPrefs.setViewMode(entityType, next);
	}

// ────────────────────────────────────────────────────────────────────────
	// Expand detail
	// ────────────────────────────────────────────────────────────────────────
	let expandedId = $state<string | null>(null);

	function handleRowClick(item: T) {
		if (expandDetail) {
			expandedId = expandedId === item.id ? null : item.id;
		} else {
			onItemClick?.(item);
		}
	}

	function handleKeyDown(e: KeyboardEvent, item: T) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			handleRowClick(item);
		}
	}

	$effect(() => {
		void items;
		expandedId = null;
	});

	// ────────────────────────────────────────────────────────────────────────
	// Auto-refresh
	// ────────────────────────────────────────────────────────────────────────
	let autoRefresh = $state(false);

	$effect(() => {
		if (!autoRefresh || !refreshInterval || !onRefresh) return;
		const timer = setInterval(onRefresh, refreshInterval);
		return () => clearInterval(timer);
	});

	// ────────────────────────────────────────────────────────────────────────
	// Mount-stagger key: bumps when dataset identity changes so the
	// {#key mountToken} block remounts and replays the fade. Hover/expand
	// re-renders within the same dataset don't bump it.
	// ────────────────────────────────────────────────────────────────────────
	let mountToken = $state(0);
	let lastIdentity = $state<string>('');

	$effect(() => {
		const fsig = JSON.stringify(filterValues);
		// NOTE: searchQuery intentionally excluded — search shouldn't replay the cascade.
		const sig = `${items.length}|${String(sortKey)}|${String(sortDir)}|${viewMode}|${fsig}`;
		if (sig !== lastIdentity) {
			lastIdentity = sig;
			mountToken++;
		}
	});

	// ────────────────────────────────────────────────────────────────────────
	// Helpers
	// ────────────────────────────────────────────────────────────────────────
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

	function staggerMs(rowIndex: number, colIndex: number): number {
		// Total visible cascade caps at ~1s (cap 600ms stagger + 400ms duration).
		return Math.min((rowIndex * 0.6 + colIndex * 0.4) * 200, 600);
	}

	const isTable = $derived(viewMode === 'table');
	const isGrid = $derived(viewMode === 'grid');
</script>

<div class="datagrid-wrapper" data-density={density}>
	<div class="datagrid-header">
		<div class="datagrid-toolbar">
			<div class="search-container">
				<Icon icon="ri:search-line" width="16" />
				<input
					type="text"
					bind:value={searchQuery}
					placeholder={searchPlaceholder}
					class="search-input"
				/>
				{#if searchQuery}
					<button class="search-clear" onclick={() => (searchQuery = '')}>
						<Icon icon="ri:close-line" width="16" />
					</button>
				{/if}
			</div>

			<div class="toolbar-meta">
				<span class="item-count">
					{#if searchQuery && filteredCount !== totalCount}
						{filteredCount} {filteredCount === 1 ? 'result' : 'results'}
					{:else}
						{totalCount} {totalCount === 1 ? 'item' : 'items'}
					{/if}
				</span>
				{#if refreshInterval && onRefresh}
					<label class="refresh-toggle">
						<input type="checkbox" bind:checked={autoRefresh} />
						<span>Auto-refresh</span>
					</label>
				{/if}
				{#if filters && filters.length > 0 && availableFilters.length > 0}
					<div class="filter-add">
						<button
							class="ctrl-btn"
							class:has-active={activeFilterCount > 0}
							bind:this={addBtnEl}
							onclick={() => (addOpen = !addOpen)}
							aria-haspopup="menu"
							aria-expanded={addOpen}
							aria-label="Add filter"
							title="Add filter"
						>
							<Icon icon="ri:filter-3-line" width="16" />
							{#if activeFilterCount > 0}
								<span class="filter-badge" aria-hidden="true">{activeFilterCount}</span>
							{/if}
						</button>
						{#if addOpen}
							<div class="add-popover" role="menu" bind:this={addPopoverEl}>
								{#each availableFilters as def (def.id)}
									<button type="button" class="add-row" onclick={() => pickAddFilter(def)}>
										{def.label}
									</button>
								{/each}
							</div>
						{/if}
					</div>
				{/if}
				<button
					class="ctrl-btn"
					onclick={toggleViewMode}
					aria-label={isTable ? 'Switch to card view' : 'Switch to table view'}
					title={isTable ? 'Table' : 'Cards'}
				>
					<Icon icon={isTable ? 'ri:list-check-2' : 'ri:layout-grid-line'} width="16" />
				</button>
			</div>
		</div>

		{#if filters && filters.length > 0}
			<DataGridFilterRail
				{filters}
				{filterValues}
				{asyncOptionsCache}
				{justAddedId}
				onChange={changeFilter}
				onClear={clearFilter}
				onLoadAsync={loadAsyncOptions}
			/>
		{/if}
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
			<button class="clear-search-btn" onclick={() => (searchQuery = '')}>Clear search</button>
		</div>
	{:else if isTable}
		{@const total = displayedItems.length}
		<div class="table-view">
			<table class="data-table">
				<thead>
					<tr>
						{#each columns as col}
							{@const isColSortable = sortable && col.sortable !== false}
							{@const isActive = sortKey === col.key}
							<th
								class:hide-mobile={col.hideOnMobile}
								class:sortable={isColSortable}
								class:sorted={isActive}
								style:width={col.width}
								style:min-width={col.minWidth}
								aria-sort={isActive
									? sortDir === 'asc'
										? 'ascending'
										: 'descending'
									: 'none'}
							>
								{#if isColSortable}
									<button type="button" class="th-sort" onclick={() => onHeaderSort(col)}>
										{#if col.icon}
											<Icon icon={col.icon} width="14" />
										{/if}
										<span>{col.label}</span>
										{#if isActive}
											<Icon
												icon={sortDir === 'asc'
													? 'ri:arrow-up-s-line'
													: 'ri:arrow-down-s-line'}
												width="12"
											/>
										{/if}
									</button>
								{:else}
									<span class="th-content">
										{#if col.icon}
											<Icon icon={col.icon} width="14" />
										{/if}
										<span>{col.label}</span>
									</span>
								{/if}
							</th>
						{/each}
					</tr>
				</thead>
				{#key mountToken}
					<tbody>
						{#each displayedItems as item, i (item.id)}
							{@const meta = { rowIndex: i, colIndex: 0, total } as RowMeta}
							<tr
								class="data-row"
								class:expanded={expandedId === item.id}
								class:animate-in={animateMount && !!tableRow}
								style:--stagger="{staggerMs(i, 0)}ms"
								onclick={() => handleRowClick(item)}
								onkeydown={(e) => handleKeyDown(e, item)}
								tabindex="0"
								role="button"
								aria-label={`Open ${getItemLabel(item)}`}
								aria-expanded={expandDetail ? expandedId === item.id : undefined}
							>
								{#if tableRow}
									{@render tableRow(item, meta)}
								{:else}
									{#each columns as col, ci}
										<td
											class:hide-mobile={col.hideOnMobile}
											class:animate-in={animateMount}
											style:--stagger="{staggerMs(i, ci)}ms"
										>
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
							{#if expandDetail && expandedId === item.id}
								<tr class="expand-row">
									<td colspan={columns.length}>
										{@render expandDetail(item, meta)}
									</td>
								</tr>
							{/if}
						{/each}
					</tbody>
				{/key}
			</table>
		</div>
	{:else}
		{@const total = displayedItems.length}
		{@const cardCols = 4}
		{#key mountToken}
			<div class="card-grid" style:--grid-min={gridMinWidth}>
				{#each displayedItems as item, i (item.id)}
					{@const meta = {
						rowIndex: Math.floor(i / cardCols),
						colIndex: i % cardCols,
						total
					} as RowMeta}
					<button
						class="card"
						class:animate-in={animateMount}
						style:--stagger="{staggerMs(meta.rowIndex, meta.colIndex)}ms"
						onclick={() => handleRowClick(item)}
						onkeydown={(e) => handleKeyDown(e, item)}
						aria-label={`Open ${getItemLabel(item)}`}
					>
						{#if card}
							{@render card(item, meta)}
						{:else}
							<div class="card-content">
								{#each columns.slice(0, 2) as col, ci}
									{@const value = getValue(item, col)}
									{#if ci === 0}
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
		{/key}
	{/if}

	{#if totalPages > 1 && !loading && !error && displayedItems.length > 0}
		<div class="pagination">
			<button
				class="page-btn"
				disabled={currentPage <= 1}
				onclick={() => currentPage--}
				type="button"
			>
				Previous
			</button>
			<span class="page-info">{currentPage} / {totalPages}</span>
			<button
				class="page-btn"
				disabled={currentPage >= totalPages}
				onclick={() => currentPage++}
				type="button"
			>
				Next
			</button>
		</div>
	{/if}
</div>

<style>
	.datagrid-wrapper {
		width: 100%;
	}

	/* Header: toolbar (rect family) on top, optional filter chips (pill family) below.
	   The chip rail only renders when ≥1 filter is active. */
	.datagrid-header {
		position: relative;
	}

	.datagrid-toolbar {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 1rem;
		padding: 0.5rem 0;
	}

	.toolbar-meta {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	/* Search — matches the project's standard Input.svelte styling */
	.search-container {
		position: relative;
		display: flex;
		align-items: center;
		flex: 1;
		max-width: 360px;
	}

	.search-container > :global(svg:first-child) {
		position: absolute;
		left: 10px;
		color: var(--color-foreground-subtle);
		pointer-events: none;
		z-index: 2;
	}

	.search-input {
		width: 100%;
		padding: 7px 30px 7px 30px;
		font-family: var(--font-sans);
		font-size: 0.875rem;
		color: var(--color-foreground);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		outline: none;
		transition: border-color 0.25s ease, box-shadow 0.25s ease;
	}

	.search-input::placeholder {
		color: var(--color-foreground-subtle);
	}

	.search-input:hover:not(:focus) {
		border-color: var(--color-border-strong);
	}

	.search-input:focus {
		border-color: var(--color-primary);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-primary) 25%, transparent);
	}

	.search-clear {
		position: absolute;
		right: 8px;
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
		z-index: 2;
	}

	.search-clear:hover {
		color: var(--color-foreground);
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

	/* Toolbar control buttons (density, view, filter) — share visual weight */
	.ctrl-btn {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		background: var(--color-background-secondary);
		border: 1px solid var(--color-border);
		border-radius: 6px;
		color: var(--color-foreground-muted);
		cursor: pointer;
		transition: all 0.15s ease;
	}

	.ctrl-btn:hover {
		color: var(--color-foreground);
		background: var(--color-background-hover);
	}

	.ctrl-btn:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	.ctrl-btn.has-active {
		color: var(--color-primary);
	}

	/* Filter add: button + popover wrapper */
	.filter-add {
		position: relative;
	}

	.filter-badge {
		position: absolute;
		top: -6px;
		right: -6px;
		min-width: 16px;
		height: 16px;
		padding: 0 4px;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 10px;
		font-weight: 600;
		line-height: 1;
		color: white;
		background: var(--color-primary);
		border-radius: 999px;
		box-sizing: border-box;
		aspect-ratio: 1;
		box-shadow: 0 0 0 1.5px var(--color-background, #fff);
		pointer-events: none;
	}

	.add-popover {
		position: absolute;
		top: calc(100% + 4px);
		right: 0;
		z-index: 50;
		min-width: 180px;
		display: flex;
		flex-direction: column;
		padding: 0.25rem;
		background: var(--color-background, #fff);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.08);
	}

	.add-row {
		padding: 0.375rem 0.5rem;
		font: inherit;
		font-size: 0.8125rem;
		color: var(--color-foreground);
		text-align: left;
		background: transparent;
		border: none;
		border-radius: 4px;
		cursor: pointer;
	}

	.add-row:hover {
		background: var(--color-background-hover);
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

	/* Diagonal stagger fade */
	@keyframes diag-fade {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	.data-row.animate-in,
	td.animate-in,
	.card.animate-in {
		animation: diag-fade 400ms ease-out both;
		animation-delay: var(--stagger, 0ms);
	}

	@media (prefers-reduced-motion: reduce) {
		.data-row.animate-in,
		td.animate-in,
		.card.animate-in {
			animation: none;
		}
	}

	/* ============================================
	   TABLE VIEW
	   ============================================ */
	.table-view {
		padding-top: 0.625rem;
	}

	.data-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.75rem;
	}

	thead tr {
		position: relative;
	}

	thead tr::after {
		content: '';
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		height: 1px;
		background: var(--color-border);
	}

	th {
		text-align: left;
		font-weight: 500;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		padding: 0.625rem 0.75rem;
		white-space: nowrap;
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
	}

	/* First/last cells keep their natural cell padding so table content
	   has a small horizontal inset from the wrapper edges, while the
	   toolbar above stays flush. */

	.th-content,
	.th-sort {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
	}

	.th-sort {
		font: inherit;
		color: inherit;
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
		border-radius: 4px;
	}

	th.sortable:hover .th-sort,
	th.sorted .th-sort {
		color: var(--color-foreground);
	}

	.th-sort:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}

	.th-content :global(svg),
	.th-sort :global(svg) {
		opacity: 0.7;
		flex-shrink: 0;
	}

	td {
		padding: 0.625rem 0.75rem;
		color: var(--color-foreground);
		vertical-align: middle;
	}


	.data-row {
		cursor: pointer;
		position: relative;
		transition: background-color 0.1s ease;
	}

	.data-row::after {
		content: '';
		position: absolute;
		left: 0;
		right: 0;
		bottom: 0;
		height: 1px;
		background: var(--color-border);
	}

	.data-row:hover {
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}

	.data-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
		background: color-mix(in srgb, var(--color-foreground) 6%, transparent);
	}

	.cell-text {
		color: var(--color-foreground);
	}

	/* Density: compact */
	.datagrid-wrapper[data-density='compact'] th,
	.datagrid-wrapper[data-density='compact'] td {
		padding-top: 0.3125rem;
		padding-bottom: 0.3125rem;
	}

	.datagrid-wrapper[data-density='compact'] .data-table {
		font-size: 0.6875rem;
	}

	.datagrid-wrapper[data-density='compact'] .card-grid {
		gap: 0.5rem;
	}

	/* ============================================
	   CARD GRID VIEW
	   ============================================ */
	.card-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(var(--grid-min, 200px), 1fr));
		gap: 0.75rem;
		padding-top: 1rem;
	}

	.card {
		display: flex;
		flex-direction: column;
		align-items: stretch;
		padding: 0;
		background: transparent;
		border: none;
		border-radius: 0;
		cursor: pointer;
		text-align: left;
		width: 100%;
		font: inherit;
		color: inherit;
	}

	.card:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
		border-radius: 8px;
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

	.badge-blue { background: color-mix(in srgb, #3b82f6 15%, transparent); color: #2563eb; }
	.badge-green { background: color-mix(in srgb, #22c55e 15%, transparent); color: #16a34a; }
	.badge-purple { background: color-mix(in srgb, #8b5cf6 15%, transparent); color: #7c3aed; }
	.badge-orange { background: color-mix(in srgb, #f97316 15%, transparent); color: #ea580c; }
	.badge-pink { background: color-mix(in srgb, #ec4899 15%, transparent); color: #db2777; }
	.badge-red { background: color-mix(in srgb, #ef4444 15%, transparent); color: #dc2626; }
	.badge-yellow { background: color-mix(in srgb, #f59e0b 15%, transparent); color: #d97706; }

	.empty-cell {
		color: var(--color-foreground-subtle);
	}

	/* Expand detail row */
	.data-row.expanded td {
		background: var(--color-background-hover, #f9fafb);
	}
	.expand-row td {
		padding: 0;
		border-bottom: 1px solid var(--color-border, #e5e7eb);
	}

	/* Auto-refresh toggle */
	.refresh-toggle {
		display: inline-flex;
		align-items: center;
		gap: 0.375rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted, #6b7280);
		cursor: pointer;
		user-select: none;
	}
	.refresh-toggle input {
		margin: 0;
		cursor: pointer;
	}

	/* Pagination */
	.pagination {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 1rem;
		padding: 0.75rem 0;
	}

	.page-btn {
		padding: 0.25rem 0.625rem;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		background: none;
		border: 1px solid var(--color-border);
		border-radius: 6px;
		cursor: pointer;
		transition: all 0.1s ease;
	}

	.page-btn:hover:not(:disabled) {
		color: var(--color-foreground);
		border-color: var(--color-border-strong);
	}

	.page-btn:disabled {
		opacity: 0.35;
		cursor: default;
	}

	.page-info {
		font-size: 0.75rem;
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
