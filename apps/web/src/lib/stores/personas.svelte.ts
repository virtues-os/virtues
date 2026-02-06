/**
 * Persona Store
 *
 * Manages AI persona state with CRUD operations.
 * Personas are fetched from the backend API on init.
 */

import { browser } from '$app/environment';

// ============================================================================
// Types
// ============================================================================

export interface Persona {
	id: string;
	title: string;
	content: string;
	is_system: boolean;
}

export interface CreatePersonaRequest {
	title: string;
	content: string;
}

export interface UpdatePersonaRequest {
	title?: string;
	content?: string;
}

// ============================================================================
// Store Implementation
// ============================================================================

class PersonaStore {
	private _personas = $state<Persona[]>([]);
	private _hidden = $state<string[]>([]);
	private _loading = $state(false);
	private _error = $state<string | null>(null);
	private _initialized = $state(false);

	// Getters for reactive state
	get personas() {
		return this._personas;
	}
	get hidden() {
		return this._hidden;
	}
	get loading() {
		return this._loading;
	}
	get error() {
		return this._error;
	}
	get initialized() {
		return this._initialized;
	}

	// Computed: visible personas (not hidden)
	get visiblePersonas() {
		return this._personas.filter((p) => !this._hidden.includes(p.id));
	}

	/**
	 * Initialize the store by fetching personas from API
	 */
	async init(): Promise<void> {
		if (this._initialized || !browser) return;

		this._loading = true;
		this._error = null;

		try {
			const res = await fetch('/api/personas');
			if (!res.ok) {
				throw new Error(`Failed to fetch personas: ${res.status}`);
			}
			this._personas = await res.json();
			this._initialized = true;
		} catch (e) {
			this._error = e instanceof Error ? e.message : 'Unknown error';
			console.error('Failed to load personas:', e);
		} finally {
			this._loading = false;
		}
	}

	/**
	 * Get a persona by ID
	 */
	getById(id: string): Persona | undefined {
		return this._personas.find((p) => p.id === id);
	}

	/**
	 * Create a new custom persona
	 */
	async create(title: string, content: string): Promise<Persona | null> {
		this._loading = true;
		this._error = null;

		try {
			const res = await fetch('/api/personas', {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ title, content } satisfies CreatePersonaRequest)
			});

			if (!res.ok) {
				throw new Error(`Failed to create persona: ${res.status}`);
			}

			const persona: Persona = await res.json();
			this._personas = [...this._personas, persona];
			return persona;
		} catch (e) {
			this._error = e instanceof Error ? e.message : 'Unknown error';
			console.error('Failed to create persona:', e);
			return null;
		} finally {
			this._loading = false;
		}
	}

	/**
	 * Update an existing persona
	 */
	async update(id: string, updates: UpdatePersonaRequest): Promise<boolean> {
		this._loading = true;
		this._error = null;

		try {
			const res = await fetch(`/api/personas/${id}`, {
				method: 'PUT',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify(updates)
			});

			if (!res.ok) {
				throw new Error(`Failed to update persona: ${res.status}`);
			}

			const updated: Persona = await res.json();
			this._personas = this._personas.map((p) => (p.id === id ? updated : p));
			return true;
		} catch (e) {
			this._error = e instanceof Error ? e.message : 'Unknown error';
			console.error('Failed to update persona:', e);
			return false;
		} finally {
			this._loading = false;
		}
	}

	/**
	 * Hide a persona (soft delete for system, hard delete for custom)
	 */
	async hide(id: string): Promise<boolean> {
		this._loading = true;
		this._error = null;

		try {
			const res = await fetch(`/api/personas/${id}`, {
				method: 'DELETE'
			});

			if (!res.ok) {
				throw new Error(`Failed to hide persona: ${res.status}`);
			}

			// Track hidden status for system personas
			const persona = this._personas.find((p) => p.id === id);
			if (persona?.is_system) {
				this._hidden = [...this._hidden, id];
			}

			// Always remove from visible list (API only returns visible personas anyway)
			this._personas = this._personas.filter((p) => p.id !== id);

			return true;
		} catch (e) {
			this._error = e instanceof Error ? e.message : 'Unknown error';
			console.error('Failed to hide persona:', e);
			return false;
		} finally {
			this._loading = false;
		}
	}

	/**
	 * Unhide a previously hidden system persona
	 */
	async unhide(id: string): Promise<boolean> {
		this._loading = true;
		this._error = null;

		try {
			const res = await fetch(`/api/personas/${id}/unhide`, {
				method: 'POST'
			});

			if (!res.ok) {
				throw new Error(`Failed to unhide persona: ${res.status}`);
			}

			this._hidden = this._hidden.filter((h) => h !== id);
			return true;
		} catch (e) {
			this._error = e instanceof Error ? e.message : 'Unknown error';
			console.error('Failed to unhide persona:', e);
			return false;
		} finally {
			this._loading = false;
		}
	}

	/**
	 * Reset all personas to defaults
	 */
	async reset(): Promise<boolean> {
		this._loading = true;
		this._error = null;

		try {
			const res = await fetch('/api/personas/reset', {
				method: 'POST'
			});

			if (!res.ok) {
				throw new Error(`Failed to reset personas: ${res.status}`);
			}

			this._personas = await res.json();
			this._hidden = [];
			return true;
		} catch (e) {
			this._error = e instanceof Error ? e.message : 'Unknown error';
			console.error('Failed to reset personas:', e);
			return false;
		} finally {
			this._loading = false;
		}
	}

	/**
	 * Force refresh personas from API
	 */
	async refresh(): Promise<void> {
		this._initialized = false;
		await this.init();
	}
}

// Export singleton instance
export const personaStore = new PersonaStore();

// Helper functions for backwards compatibility
export function getPersonaById(id: string): Persona | undefined {
	return personaStore.getById(id);
}

export function getDefaultPersona(): Persona | undefined {
	return personaStore.personas[0];
}
