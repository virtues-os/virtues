<!--
	OntologyDataTable.svelte

	Unified data table for any ontology. Dynamically generates columns
	from schema metadata and renders via UniversalDataGrid.

	Props:
	  - ontologyName: ontology table name (e.g. "data_calendar_event")
	  - date: optional YYYY-MM-DD filter (day page mode)
	  - compact: smaller page size, table-only for embedded use
-->

<script lang="ts">
	import { browser } from '$app/environment';
	import { queryOntologyData, type OntologyDataResponse, type OntologyColumnInfo } from '$lib/api/client';
	import UniversalDataGrid, { type Column } from '$lib/components/datagrid/UniversalDataGrid.svelte';
	import { windowShellStore } from '$lib/stores/window-shell.svelte';

	interface Props {
		ontologyName: string;
		date?: string;
		compact?: boolean;
	}

	let { ontologyName, date, compact = false }: Props = $props();

	type OntologyRow = Record<string, unknown> & { id: string };

	// State
	let data = $state<OntologyDataResponse | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Pagination
	let currentOffset = $state(0);
	const pageSize = $derived(compact ? 25 : 50);

	// Sort
	let sortColumn = $state<string | null>(null);
	let sortDir = $state<'asc' | 'desc'>('desc');

	// Load version to prevent stale responses
	let loadVersion = 0;

	async function loadData() {
		if (!browser) return;
		const version = ++loadVersion;
		loading = true;
		error = null;

		try {
			const result = await queryOntologyData(ontologyName, {
				limit: pageSize,
				offset: currentOffset,
				sort: sortColumn ?? undefined,
				dir: sortDir,
				date,
			});
			if (version !== loadVersion) return;
			data = result;
		} catch (e) {
			if (version !== loadVersion) return;
			error = e instanceof Error ? e.message : 'Failed to load data';
			data = null;
		} finally {
			if (version === loadVersion) loading = false;
		}
	}

	// Reload when inputs change
	$effect(() => {
		// Touch reactive deps
		void ontologyName;
		void date;
		void currentOffset;
		void sortColumn;
		void sortDir;
		void pageSize;
		loadData();
	});

	// Reset pagination when ontology or date changes
	$effect(() => {
		void ontologyName;
		void date;
		currentOffset = 0;
		sortColumn = null;
		sortDir = 'desc';
	});

	// ─────────────────────────────────────────────────────────────────────────
	// Column generation
	// ─────────────────────────────────────────────────────────────────────────

	/** Columns that should not be displayed (but id stays on the data object) */
	const DISPLAY_HIDDEN = new Set(['id', 'embedding']);

	function detectFormat(col: OntologyColumnInfo, timestampColumn: string): Column<OntologyRow>['format'] {
		const name = col.name.toLowerCase();
		const type = col.data_type.toUpperCase();

		// Timestamp columns
		if (name === timestampColumn || name.endsWith('_time') || name.endsWith('_at') || name === 'timestamp' || name === 'played_at') {
			return 'relative-date';
		}

		// Numeric
		if (type === 'INTEGER' || type === 'REAL' || type === 'NUMERIC') {
			// Boolean-like integers
			if (name.startsWith('is_') || name.startsWith('has_')) return 'badge';
			return 'number';
		}

		// Badge candidates
		if (name.endsWith('_type') || name === 'status' || name === 'category' || name === 'direction'
			|| name === 'response_status' || name === 'block_type' || name === 'payment_channel'
			|| name === 'workout_type' || name === 'sleep_quality_score') {
			return 'badge';
		}

		return 'text';
	}

	/** Truncate long text values */
	function makeTruncatedGetter(key: string, maxLen: number): (item: OntologyRow) => string {
		return (item: OntologyRow) => {
			const val = item[key];
			if (val == null) return '';
			const str = String(val);
			return str.length > maxLen ? str.slice(0, maxLen) + '...' : str;
		};
	}

	/** Format boolean-like values */
	function makeBooleanGetter(key: string): (item: OntologyRow) => string {
		return (item: OntologyRow) => {
			const val = item[key];
			if (val === 1 || val === true) return 'Yes';
			if (val === 0 || val === false) return 'No';
			return String(val ?? '');
		};
	}

	const LONG_TEXT_COLUMNS = new Set(['body', 'body_preview', 'description', 'content', 'content_summary', 'text', 'summary', 'event_summary']);

	const generatedColumns = $derived.by((): Column<OntologyRow>[] => {
		if (!data) return [];

		const { columns: schemaCols, key_columns, timestamp_column } = data;

		// Filter out hidden columns
		const visibleCols = schemaCols.filter(c => !DISPLAY_HIDDEN.has(c.name));

		// Sort: key_columns first (in order), then remaining alphabetically
		const keySet = new Set(key_columns);
		const keyCols = key_columns
			.map(k => visibleCols.find(c => c.name === k))
			.filter((c): c is OntologyColumnInfo => c != null);
		const restCols = visibleCols
			.filter(c => !keySet.has(c.name))
			.sort((a, b) => a.name.localeCompare(b.name));

		const orderedCols = [...keyCols, ...restCols];

		return orderedCols.map((col): Column<OntologyRow> => {
			const format = detectFormat(col, timestamp_column);
			const name = col.name.toLowerCase();
			const isLongText = LONG_TEXT_COLUMNS.has(name);
			const isBool = name.startsWith('is_') || name.startsWith('has_');

			const column: Column<OntologyRow> = {
				key: col.name as keyof OntologyRow,
				label: col.name
					.replace(/_/g, ' ')
					.replace(/\b\w/g, c => c.toUpperCase()),
				format,
				hideOnMobile: orderedCols.indexOf(col) > 2,
			};

			if (isLongText) {
				column.getValue = makeTruncatedGetter(col.name, 80);
			} else if (isBool && format === 'badge') {
				column.getValue = makeBooleanGetter(col.name);
				column.badgeColors = { 'yes': 'badge-success', 'no': 'badge-muted' };
			}

			return column;
		});
	});

	// Ensure rows have string id for UniversalDataGrid
	const rows = $derived.by((): OntologyRow[] => {
		if (!data?.rows) return [];
		return data.rows.map(row => ({
			...row,
			id: String(row.id ?? ''),
		})) as OntologyRow[];
	});

	const totalCount = $derived(data?.total_count ?? 0);
	const totalPages = $derived(Math.ceil(totalCount / pageSize));
	const currentPage = $derived(Math.floor(currentOffset / pageSize) + 1);

	function goToPage(page: number) {
		currentOffset = (page - 1) * pageSize;
	}

	function handleSort(colName: string) {
		if (sortColumn === colName) {
			sortDir = sortDir === 'asc' ? 'desc' : 'asc';
		} else {
			sortColumn = colName;
			sortDir = 'desc';
		}
		currentOffset = 0;
	}

	// Open the raw record beside the current view. `ontologyName` is a table name
	// here (e.g. data_calendar_event); the record endpoint accepts that as well as
	// the ontology name, so the link works either way.
	function openRecord(item: OntologyRow) {
		if (!item.id) return;
		windowShellStore.openRouteBeside(`/record/${ontologyName}/${item.id}`);
	}
