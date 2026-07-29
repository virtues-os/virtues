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
		/** Offer this column in the Group control. Its rendered value is the group key. */
		groupable?: boolean;
		/** Keep the field available for grouping, sorting and search, but don't
		 *  render a column for it — e.g. Kind, which the row icon already says. */
		hidden?: boolean;
		/** Order group keys explicitly; anything unlisted follows, alphabetically. */
		groupOrder?: string[];
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
	import { fly } from 'svelte/transition';
	import { flip } from 'svelte/animate';
	import { cubicOut } from 'svelte/easing';
	import Icon from '$lib/components/Icon.svelte';
	import { dataGridPrefs, type ViewMode, type Density } from '$lib/stores/dataGridPrefs.svelte';
	import { mobileLayout } from '$lib/stores/mobileLayout.svelte';
	import DataGridFilterRail from './DataGridFilterRail.svelte';
	import Popover from '$lib/floating/primitives/Popover.svelte';
	import type {
		FilterDef,
		FilterOption,
		FilterValue,
		GridPage,
		GridQuery,
		GridServerSource,
	} from './types';
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
		/** Column key to group by when no stored preference exists. '' = ungrouped. */
		defaultGroupBy?: string;
		/** Minimum card width in grid mode (CSS value). Default: '200px'. */
		gridMinWidth?: string;
		/** If provided, row click toggles an inline detail row instead of firing onItemClick. */
		expandDetail?: Snippet<[T, RowMeta]>;
		/** Auto-refresh interval in ms. If set, shows a toggle in the toolbar. */
		refreshInterval?: number;
		/** Diagonal fade-in stagger on first paint and on dataset identity change.
		 *  Default false: a per-cell staggered animation makes every list feel slow
		 *  and replays on each sort/filter change. Opt in per grid. */
		animateMount?: boolean;
		/** Enable built-in column sort. Default true. Set false to opt out per-grid. */
		sortable?: boolean;
		/** Declarative filters. Renders a chip rail in the toolbar; results are
		 *  filtered client-side via def.predicate (or equality on def.field). */
		filters?: FilterDef<T>[];
		/** Server-side pagination. When set, `items` is ignored and search/
		 *  sort/filter/page are forwarded to this source as a GridQuery — the
		 *  grid never holds more than one page. Grouping is unavailable (it
		 *  needs the whole set), selection is page-scoped, and only columns
		 *  the server can ORDER BY should be marked `sortable`. */
		server?: GridServerSource<T>;
		/** Consumer-owned query context passed through to GridQuery.extra
		 *  (e.g. an external chip row). Changing it refetches from page 1. */
		serverExtra?: Record<string, unknown>;
		onItemClick?: (item: T) => void;
		onItemContextMenu?: (item: T, e: MouseEvent) => void;
		onRefresh?: () => void;
		onRetry?: () => void;
		// Custom renderers — receive RowMeta for stagger / index-aware rendering.
		tableRow?: Snippet<[T, RowMeta]>;
		card?: Snippet<[T, RowMeta]>;
		/** Grid-level actions (add, import…) rendered beside the view controls,
		 *  so a consumer doesn't need its own header row above the toolbar. */
		toolbarActions?: Snippet;
		/** Multi-select with checkboxes, shift-range, ⌘A and a bulk action bar. */
		selectable?: boolean;
		/** Rendered in the bulk bar while rows are selected. */
		bulkActions?: Snippet<[T[], () => void]>;
		onSelectionChange?: (items: T[]) => void;
		/** Trailing per-row controls, revealed on hover/focus. Discoverable in a
		 *  way a right-click-only menu never is. */
		rowActions?: Snippet<[T]>;
		/** Leading glyph for a row. It shares one column with the select box:
		 *  the icon is what you see at rest, the checkbox is what you see on
		 *  hover — so selection costs no width and the icon isn't decoration. */
		rowIcon?: (item: T) => string | null | undefined;
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
		defaultGroupBy,
		gridMinWidth = '200px',
		expandDetail,
		refreshInterval,
		animateMount = false,
		sortable = true,
		filters,
		server,
		serverExtra,
		onItemClick,
		onItemContextMenu,
		onRefresh,
		onRetry,
		tableRow,
		card,
		toolbarActions,
		selectable = false,
		bulkActions,
		onSelectionChange,
		rowActions,
		rowIcon
	}: Props = $props();

	/** The leading column exists if either thing needs it; they share it. */
	const hasLeadCol = $derived(selectable || !!rowIcon);

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

	// Add-filter popover state (lives in toolbar, not in the chip rail).
	// Positioning + click-outside handled by the Popover primitive.
	let addOpen = $state(false);
	let justAddedId = $state<string | null>(null);

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
	// Server mode. With a `server` source the client pipeline above becomes
	// inert and search/sort/filter/page are forwarded as a GridQuery. The grid
	// keeps the previous page's rows on screen while the next one loads
	// (stale-while-loading); the skeleton only shows before the first page.
	// ────────────────────────────────────────────────────────────────────────
	const serverMode = $derived(!!server);

	let serverItems = $state<T[]>([]);
	let serverTotal = $state(0);
	let serverLoading = $state(false);
	let serverLoaded = $state(false);
	let serverError = $state<string | null>(null);
	let serverSeq = 0;
	let refreshTick = $state(0);

	// Debounce only the query the server sees — the input stays live.
	let debouncedSearch = $state('');
	$effect(() => {
		const q = searchQuery;
		if (!serverMode) return;
		const t = setTimeout(() => (debouncedSearch = q), 250);
		return () => clearTimeout(t);
	});

	// A handful of recent pages, keyed on the full query, so Previous is
	// instant. Cleared by auto-refresh and manual retry.
	const pageCache = new Map<string, GridPage<T>>();
	const PAGE_CACHE_MAX = 8;

	// ────────────────────────────────────────────────────────────────────────
	// Pagination
	// ────────────────────────────────────────────────────────────────────────
	let currentPage = $state(1);
	const totalCount = $derived(serverMode ? serverTotal : items.length);
	const filteredCount = $derived(serverMode ? serverTotal : sortedItems.length);
	const totalPages = $derived(Math.max(1, Math.ceil(filteredCount / pageSize)));
	const displayedItems = $derived(
		serverMode
			? serverItems
			: sortedItems.slice((currentPage - 1) * pageSize, currentPage * pageSize)
	);

	$effect(() => {
		void searchQuery;
		void items;
		void sortKey;
		void sortDir;
		void filterValues;
		void serverExtra;
		currentPage = 1;
	});

	$effect(() => {
		if (!server) return;
		void refreshTick;
		const query: GridQuery = {
			offset: (currentPage - 1) * pageSize,
			limit: pageSize,
			search: debouncedSearch.trim(),
			sort: sortKey && sortDir ? { key: String(sortKey), dir: sortDir } : null,
			filters: { ...filterValues },
			extra: serverExtra,
		};
		const key = JSON.stringify(query);
		const cached = pageCache.get(key);
		if (cached) {
			serverItems = cached.items;
			serverTotal = cached.total;
			serverLoaded = true;
			return;
		}
		const seq = ++serverSeq;
		serverLoading = true;
		serverError = null;
		server(query)
			.then((pg) => {
				if (seq !== serverSeq) return;
				pageCache.set(key, pg);
				if (pageCache.size > PAGE_CACHE_MAX) {
					const oldest = pageCache.keys().next().value;
					if (oldest !== undefined) pageCache.delete(oldest);
				}
				serverItems = pg.items;
				serverTotal = pg.total;
			})
			.catch((e) => {
				if (seq !== serverSeq) return;
				serverError = e instanceof Error ? e.message : 'Failed to load';
			})
			.finally(() => {
				if (seq !== serverSeq) return;
				serverLoading = false;
				serverLoaded = true;
			});
	});

	function retryServer() {
		pageCache.clear();
		serverLoaded = false;
		refreshTick++;
	}

	/** Search or filters currently narrowing the result set. */
	const isNarrowed = $derived(
		!!searchQuery.trim() ||
			(filters ?? []).some((f) => f.id in filterValues && isFilterActive(filterValues[f.id]))
	);

	const effectiveLoading = $derived(loading || (serverMode && serverLoading && !serverLoaded));
	const effectiveError = $derived(error ?? (serverMode ? serverError : null));

	// ────────────────────────────────────────────────────────────────────────
	// View mode + density (persisted per entityType)
	//
	// Initialize directly from prefs / defaultViewMode so there's no first-paint
	// flash from 'table' to the actual mode. Reading reactive props at $state
	// init time captures the initial value, which is fine — the $effect below
	// re-syncs on subsequent prop changes.
	// ────────────────────────────────────────────────────────────────────────
	// On the phone, cards are the default (tables need width); an explicit
	// user preference still wins.
	const fallbackViewMode = $derived<ViewMode>(
		mobileLayout.isMobile ? 'grid' : defaultViewMode
	);

	// svelte-ignore state_referenced_locally
	let viewMode = $state<ViewMode>(
		dataGridPrefs.hasViewMode(entityType)
			? dataGridPrefs.getViewMode(entityType)
			: mobileLayout.isMobile
				? 'grid'
				: defaultViewMode
	);
	// svelte-ignore state_referenced_locally
	let density = $state<Density>(
		dataGridPrefs.hasDensity(entityType) ? dataGridPrefs.getDensity(entityType) : 'comfortable'
	);

	$effect(() => {
		viewMode = dataGridPrefs.hasViewMode(entityType)
			? dataGridPrefs.getViewMode(entityType)
			: fallbackViewMode;
		density = dataGridPrefs.hasDensity(entityType)
			? dataGridPrefs.getDensity(entityType)
			: 'comfortable';
	});

	// Grouping needs the whole set; a server page can't honestly group.
	const groupableCols = $derived(serverMode ? [] : columns.filter((c) => c.groupable));
	/** Columns that actually get a header and a cell. */
	const visibleColumns = $derived(columns.filter((c) => !c.hidden));

	/**
	 * Two modes. There used to be a third — Board — but a board is precisely
	 * the card view with a grouping applied, so it duplicated a state the Group
	 * control could already express, and choosing it *without* a group left you
	 * looking at cards in a single nameless column. Group is an orthogonal axis
	 * now: set one in card view and the groups become the columns.
	 */
	const VIEW_META: Record<ViewMode, { icon: string; label: string }> = {
		table: { icon: 'ri:list-check-2', label: 'Table' },
		grid: { icon: 'ri:layout-grid-line', label: 'Cards' },
	};

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

	// ────────────────────────────────────────────────────────────────────────
	// Selection. A table that can't operate on a set is a viewer, not a tool:
	// click to toggle, shift-click for a range, header box for all-visible.
	// ────────────────────────────────────────────────────────────────────────
	let selectedIds = $state<Set<string>>(new Set());
	/** Anchor for shift-range, as an index into the flat visible order. */
	let selectionAnchor = $state<number | null>(null);

	const selectedItems = $derived(displayedItems.filter((i) => selectedIds.has(i.id)));
	const allVisibleSelected = $derived(
		displayedItems.length > 0 && displayedItems.every((i) => selectedIds.has(i.id)),
	);
	const someVisibleSelected = $derived(
		!allVisibleSelected && displayedItems.some((i) => selectedIds.has(i.id)),
	);

	function setSelection(next: Set<string>) {
		selectedIds = next;
		onSelectionChange?.(displayedItems.filter((i) => next.has(i.id)));
	}

	function toggleSelected(item: T, index: number, extend = false) {
		const next = new Set(selectedIds);
		if (extend && selectionAnchor !== null) {
			const [from, to] = index < selectionAnchor ? [index, selectionAnchor] : [selectionAnchor, index];
			for (let i = from; i <= to; i++) {
				const it = visualOrder[i];
				if (it) next.add(it.id);
			}
		} else {
			if (next.has(item.id)) next.delete(item.id);
			else next.add(item.id);
			selectionAnchor = index;
		}
		setSelection(next);
	}

	function toggleAllVisible() {
		if (allVisibleSelected) setSelection(new Set());
		else setSelection(new Set(displayedItems.map((i) => i.id)));
		selectionAnchor = null;
	}

	function clearSelection() {
		selectionAnchor = null;
		setSelection(new Set());
	}

	// Selecting is per-result-set: a row you can no longer see must not stay
	// silently selected and get swept up by a bulk action.
	//
	// Keyed on the visible ids, NOT on `items` identity — consumers commonly pass
	// a `$derived` array, which is a fresh object on every render including the
	// render that selecting itself triggers. Watching identity made a selection
	// clear itself the instant it was made.
	let lastVisibleSig = $state('');
	$effect(() => {
		const sig = displayedItems.map((i) => i.id).join(',');
		if (sig === lastVisibleSig) return;
		lastVisibleSig = sig;
		if (selectedIds.size) clearSelection();
	});

	// ────────────────────────────────────────────────────────────────────────
	// Keyboard. Roving focus over the flat visible order, so the grid is
	// operable without reaching for the mouse.
	// ────────────────────────────────────────────────────────────────────────
	let focusedIndex = $state(-1);

	function focusRow(index: number) {
		const clamped = Math.max(0, Math.min(index, visualOrder.length - 1));
		focusedIndex = clamped;
		const item = visualOrder[clamped];
		if (!item) return;
		// The DOM order matches the flat order within each group, so target by id.
		gridEl
			?.querySelector<HTMLElement>(`[data-row-id="${CSS.escape(item.id)}"]`)
			?.focus({ preventScroll: false });
	}

	let gridEl = $state<HTMLElement | null>(null);

	function handleKeyDown(e: KeyboardEvent, item: T) {
		const index = visualOrder.findIndex((i) => i.id === item.id);
		switch (e.key) {
			case 'Enter':
				e.preventDefault();
				handleRowClick(item);
				break;
			case ' ':
				// Space selects; Enter opens. Conflating them is why so many
				// tables can't be driven from the keyboard at all.
				if (selectable) {
					e.preventDefault();
					toggleSelected(item, index, e.shiftKey);
				} else {
					e.preventDefault();
					handleRowClick(item);
				}
				break;
			case 'ArrowDown':
				e.preventDefault();
				focusRow(index + 1);
				if (e.shiftKey && selectable) toggleSelected(visualOrder[index + 1] ?? item, index + 1, true);
				break;
			case 'ArrowUp':
				e.preventDefault();
				focusRow(index - 1);
				if (e.shiftKey && selectable) toggleSelected(visualOrder[index - 1] ?? item, index - 1, true);
				break;
			case 'Home':
				e.preventDefault();
				focusRow(0);
				break;
			case 'End':
				e.preventDefault();
				focusRow(visualOrder.length - 1);
				break;
			case 'a':
				if (selectable && (e.metaKey || e.ctrlKey)) {
					e.preventDefault();
					setSelection(new Set(displayedItems.map((i) => i.id)));
				}
				break;
			case 'Escape':
				if (selectedIds.size) {
					e.preventDefault();
					clearSelection();
				}
				break;
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
		if (!autoRefresh || !refreshInterval) return;
		// Server mode refreshes itself: drop the page cache and refetch the
		// current query. Client mode delegates to the consumer's onRefresh.
		if (serverMode) {
			const timer = setInterval(() => {
				pageCache.clear();
				refreshTick++;
			}, refreshInterval);
			return () => clearInterval(timer);
		}
		if (!onRefresh) return;
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
		// Without the animation there's nothing to replay, and bumping the token
		// would tear down and rebuild every row on each sort/filter for nothing.
		if (!animateMount) return;
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
		if (!col.badgeColors) return 'badge-muted';
		return col.badgeColors[value.toLowerCase()] || 'badge-muted';
	}

	function staggerMs(rowIndex: number, colIndex: number): number {
		// Total visible cascade caps at ~1s (cap 600ms stagger + 400ms duration).
		return Math.min((rowIndex * 0.6 + colIndex * 0.4) * 200, 600);
	}

	// ────────────────────────────────────────────────────────────────────────
	// Grouping. `board` is the card renderer with groups as columns, so it
	// forces a grouping on; table and cards render groups as sections.
	// ────────────────────────────────────────────────────────────────────────
	let groupKey = $state('');
	let groupOpen = $state(false);
	let groupInitialized = $state(false);
	$effect(() => {
		if (groupInitialized) return;
		groupInitialized = true;
		const stored = dataGridPrefs.hasGroupBy(entityType)
			? dataGridPrefs.getGroupBy(entityType)
			: (defaultGroupBy ?? '');
		// A stored key whose column has since gone away must not strand the grid
		// in a grouping the user can no longer see or clear.
		groupKey = groupableCols.some((c) => String(c.key) === stored) ? stored : '';
	});

	function setGroupKey(next: string) {
		groupKey = next;
		dataGridPrefs.setGroupBy(entityType, next);
	}

	const activeGroupCol = $derived(groupableCols.find((c) => String(c.key) === groupKey));

	let collapsedGroups = $state<Set<string>>(new Set());
	function toggleGroup(key: string) {
		const next = new Set(collapsedGroups);
		if (next.has(key)) next.delete(key);
		else next.add(key);
		collapsedGroups = next;
	}

	const groupedItems = $derived.by<{ key: string; items: T[] }[]>(() => {
		const col = activeGroupCol;
		if (!col) return [{ key: '', items: displayedItems }];
		const buckets = new Map<string, T[]>();
		for (const item of displayedItems) {
			const key = getValue(item, col) || '—';
			const bucket = buckets.get(key);
			if (bucket) bucket.push(item);
			else buckets.set(key, [item]);
		}
		const order = col.groupOrder ?? [];
		return [...buckets.entries()]
			.map(([key, items]) => ({ key, items }))
			.sort((a, b) => {
				const ai = order.indexOf(a.key);
				const bi = order.indexOf(b.key);
				if (ai !== -1 || bi !== -1) return (ai === -1 ? 999 : ai) - (bi === -1 ? 999 : bi);
				return a.key.localeCompare(b.key);
			});
	});

	const isGrouped = $derived(!!activeGroupCol);

	/**
	 * The rows as they actually appear: group order, collapsed groups omitted.
	 * Keyboard travel and shift-ranges must follow what's on screen — indexing
	 * into the ungrouped data order makes a range from row 1 to row 3 select
	 * whatever happens to sit between them in the underlying array instead.
	 */
	const visualOrder = $derived(
		groupedItems.filter((g) => !collapsedGroups.has(g.key)).flatMap((g) => g.items),
	);
	/** Row index in visual order, for RowMeta, the stagger, and range selection. */
	const rowIndexById = $derived(new Map(visualOrder.map((item, i) => [item.id, i])));

	const isTable = $derived(viewMode === 'table');
	/** Cards + a grouping = a board. That is the whole of what Board ever was. */
	const asBoard = $derived(viewMode === 'grid' && isGrouped);

	// Motion duration, zeroed when the OS asks for reduced motion. Svelte's JS
	// transitions don't honour the media query on their own — the CSS-only
	// `@media (prefers-reduced-motion)` block can't reach them.
	const prefersReducedMotion =
		typeof window !== 'undefined' &&
		window.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
	const motionMs = prefersReducedMotion ? 0 : 180;
</script>

<div class="datagrid-wrapper" data-density={density}>
	<div class="datagrid-header">
		<div class="datagrid-toolbar">
			{#if selectable && selectedItems.length > 0}
				<!-- Selection takes over the toolbar rather than inserting a bar above
				     the table: an extra row would push every result down the moment you
				     tick a box, and back up again when you untick it. -->
				<span class="bulk-count mono" role="status" aria-live="polite">
					{selectedItems.length} selected
				</span>
				<span class="bulk-sp"></span>
				{#if bulkActions}
					{@render bulkActions(selectedItems, clearSelection)}
				{/if}
				<button class="bulk-clear" onclick={clearSelection}>Clear</button>
			{:else}
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
					<!-- Any narrowing counts, not just search. With a filter chip set, the
					     count went on reporting the unfiltered total, so the number
					     contradicted the rows directly beneath it. (In server mode the
					     total IS the narrowed total, so narrowing decides the word.) -->
					{#if filteredCount !== totalCount || (serverMode && isNarrowed)}
						{filteredCount} {filteredCount === 1 ? 'result' : 'results'}
					{:else}
						{totalCount} {totalCount === 1 ? 'item' : 'items'}
					{/if}
				</span>
				{#if refreshInterval && (onRefresh || serverMode)}
					<label class="refresh-toggle">
						<input type="checkbox" bind:checked={autoRefresh} />
						<span>Auto-refresh</span>
					</label>
				{/if}
				{#if filters && filters.length > 0 && availableFilters.length > 0}
					<div class="filter-add">
						<Popover bind:open={addOpen} placement="bottom-end" offset={4}>
							{#snippet trigger({ toggle: triggerToggle })}
								<button
									class="ctrl-btn"
									class:has-active={activeFilterCount > 0}
									onclick={triggerToggle}
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
							{/snippet}
							{#snippet children()}
								<div class="add-popover" role="menu">
									{#each availableFilters as def (def.id)}
										<button type="button" class="add-row" onclick={() => pickAddFilter(def)}>
											{def.label}
										</button>
									{/each}
								</div>
							{/snippet}
						</Popover>
					</div>
				{/if}
				{#if groupableCols.length > 0}
					<!-- A menu, not a <select>. The native control paints OS chrome that
					     ignores the app's type and colour entirely, and it can't show a
					     checkmark against the active choice — so the one place the toolbar
					     stated a *value* was the one place that didn't look like the app. -->
					<Popover bind:open={groupOpen} placement="bottom-end" offset={4}>
						{#snippet trigger({ toggle: triggerToggle })}
							<button
								class="ctrl-btn group-btn"
								class:has-active={!!activeGroupCol}
								onclick={triggerToggle}
								aria-haspopup="menu"
								aria-expanded={groupOpen}
								title="Group by"
							>
								<Icon icon="ri:stack-line" width="16" />
								{#if activeGroupCol}
									<span class="group-btn-value">{activeGroupCol.label}</span>
								{/if}
							</button>
						{/snippet}
						{#snippet children({ close }: { close: () => void })}
							<div class="menu-popover" role="menu">
								<button
									type="button"
									class="menu-opt"
									class:on={!activeGroupCol}
									onclick={() => {
										setGroupKey('');
										close();
									}}
								>
									<Icon icon="ri:check-line" width="14" />
									<span>No grouping</span>
								</button>
								{#each groupableCols as col (String(col.key))}
									<button
										type="button"
										class="menu-opt"
										class:on={groupKey === String(col.key)}
										onclick={() => {
											setGroupKey(String(col.key));
											close();
										}}
									>
										<Icon icon="ri:check-line" width="14" />
										<span>{col.label}</span>
									</button>
								{/each}
							</div>
						{/snippet}
					</Popover>
				{/if}
				<button
					class="ctrl-btn"
					onclick={toggleViewMode}
					aria-label={isTable ? 'Switch to card view' : 'Switch to table view'}
					title={VIEW_META[viewMode].label}
				>
					<Icon icon={VIEW_META[viewMode].icon} width="16" />
				</button>
				{#if toolbarActions}
					{@render toolbarActions()}
				{/if}
			</div>
			{/if}
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

	{#if effectiveLoading}
		<!-- Skeleton rows in the table's own shape: nothing reflows when the data
		     lands, which is the difference between "loading" and "jumping". -->
		<div class="skeleton" role="status" aria-live="polite" aria-label={loadingMessage}>
			{#each Array(6) as _, i}
				<div class="sk-row" style:--sk-delay="{i * 60}ms">
					{#if hasLeadCol}<span class="sk-box"></span>{/if}
					<span class="sk-bar" style:width="{58 - ((i * 7) % 22)}%"></span>
					<span class="sk-bar sk-narrow"></span>
					<span class="sk-bar sk-narrow"></span>
				</div>
			{/each}
		</div>
	{:else if effectiveError}
		<div class="error-state" role="alert">
			<Icon icon="ri:error-warning-line" width="24" />
			<span>{effectiveError}</span>
			{#if serverMode}
				<button class="retry-btn" onclick={retryServer}>Retry</button>
			{:else if onRetry}
				<button class="retry-btn" onclick={onRetry}>Retry</button>
			{/if}
		</div>
	{:else if displayedItems.length === 0 && !isNarrowed}
		<div class="empty-state">
			<Icon icon={emptyIcon} width="32" />
			<p>{emptyMessage}</p>
		</div>
	{:else if displayedItems.length === 0}
		<div class="empty-state">
			<Icon icon="ri:search-line" width="32" />
			{#if searchQuery.trim()}
				<p>No results for "{searchQuery}"</p>
				<button class="clear-search-btn" onclick={() => (searchQuery = '')}>Clear search</button>
			{:else}
				<p>No results match the active filters</p>
			{/if}
		</div>
	{:else if isTable}
		{@const total = displayedItems.length}
		<!-- Once anything is selected the boxes stay put: hunting for a checkbox
		     that only exists under the cursor is fine for starting a selection and
		     miserable for extending one. -->
		<div
			class="table-view"
			class:sel-on={selectedIds.size > 0}
			bind:this={gridEl}
			in:fly={{ y: 6, duration: motionMs, easing: cubicOut }}
		>
			<table class="data-table">
				<thead>
					<tr>
						{#if hasLeadCol}
							<th class="sel-col">
								{#if selectable}
									<input
										type="checkbox"
										class="sel-box sel-head"
										checked={allVisibleSelected}
										indeterminate={someVisibleSelected}
										onchange={toggleAllVisible}
										aria-label={allVisibleSelected ? 'Deselect all' : 'Select all'}
									/>
								{/if}
							</th>
						{/if}
						{#each visibleColumns as col}
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
						{#if rowActions}
							<th class="row-actions-col" aria-label="Actions"></th>
						{/if}
					</tr>
				</thead>
				{#key mountToken}
					<tbody>
						{#each groupedItems as group (group.key)}
							{#if isGrouped}
								<tr class="group-row">
									<td colspan={visibleColumns.length + (hasLeadCol ? 1 : 0) + (rowActions ? 1 : 0)}>
										<button
											class="group-toggle"
											class:closed={collapsedGroups.has(group.key)}
											onclick={() => toggleGroup(group.key)}
											aria-expanded={!collapsedGroups.has(group.key)}
										>
											<Icon icon="ri:arrow-down-s-line" width="14" class="group-chev" />
											<span class="group-name">{group.key}</span>
											<span class="group-count">{group.items.length}</span>
										</button>
									</td>
								</tr>
							{/if}
							{#if !collapsedGroups.has(group.key)}
						{#each group.items as item (item.id)}
							{@const i = rowIndexById.get(item.id) ?? 0}
							{@const meta = { rowIndex: i, colIndex: 0, total } as RowMeta}
							<tr
								class="data-row"
								class:expanded={expandedId === item.id}
								class:selected={selectedIds.has(item.id)}
								class:animate-in={animateMount && !!tableRow}
								style:--stagger="{staggerMs(i, 0)}ms"
								data-row-id={item.id}
								onclick={(e) => {
									// Shift-click extends a selection rather than opening; without
									// this the only way to select a range is one row at a time.
									if (selectable && (e.shiftKey || e.metaKey || e.ctrlKey)) {
										e.preventDefault();
										toggleSelected(item, i, e.shiftKey);
									} else handleRowClick(item);
								}}
								oncontextmenu={onItemContextMenu ? (e) => onItemContextMenu(item, e) : undefined}
								onkeydown={(e) => handleKeyDown(e, item)}
								onfocus={() => (focusedIndex = i)}
								tabindex={focusedIndex === i || (focusedIndex === -1 && i === 0) ? 0 : -1}
								role={selectable ? 'row' : 'button'}
								aria-label={`Open ${getItemLabel(item)}`}
								aria-selected={selectable ? selectedIds.has(item.id) : undefined}
								aria-expanded={expandDetail ? expandedId === item.id : undefined}
							>
								{#if hasLeadCol}
									{@const glyph = rowIcon?.(item)}
									<td class="sel-col">
										<span class="lead" class:has-glyph={!!glyph}>
											{#if glyph}
												<Icon icon={glyph} width="15" class="lead-glyph" />
											{/if}
											{#if selectable}
												<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
												<input
													type="checkbox"
													class="sel-box"
													checked={selectedIds.has(item.id)}
													onclick={(e) => {
														e.stopPropagation();
														toggleSelected(item, i, e.shiftKey);
													}}
													aria-label={`Select ${getItemLabel(item)}`}
												/>
											{/if}
										</span>
									</td>
								{/if}
								{#if tableRow}
									{@render tableRow(item, meta)}
								{:else}
									{#each visibleColumns as col, ci}
										<td
											class:hide-mobile={col.hideOnMobile}
											class:numeric={col.format === 'number'}
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
								{#if rowActions}
									<td class="row-actions-cell">
										<div
											class="row-actions"
											role="presentation"
											onclick={(e) => e.stopPropagation()}
										>
											{@render rowActions(item)}
										</div>
									</td>
								{/if}
							</tr>
							{#if expandDetail && expandedId === item.id}
								<tr class="expand-row">
									<td colspan={visibleColumns.length + (hasLeadCol ? 1 : 0) + (rowActions ? 1 : 0)}>
										{@render expandDetail(item, meta)}
									</td>
								</tr>
							{/if}
						{/each}
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
			<div
				class:card-groups={!asBoard}
				class:board={asBoard}
				in:fly={{ y: 6, duration: motionMs, easing: cubicOut }}
			>
			{#each groupedItems as group (group.key)}
				<div class:card-group={!asBoard} class:board-col={asBoard}>
					{#if isGrouped}
						<button
							class="group-toggle"
							class:closed={collapsedGroups.has(group.key)}
							onclick={() => toggleGroup(group.key)}
							aria-expanded={!collapsedGroups.has(group.key)}
						>
							{#if !asBoard}
								<Icon icon="ri:arrow-down-s-line" width="14" class="group-chev" />
							{/if}
							<span class="group-name">{group.key}</span>
							<span class="group-count">{group.items.length}</span>
						</button>
					{/if}
					{#if !collapsedGroups.has(group.key)}
					<div
						class:card-grid={!asBoard}
						class:board-stack={asBoard}
						style:--grid-min={gridMinWidth}
					>
				{#each group.items as item (item.id)}
					{@const i = rowIndexById.get(item.id) ?? 0}
					{@const meta = {
						rowIndex: Math.floor(i / cardCols),
						colIndex: i % cardCols,
						total
					} as RowMeta}
					<button
						class="card"
						class:animate-in={animateMount}
						animate:flip={{ duration: motionMs, easing: cubicOut }}
						style:--stagger="{staggerMs(meta.rowIndex, meta.colIndex)}ms"
						onclick={() => handleRowClick(item)}
						oncontextmenu={onItemContextMenu ? (e) => onItemContextMenu(item, e) : undefined}
						onkeydown={(e) => handleKeyDown(e, item)}
						aria-label={`Open ${getItemLabel(item)}`}
					>
						{#if card}
							{@render card(item, meta)}
						{:else}
							<div class="card-content">
								{#each visibleColumns.slice(0, 2) as col, ci}
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
					{/if}
				</div>
			{/each}
			</div>
		{/key}
	{/if}

	{#if totalPages > 1 && !effectiveLoading && !effectiveError && displayedItems.length > 0}
		<div class="pagination">
			<button
				class="page-btn"
				disabled={currentPage <= 1 || (serverMode && serverLoading)}
				onclick={() => currentPage--}
				type="button"
			>
				Previous
			</button>
			{#if serverMode}
				<!-- An honest range: the server knows the true total, so say it. -->
				<span class="page-info">
					{(currentPage - 1) * pageSize + 1}–{Math.min(
						currentPage * pageSize,
						serverTotal
					)} of {serverTotal}
				</span>
			{:else}
				<span class="page-info">{currentPage} / {totalPages}</span>
			{/if}
			<button
				class="page-btn"
				disabled={currentPage >= totalPages || (serverMode && serverLoading)}
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
		/* 34px control + 2 x 8px padding. Pinned because the row swaps between
		   search and selection controls, and everything below it would jump. */
		min-height: 50px;
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
		/* Fixed so the toolbar's height is a known quantity: the row swaps between
		   this and the selection controls, and it must not resize when it does. */
		height: 34px;
		padding: 0 30px;
		font-family: var(--font-sans);
		font-size: 0.875rem;
		color: var(--color-foreground);
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
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

	/* Recovery actions, not calls to action — an outline in the primary colour
	   gave "Clear search" the same weight as a submit button. */
	.clear-search-btn {
		margin-top: 0.5rem;
		padding: 0.375rem 0.75rem;
		font-size: 0.8125rem;
		color: var(--color-foreground-muted);
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		cursor: pointer;
	}

	.clear-search-btn:hover {
		background: var(--color-background-hover);
		color: var(--color-foreground);
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
		border-radius: 8px;
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

	/* Group-by: an icon button that grows a label once a grouping is on, so the
	   toolbar stays quiet when it has nothing to say and states the grouping
	   plainly when it does. */
	.group-btn { width: auto; min-width: 32px; gap: 6px; padding: 0 8px; }
	.group-btn-value {
		font-size: 0.75rem;
		color: var(--color-foreground);
		white-space: nowrap;
	}

	/* Menu shared by the group control. Same surface as .add-popover — one
	   popover look for the whole toolbar. */
	.menu-popover {
		min-width: 170px;
		display: flex;
		flex-direction: column;
		padding: 0.25rem;
		background: var(--color-background, #fff);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.08);
	}
	.menu-opt {
		display: flex;
		align-items: center;
		gap: 0.4rem;
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
	.menu-opt:hover { background: var(--color-background-hover); }
	/* The checkmark holds its column whether or not it's shown, so the labels
	   line up instead of shifting by 14px between states. */
	.menu-opt :global(svg) { opacity: 0; flex-shrink: 0; color: var(--color-foreground-muted); }
	.menu-opt.on :global(svg) { opacity: 1; }
	.menu-opt:focus-visible { outline: 2px solid var(--color-primary); outline-offset: -2px; }

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
		border-radius: var(--radius-full);
		box-sizing: border-box;
		aspect-ratio: 1;
		box-shadow: 0 0 0 1.5px var(--color-background, #fff);
		pointer-events: none;
	}

	.add-popover {
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
		color: var(--color-foreground-muted);
		background: transparent;
		border: 1px solid var(--color-border);
		border-radius: 8px;
		cursor: pointer;
	}

	.retry-btn:hover {
		background: var(--color-background-hover);
		color: var(--color-foreground);
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
		/* The shell clips overflow (fixed, overflow:hidden); when the visible
		   columns' min-widths exceed a phone viewport the table must scroll
		   sideways within its own container, not get cut off. */
		overflow-x: auto;
		-webkit-overflow-scrolling: touch;
	}

	.data-table {
		width: 100%;
		border-collapse: collapse;
		/* 13.5px. Was 12px — caption size doing primary-content work, which
		   consumers were already overriding to 13px in their own cells. */
		font-size: 0.84375rem;
		font-variant-numeric: tabular-nums;
	}

	/* Header carries a single hairline, not a fill AND a rule. The label takes
	   the app's mono eyebrow treatment so it reads as a column name rather than
	   as a first row of data. */
	th {
		text-align: left;
		font-family: var(--font-mono);
		font-weight: 400;
		font-size: 0.65625rem;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--color-foreground-subtle);
		padding: 0.5rem 0.75rem;
		white-space: nowrap;
		border-bottom: 1px solid var(--color-border);
		/* Pinned: past a screenful of rows, a header that scrolls away turns
		   every column into a guess. It needs an opaque fill to cover the rows
		   sliding under it — but the fill should be the PAGE's colour, so the
		   header reads as clear and only its rule shows.
		   It was --color-background (#FDFCF9) inside a card painted --surface
		   (#FFFFFF), which drew a cream band across the top of every table for
		   no reason anyone chose. */
		position: sticky;
		top: 0;
		z-index: 2;
		background: var(--color-surface);
	}

	td.numeric { text-align: right; font-variant-numeric: tabular-nums; }

	/* Column icons are sized for the old 12px sans label; bring them down to
	   the eyebrow's optical size. */
	.th-content :global(svg),
	.th-sort :global(svg) {
		width: 12px;
		height: 12px;
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


	/* Separators are real borders now. As ::after overlays they sat inside the
	   row box, so the hover wash painted over them and the grid lines dropped
	   out as the pointer moved down the table.
	   The border must sit on the row, not on td: cells come from consumer
	   `tableRow` snippets and carry the *consumer's* Svelte scope hash, so a
	   `td` rule here would never match them. `border-collapse: collapse` paints
	   the row border full width, and a border draws above its own background,
	   so the hover wash can no longer erase it. */
	.data-row {
		cursor: pointer;
		position: relative;
		border-bottom: 1px solid var(--color-border);
		transition: background-color 0.1s ease;
	}

	.data-row:hover {
		background: color-mix(in srgb, var(--color-foreground) 3.5%, transparent);
	}

	.data-row:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: -2px;
		background: color-mix(in srgb, var(--color-foreground) 3.5%, transparent);
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
		font-size: 0.78125rem;
	}

	.datagrid-wrapper[data-density='compact'] .card-grid {
		gap: 0.5rem;
	}

	/* ============================================
	   SELECTION + ROW ACTIONS
	   ============================================ */
	/* One column, two jobs. The row's type glyph is what's there at rest; the
	   checkbox replaces it under the cursor. A dedicated checkbox column spends
	   permanent width on a control that is empty most of the time, and pushes
	   every row's real content in by the same amount.
	   Flush left — the table's first column should start where the table does. */
	.sel-col { width: 30px; padding: 0 0.5rem 0 0; text-align: left; }
	.lead {
		display: inline-grid;
		place-items: center;
		width: 16px;
		height: 16px;
		vertical-align: middle;
	}
	/* Stacked, not swapped: both occupy the same cell so nothing moves when the
	   visible one changes. */
	.lead > :global(*) { grid-area: 1 / 1; }
	.lead :global(.lead-glyph) {
		color: var(--color-foreground-subtle);
		transition: opacity 0.1s ease;
		/* Decoration only. Stacked over the checkbox it would otherwise take the
		   click — `opacity: 0` hides an element, it does not excuse it from
		   hit-testing — so ticking a box opened the row instead. */
		pointer-events: none;
	}
	/* With a glyph behind it the box is hidden until wanted; with no glyph there
	   is nothing to reveal, so it stays put. */
	.lead.has-glyph .sel-box { opacity: 0; transition: opacity 0.1s ease; }
	.data-row:hover .lead.has-glyph .sel-box,
	.data-row:focus-within .lead.has-glyph .sel-box,
	.sel-on .lead.has-glyph .sel-box { opacity: 1; }
	.data-row:hover .lead.has-glyph :global(.lead-glyph),
	.data-row:focus-within .lead.has-glyph :global(.lead-glyph),
	.sel-on .lead.has-glyph :global(.lead-glyph) { opacity: 0; }
	/* Select-all is the one control with no glyph to hide behind, so it appears
	   when the pointer is anywhere in the table rather than never. */
	.sel-head { opacity: 0; transition: opacity 0.1s ease; }
	.table-view:hover .sel-head,
	.sel-head:focus-visible,
	.sel-on .sel-head { opacity: 1; }
	/* No hover to reveal anything with. Keep the glyph — it's the fastest way to
	   read a list — and make select-all the way in: tapping it turns every box
	   on via .sel-on, and clearing turns them back off. */
	@media (hover: none) {
		.sel-head { opacity: 1; }
	}
	/* Custom control rather than accent-color on the native widget: the platform
	   checkbox is a different shape, weight and blue in every theme, and it was
	   the one element on the page that didn't belong to the app. The real input
	   is still underneath — only its painting is replaced — so focus, keyboard
	   and screen readers are untouched. */
	.sel-box {
		appearance: none;
		-webkit-appearance: none;
		width: 14px;
		height: 14px;
		margin: 0;
		flex: none;
		display: inline-grid;
		place-content: center;
		vertical-align: middle;
		border: 1.5px solid var(--color-border-strong, var(--color-border));
		border-radius: 4px;
		background: var(--color-surface);
		cursor: pointer;
		transition: background-color 0.12s ease, border-color 0.12s ease;
	}
	.sel-box::before {
		content: '';
		width: 9px;
		height: 9px;
		transform: scale(0);
		transition: transform 0.12s cubic-bezier(0.2, 0.7, 0.3, 1);
		background: var(--color-background);
		clip-path: polygon(14% 44%, 0 65%, 50% 100%, 100% 16%, 80% 0%, 43% 62%);
	}
	.sel-box:hover { border-color: var(--color-foreground-subtle); }
	.sel-box:checked,
	.sel-box:indeterminate {
		background: var(--color-foreground);
		border-color: var(--color-foreground);
	}
	.sel-box:checked::before { transform: scale(1); }
	/* Indeterminate reuses the mark box as a dash — one shape, two states. */
	.sel-box:indeterminate::before {
		transform: scale(1);
		clip-path: inset(42% 8% 42% 8%);
	}
	.sel-box:focus-visible {
		outline: 2px solid var(--color-primary);
		outline-offset: 2px;
	}
	@media (prefers-reduced-motion: reduce) {
		.sel-box, .sel-box::before { transition: none; }
	}
	.data-row.selected {
		background: color-mix(in srgb, var(--color-primary) 8%, transparent);
	}
	.data-row.selected:hover {
		background: color-mix(in srgb, var(--color-primary) 12%, transparent);
	}

	/* Trailing controls: present in the DOM at all times for keyboard reach,
	   revealed on hover so they don't add permanent visual noise. */
	.row-actions-col { width: 40px; }
	.row-actions-cell { padding: 0 0.5rem 0 0; text-align: right; }
	.row-actions {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		opacity: 0;
		transition: opacity 0.12s ease;
	}
	.data-row:hover .row-actions,
	.data-row:focus-within .row-actions { opacity: 1; }
	@media (hover: none) {
		.row-actions { opacity: 1; }
	}

	.bulk-count { font-size: 0.75rem; color: var(--color-foreground); white-space: nowrap; }
	.bulk-sp { flex: 1; }
	.bulk-clear {
		border: none;
		background: none;
		padding: 0;
		font: inherit;
		font-size: 0.75rem;
		color: var(--color-foreground-muted);
		cursor: pointer;
		text-decoration: underline;
	}
	.bulk-clear:hover { color: var(--color-foreground); }

	/* ============================================
	   SKELETON
	   ============================================ */
	.skeleton { display: flex; flex-direction: column; padding-top: 0.75rem; }
	.sk-row {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 0.62rem 0.75rem 0.62rem 0;
		border-bottom: 1px solid var(--color-border);
	}
	.sk-box, .sk-bar {
		display: block;
		height: 9px;
		border-radius: 4px;
		background: linear-gradient(
			90deg,
			var(--color-surface-elevated) 0%,
			color-mix(in srgb, var(--color-foreground) 8%, var(--color-surface-elevated)) 50%,
			var(--color-surface-elevated) 100%
		);
		background-size: 200% 100%;
		animation: sk-sweep 1.4s ease-in-out infinite;
		animation-delay: var(--sk-delay, 0ms);
	}
	.sk-box { width: 14px; height: 14px; border-radius: 3px; flex: none; }
	.sk-narrow { width: 68px; flex: none; }
	@keyframes sk-sweep {
		from { background-position: 200% 0; }
		to { background-position: -200% 0; }
	}
	@media (prefers-reduced-motion: reduce) {
		.sk-box, .sk-bar { animation: none; }
	}

	/* ============================================
	   GROUPING
	   ============================================ */
	/* One control, one look, in all three renderers: a disclosure, the group's
	   value, and its count. In a board it becomes the column head. */
	.group-toggle {
		display: flex;
		align-items: center;
		gap: 7px;
		width: 100%;
		padding: 0.35rem 0.1rem;
		border: none;
		background: transparent;
		color: var(--color-foreground-muted);
		font: inherit;
		font-size: 0.78125rem;
		font-weight: 600;
		text-align: left;
		cursor: pointer;
	}
	.group-toggle:hover { color: var(--color-foreground); }
	.group-toggle:focus-visible { outline: 2px solid var(--color-primary); outline-offset: -2px; }
	.group-toggle :global(.group-chev) {
		color: var(--color-foreground-subtle);
		transition: transform 0.16s ease;
		flex-shrink: 0;
	}
	.group-toggle.closed :global(.group-chev) { transform: rotate(-90deg); }
	.group-count {
		font-family: var(--font-mono);
		font-size: 0.625rem;
		font-weight: 400;
		font-variant-numeric: tabular-nums;
		color: var(--color-foreground-subtle);
		background: var(--color-surface-elevated);
		border-radius: 3px;
		padding: 1px 6px;
	}

	.group-row td {
		padding: 0.55rem 0.75rem 0.15rem 0;
		border-bottom: none;
		background: transparent;
	}
	/* The first group head sits directly under the column head; later ones need
	   air so the groups read as separate blocks. */
	.group-row:not(:first-child) td { padding-top: 1.25rem; }

	.card-groups { display: flex; flex-direction: column; gap: 1.25rem; padding-top: 0.75rem; }
	.card-group { display: flex; flex-direction: column; gap: 0.35rem; }

	/* Board: the card renderer with the groups laid out as columns. */
	.board {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
		gap: 0.85rem;
		padding-top: 1rem;
		align-items: start;
		overflow-x: auto;
	}
	.board-col { display: flex; flex-direction: column; gap: 0.5rem; min-width: 0; }
	.board-stack { display: flex; flex-direction: column; gap: 0.5rem; }

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

	/* The default card body. `.card` itself stays an unstyled button because
	   consumers passing a `card` snippet draw their own surface inside it —
	   giving the button a border would double it. So the surface lives here,
	   which only the built-in branch renders. Left-aligned: these are records,
	   and centred text makes ragged columns of them. */
	.card-content {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 0.3rem;
		text-align: left;
		width: 100%;
		height: 100%;
		padding: 0.85rem 0.9rem;
		border: 1px solid var(--color-border);
		border-radius: 10px;
		background: var(--color-surface);
		transition: background-color 0.12s ease, border-color 0.12s ease;
	}

	.card:hover .card-content {
		background: var(--color-background-hover);
		border-color: var(--color-border-strong, var(--color-border));
	}

	.card-title {
		font-weight: 550;
		font-size: 0.875rem;
		color: var(--color-foreground);
		line-height: 1.35;
		/* Long names must not stretch the column or run to five lines. */
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.card-meta {
		font-size: 0.75rem;
		color: var(--color-foreground-subtle);
		font-variant-numeric: tabular-nums;
	}

	/* ============================================
	   SHARED STYLES
	   Badge classes are defined globally in app.css
	   ============================================ */

	.empty-cell {
		color: var(--color-foreground-subtle);
	}

	/* Expand detail row */
	.data-row.expanded td {
		background: var(--color-background-hover);
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
		border-radius: 8px;
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
