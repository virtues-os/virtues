/**
 * Shared types for the datagrid component family.
 *
 * Column<T>, RowMeta, SortDir live in UniversalDataGrid.svelte's module script.
 * This file is for filter-related types and helpers that are also imported
 * by sibling components (chip / rail / summary).
 */

export type FilterValue = string | string[] | null;

export interface FilterOption {
	value: string;
	label: string;
	/** Reuses the .badge-* classes (gray/blue/green/yellow/red/purple/orange/pink). */
	badgeColor?: string;
	icon?: string;
}

interface FilterDefBase<T> {
	id: string;
	label: string;
	field?: keyof T;
	predicate?: (item: T, value: FilterValue) => boolean;
	defaultValue?: FilterValue;
	/** false = always-on (cannot be cleared from chip rail). Default true. */
	removable?: boolean;
	/** Suppress from "+ Filter" menu but keep predicate active when value is set. */
	hidden?: boolean;
}

export type EnumFilterDef<T> = FilterDefBase<T> & {
	kind: 'enum';
	options: FilterOption[];
};

export type MultiFilterDef<T> = FilterDefBase<T> & {
	kind: 'multi';
	options: FilterOption[];
};

export type AsyncFilterDef<T> = FilterDefBase<T> & {
	kind: 'async';
	loadOptions: () => Promise<FilterOption[]>;
	searchable?: boolean;
	placeholder?: string;
};

export type FilterDef<T> = EnumFilterDef<T> | MultiFilterDef<T> | AsyncFilterDef<T>;

// ────────────────────────────────────────────────────────────────────────────
// Server-side pagination. Passing a `server` source to the grid switches its
// pipeline from client-side (search/filter/sort/page over `items`) to
// forwarding those inputs as a query. Offset-based on purpose — at one life's
// scale an indexed ORDER BY ... LIMIT/OFFSET is plenty, and the shape can grow
// cursor fields later without breaking consumers.
// ────────────────────────────────────────────────────────────────────────────

export interface GridQuery {
	offset: number;
	limit: number;
	/** Debounced search text ('' when empty). */
	search: string;
	/** Active column sort, or null. Only offer `sortable` columns the server can order by. */
	sort: { key: string; dir: 'asc' | 'desc' } | null;
	/** Active filter values, keyed by FilterDef id. */
	filters: Record<string, FilterValue>;
	/** The grid's `serverExtra` prop, verbatim — consumer-owned query context
	 *  (e.g. a chip row rendered outside the grid). Changing it refetches and
	 *  resets to page 1, and it participates in the page cache key. */
	extra?: Record<string, unknown>;
}

export interface GridPage<T> {
	items: T[];
	/** Total rows matching the query across all pages. */
	total: number;
}

export type GridServerSource<T> = (query: GridQuery) => Promise<GridPage<T>>;

/**
 * True when a filter value is "set" — i.e. should narrow the result set.
 * Empty string, null, and empty array all count as "not set".
 */
export function isFilterActive(v: FilterValue): boolean {
	if (v == null) return false;
	if (Array.isArray(v)) return v.length > 0;
	return v !== '';
}

/**
 * Apply a single filter to an item. Used by the grid's derived pipeline.
 * Falls back to equality on `field` if no `predicate` is provided.
 */
export function applyFilter<T>(item: T, def: FilterDef<T>, value: FilterValue): boolean {
	if (!isFilterActive(value)) return true;
	if (def.predicate) return def.predicate(item, value);
	if (!def.field) return true;
	const itemValue = item[def.field];
	const itemStr = itemValue == null ? '' : String(itemValue);
	if (Array.isArray(value)) return value.includes(itemStr);
	return itemStr === value;
}

/**
 * Pretty-print a filter's current value for the chip label / summary line.
 * `Status: Enabled` or `Trigger: cron, manual` or `Applet: Day Summary`.
 */
export function describeFilter<T>(
	def: FilterDef<T>,
	value: FilterValue,
	loadedOptions?: FilterOption[]
): string {
	if (!isFilterActive(value)) return def.label;
	const options =
		def.kind === 'async' ? (loadedOptions ?? []) : (def as EnumFilterDef<T> | MultiFilterDef<T>).options;
	const lookup = (v: string): string => options.find((o) => o.value === v)?.label ?? v;
	if (Array.isArray(value)) return `${def.label}: ${value.map(lookup).join(', ')}`;
	return `${def.label}: ${lookup(value as string)}`;
}