</script>

<div class="ontology-table-wrapper">
	{#if !compact && data}
		<div class="ontology-header">
			<span class="ontology-domain-badge">{data.domain}</span>
			<span class="ontology-record-count">{totalCount.toLocaleString()} records</span>
		</div>
	{/if}

	<!-- Sort controls (clickable column headers above the grid) -->
	{#if data && generatedColumns.length > 0 && !loading}
		<div class="sort-bar">
			{#each generatedColumns.slice(0, compact ? 4 : 8) as col}
				{@const colName = String(col.key)}
				{@const isActive = sortColumn === colName}
				<button
					class="sort-chip"
					class:active={isActive}
					onclick={() => handleSort(colName)}
					type="button"
				>
					{col.label}
					{#if isActive}
						<span class="sort-arrow">{sortDir === 'asc' ? '\u2191' : '\u2193'}</span>
					{/if}
				</button>
			{/each}
		</div>
	{/if}

	<UniversalDataGrid
		items={rows}
		columns={generatedColumns}
		entityType={ontologyName}
		{loading}
		{error}
		emptyIcon="ri:database-2-line"
		emptyMessage="No data"
		loadingMessage="Loading data..."
		searchPlaceholder="Search..."
		pageSize={pageSize}
		onItemClick={openRecord}
		onRetry={loadData}
	/>

	<!-- Server-side pagination -->
	{#if totalPages > 1 && !loading}
		<div class="pagination">
			<button
				class="page-btn"
				disabled={currentPage <= 1}
				onclick={() => goToPage(currentPage - 1)}
				type="button"
			>
				Previous
			</button>
			<span class="page-info">
				Page {currentPage} of {totalPages}
			</span>
			<button
				class="page-btn"
				disabled={currentPage >= totalPages}
				onclick={() => goToPage(currentPage + 1)}
				type="button"
			>
				Next
			</button>
		</div>
	{/if}
</div>

<style>
	.ontology-table-wrapper {
		width: 100%;
	}

	.ontology-header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		margin-bottom: 0.5rem;
	}

	.ontology-domain-badge {
		font-size: 0.6875rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 8%, transparent);
		padding: 2px 8px;
		border-radius: 9999px;
		text-transform: capitalize;
	}

	.ontology-record-count {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
	}

	/* Sort bar */
	.sort-bar {
		display: flex;
		flex-wrap: wrap;
		gap: 0.375rem;
		padding: 0.5rem 0;
	}

	.sort-chip {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.25rem 0.625rem;
		font-size: 0.6875rem;
		font-weight: 500;
		color: var(--color-foreground-muted);
		background: color-mix(in srgb, var(--color-foreground) 4%, transparent);
		border: 1px solid var(--color-border);
		border-radius: 9999px;
		cursor: pointer;
		transition: all 0.1s ease;
	}

	.sort-chip:hover {
		color: var(--color-foreground);
		border-color: var(--color-border-strong);
	}

	.sort-chip.active {
		color: var(--color-primary);
		border-color: var(--color-primary);
		background: color-mix(in srgb, var(--color-primary) 8%, transparent);
	}

	.sort-arrow {
		font-size: 0.75rem;
	}

	/* Pagination */
	.pagination {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 1rem;
		padding: 1rem 0;
	}

	.page-btn {
		padding: 0.375rem 0.75rem;
		font-size: 0.8125rem;
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
		opacity: 0.4;
		cursor: default;
	}

	.page-info {
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
	}
</style>
