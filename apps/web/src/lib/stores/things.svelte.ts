/**
 * Things Store
 *
 * A "thing" is a plain entity — a project, pet, goal, topic, anything you want
 * to name and keep around. Organization/membership now lives in Spaces, not on
 * the Thing itself.
 *
 * The `category` column exists on the DB row but is intentionally not surfaced
 * in v1 UX — kept for future use and AI-generated tagging.
 */

import {
	listThings,
	getThing,
	createThing,
	updateThing,
	deleteThing,
	type Thing,
	type ThingSummary
} from '$lib/api/client';

class ThingsStore {
	things = $state<ThingSummary[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);

	private detailCache = $state<Map<string, Thing>>(new Map());

	async load(category?: string): Promise<void> {
		this.loading = true;
		this.error = null;
		try {
			const res = await listThings(category);
			this.things = res.things;
		} catch (e) {
			console.error('[ThingsStore] Failed to load things:', e);
			this.error = e instanceof Error ? e.message : 'Failed to load things';
			this.things = [];
		} finally {
			this.loading = false;
		}
	}

	async loadDetail(id: string, force = false): Promise<Thing> {
		if (!force) {
			const cached = this.detailCache.get(id);
			if (cached) return cached;
		}
		const detail = await getThing(id);
		const next = new Map(this.detailCache);
		next.set(id, detail);
		this.detailCache = next;
		return detail;
	}

	getCachedDetail(id: string): Thing | undefined {
		return this.detailCache.get(id);
	}

	async create(
		name: string,
		options?: {
			category?: string | null;
			icon?: string | null;
			description?: string | null;
		}
	): Promise<Thing> {
		const thing = await createThing(name, options);
		// Add to top of list (most-recently-updated)
		this.things = [thing as ThingSummary, ...this.things];
		return thing;
	}

	async update(
		id: string,
		updates: {
			name?: string;
			category?: string | null;
			icon?: string | null;
			description?: string | null;
		}
	): Promise<Thing> {
		const updated = await updateThing(id, updates);
		this.things = this.things.map((t) => (t.id === id ? { ...t, ...updated } : t));
		const cached = this.detailCache.get(id);
		if (cached) {
			const next = new Map(this.detailCache);
			next.set(id, { ...cached, ...updated });
			this.detailCache = next;
		}
		return updated;
	}

	async remove(id: string): Promise<void> {
		await deleteThing(id);
		this.things = this.things.filter((t) => t.id !== id);
		if (this.detailCache.has(id)) {
			const next = new Map(this.detailCache);
			next.delete(id);
			this.detailCache = next;
		}
	}
}

export const thingsStore = new ThingsStore();
