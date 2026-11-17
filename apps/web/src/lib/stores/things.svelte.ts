/**
 * Things: what goes on in the person's life and they named, like a garden, a
 * bike or guitar. One list for every view that shows or picks one: the wiki's
 * Things room, a thing's page, and the "under" menus on a project and a task.
 */
import { createThing, deleteThing, listThings, updateThing, type ThingApi } from '$lib/wiki/api';

class ThingsStore {
	things = $state<ThingApi[]>([]);
	/** A list has arrived at least once, so an empty one means none. */
	loaded = $state(false);
	loading = $state(false);
	error = $state<string | null>(null);

	async load(): Promise<void> {
		this.loading = true;
		this.error = null;
		try {
			this.things = await listThings();
			this.loaded = true;
		} catch (e) {
			// Keep the last good list: a failed reload is not an empty one.
			this.error = e instanceof Error ? e.message : "Your server couldn't load your things";
		} finally {
			this.loading = false;
		}
	}

	/** Load once. A view that only needs names calls this and reads `things`. */
	ensure(): void {
		if (!this.loaded && !this.loading) void this.load();
	}

	byId(id: string | null | undefined): ThingApi | undefined {
		return id ? this.things.find((t) => t.id === id) : undefined;
	}

	async create(title: string): Promise<ThingApi> {
		const thing = await createThing(title);
		this.things = [thing, ...this.things];
		return thing;
	}

	async update(
		id: string,
		fields: Partial<Pick<ThingApi, 'title' | 'summary' | 'started_at' | 'ended_at'>>,
	): Promise<ThingApi> {
		const saved = await updateThing(id, fields);
		this.put(saved);
		return saved;
	}

	async remove(id: string): Promise<void> {
		await deleteThing(id);
		this.things = this.things.filter((t) => t.id !== id);
	}

	/** Replace one thing's row, or add it. */
	put(thing: ThingApi): void {
		this.things = this.things.some((t) => t.id === thing.id)
			? this.things.map((t) => (t.id === thing.id ? thing : t))
			: [thing, ...this.things];
	}
}

export const thingsStore = new ThingsStore();
