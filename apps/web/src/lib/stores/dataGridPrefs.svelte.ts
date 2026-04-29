/**
 * DataGrid Preferences Store
 *
 * Persists view mode and density preferences per entity type to localStorage.
 */

const STORAGE_KEY = 'virtues-datagrid-prefs';

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
}

class DataGridPrefsStore {
	private prefs = $state<DataGridPrefs>({ viewModes: {}, densities: {} });

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
					if (isValidViewMode(value)) validatedModes[key] = value;
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
}

export const dataGridPrefs = new DataGridPrefsStore();
