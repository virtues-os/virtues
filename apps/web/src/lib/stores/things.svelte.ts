/**
 * Things Store
 *
 * A "thing" is a folder you can re-enter — a project, pet, goal, topic,
 * anything you want to keep loosely organized. Things accumulate pinned
 * URLs (pages, chats, entities, files, external links). The catch-up
 * memo (`current_status`) sits at the top of every detail view.
 *
 * The `category` column exists on the DB row but is intentionally not
 * surfaced in v1 UX — kept for future use and AI-generated tagging.
 */

import {
	listThings,
	getThing,
	createThing,
	updateThing,
	deleteThing,
	addThingPin as apiAddThingPin,
	removeThingPin as apiRemoveThingPin,
	reorderThingPins as apiReorderThingPins,
	type Thing,
	type ThingSummary,
	type ThingDetail,
	type ThingPin
} from '$lib/api/client';

class ThingsStore {
	things = $state<ThingSummary[]>([]);
	loading = $state(false);
	error = $state<string | null>(null);

	private detailCache = $state<Map<string, ThingDetail>>(new Map());

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

	async loadDetail(id: string, force = false): Promise<ThingDetail> {
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

	getCachedDetail(id: string): ThingDetail | undefined {
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
		this.things = [
			{ ...thing, pin_count: 0 } as ThingSummary,
			...this.things
		];
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
		this.things = this.things.map((t) =>
			t.id === id ? { ...t, ...updated } : t
		);
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

	async addPin(
		thingId: string,
		url: string,
		options?: { name?: string | null; description?: string | null }
	): Promise<ThingPin> {
		const pin = await apiAddThingPin(thingId, url, options);
		if (this.detailCache.has(thingId)) {
			const next = new Map(this.detailCache);
			next.delete(thingId);
			this.detailCache = next;
		}
		this.things = this.things.map((t) =>
			t.id === thingId ? { ...t, pin_count: t.pin_count + 1 } : t
		);
		return pin;
	}

	async removePin(thingId: string, url: string): Promise<void> {
		await apiRemoveThingPin(thingId, url);
		if (this.detailCache.has(thingId)) {
			const next = new Map(this.detailCache);
			next.delete(thingId);
			this.detailCache = next;
		}
		this.things = this.things.map((t) =>
			t.id === thingId ? { ...t, pin_count: Math.max(0, t.pin_count - 1) } : t
		);
	}

	async reorderPins(thingId: string, urls: string[]): Promise<void> {
		await apiReorderThingPins(thingId, urls);
		if (this.detailCache.has(thingId)) {
			const next = new Map(this.detailCache);
			next.delete(thingId);
			this.detailCache = next;
		}
	}
}

export const thingsStore = new ThingsStore();
