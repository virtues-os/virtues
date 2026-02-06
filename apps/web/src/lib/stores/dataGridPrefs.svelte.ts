/**
 * DataGrid Preferences Store
 *
 * Persists view mode preferences per entity type to localStorage.
 * Each entity type (person, place, org, etc.) can have its own view mode.
 */

const STORAGE_KEY = 'virtues-datagrid-prefs';

export type ViewMode = 'table' | 'list' | 'grid';

const VALID_VIEW_MODES: ViewMode[] = ['table', 'list', 'grid'];

function isValidViewMode(value: unknown): value is ViewMode {
	return typeof value === 'string' && VALID_VIEW_MODES.includes(value as ViewMode);
}

interface DataGridPrefs {
	viewModes: Record<string, ViewMode>;
}

class DataGridPrefsStore {
	private prefs = $state<DataGridPrefs>({ viewModes: {} });

	constructor() {
		this.load();
	}

	private load(): void {
		if (typeof window === 'undefined') return;
		try {
			const stored = localStorage.getItem(STORAGE_KEY);
			if (stored) {
				const parsed = JSON.parse(stored);
				// Validate the parsed data structure
				if (parsed && typeof parsed === 'object' && parsed.viewModes && typeof parsed.viewModes === 'object') {
					// Validate each view mode value
					const validatedModes: Record<string, ViewMode> = {};
					for (const [key, value] of Object.entries(parsed.viewModes)) {
						if (isValidViewMode(value)) {
							validatedModes[key] = value;
						}
					}
					// Mutate existing state instead of reassigning for proper reactivity
					this.prefs.viewModes = validatedModes;
				}
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

	/**
	 * Get the view mode for a specific entity type.
	 * Defaults to 'table' if not set.
	 */
	getViewMode(entityType: string): ViewMode {
		return this.prefs.viewModes[entityType] || 'table';
	}

	/**
	 * Set the view mode for a specific entity type.
	 * Persists to localStorage immediately.
	 */
	setViewMode(entityType: string, mode: ViewMode): void {
		this.prefs.viewModes[entityType] = mode;
		this.persist();
	}
}

export const dataGridPrefs = new DataGridPrefsStore();
