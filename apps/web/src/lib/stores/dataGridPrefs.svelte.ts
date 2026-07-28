/**
 * DataGrid Preferences Store
 *
 * Persists view mode, density, and grouping preferences per entity type to
 * localStorage.
 */

const STORAGE_KEY = 'virtues-datagrid-prefs';

/**
 * Two modes, not three. A board *is* the card view with a grouping applied —
 * it was a third mode that could only differ from cards by being grouped, so
 * the user had to pick "board" and *then* pick a group to get anywhere.
 * Grouping is an orthogonal axis now: set a group in cards and you get columns.
 */
export type ViewMode = 'table' | 'grid';
export type Density = 'compact' | 'comfortable';

const VALID_VIEW_MODES: ViewMode[] = ['table', 'grid'];
const VALID_DENSITIES: Density[] = ['compact', 'comfortable'];

function isValidViewMode(value: unknown): value is ViewMode {
	return typeof value === 'string' && VALID_VIEW_MODES.includes(value as ViewMode);
}

function isValidDensity(value: unknown): value is Density {
	return typeof value === 'string' && VALID_DENSITIES.includes(value as Density);
}

interface DataGridPrefs {
	viewModes: Record<string, ViewMode>;
	densities: Record<string, Density>;
	/** Column key to group by, or '' for ungrouped. */
	groupBy: Record<string, string>;
}

class DataGridPrefsStore {
	private prefs = $state<DataGridPrefs>({ viewModes: {}, densities: {}, groupBy: {} });

	constructor() {
		this.load();
	}

	private load(): void {
		if (typeof window === 'undefined') return;
		try {
			const stored = localStorage.getItem(STORAGE_KEY);
			if (!stored) return;
			const parsed = JSON.parse(stored);
			if (!parsed || typeof parsed !== 'object') return;

			if (parsed.viewModes && typeof parsed.viewModes === 'object') {
				const validatedModes: Record<string, ViewMode> = {};
				for (const [key, value] of Object.entries(parsed.viewModes)) {
					// A stored 'board' becomes 'grid': board was cards-with-columns, so
					// cards is what that preference meant. Dropping it as invalid would
					// silently drop the user back to the table instead.
					if (value === 'board') validatedModes[key] = 'grid';
					else if (isValidViewMode(value)) validatedModes[key] = value;
				}
				this.prefs.viewModes = validatedModes;
			}

			if (parsed.densities && typeof parsed.densities === 'object') {
				const validatedDensities: Record<string, Density> = {};
				for (const [key, value] of Object.entries(parsed.densities)) {
					if (isValidDensity(value)) validatedDensities[key] = value;
				}
				this.prefs.densities = validatedDensities;
			}
			if (parsed.groupBy && typeof parsed.groupBy === 'object') {
				const validated: Record<string, string> = {};
				for (const [key, value] of Object.entries(parsed.groupBy)) {
					if (typeof value === 'string') validated[key] = value;
				}
				this.prefs.groupBy = validated;
			}
		} catch (e) {
			console.warn('[DataGridPrefs] Failed to load preferences:', e);
		}
	}

	private persist(): void {
		if (typeof window === 'undefined') return;
		try {
			localStorage.setItem(STORAGE_KEY, JSON.stringify(this.prefs));
		} catch (e) {
			console.warn('[DataGridPrefs] Failed to persist preferences:', e);
		}
	}

	hasViewMode(entityType: string): boolean {
		return entityType in this.prefs.viewModes;
	}

	getViewMode(entityType: string): ViewMode {
		return this.prefs.viewModes[entityType] || 'table';
	}

	setViewMode(entityType: string, mode: ViewMode): void {
		this.prefs.viewModes[entityType] = mode;
		this.persist();
	}

	hasDensity(entityType: string): boolean {
		return entityType in this.prefs.densities;
	}

	getDensity(entityType: string): Density {
		return this.prefs.densities[entityType] || 'comfortable';
	}

	setDensity(entityType: string, density: Density): void {
		this.prefs.densities[entityType] = density;
		this.persist();
	}

	hasGroupBy(entityType: string): boolean {
		return entityType in this.prefs.groupBy;
	}

	/** '' means ungrouped. */
	getGroupBy(entityType: string): string {
		return this.prefs.groupBy[entityType] ?? '';
	}

	setGroupBy(entityType: string, key: string): void {
		this.prefs.groupBy[entityType] = key;
		this.persist();
	}
}

export const dataGridPrefs = new DataGridPrefsStore();
