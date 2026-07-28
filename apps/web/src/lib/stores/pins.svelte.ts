/**
 * Sidebar pins store.
 *
 * Thin wrapper around `/api/pins`. Holds the user's pin list in memory so
 * the sidebar can render it without each render firing a fetch. Other
 * surfaces (page header pin button, day-page pin, etc.) call
 * `pinsStore.add(...)` / `remove(...)` and the sidebar updates via the
 * shared `$state`.
 */
import {
	listPins,
	createPin,
	deletePin,
	reorderPins,
	type Pin
} from '$lib/api/client';

class PinsStore {
	pins = $state<Pin[]>([]);
	loaded = $state(false);

	async load() {
		try {
			this.pins = await listPins();
			this.loaded = true;
		} catch (e) {
			console.error('[pinsStore] load failed', e);
		}
	}

	/** Pin a URL. Idempotent — pinning twice returns the existing pin. */
	async add(url: string, label?: string | null, icon?: string | null) {
		const pin = await createPin({ url, label, icon });
		// Replace if exists, else append.
		const idx = this.pins.findIndex((p) => p.url === pin.url);
		if (idx >= 0) this.pins[idx] = pin;
		else this.pins = [...this.pins, pin];
		return pin;
	}

	async remove(id: string) {
		await deletePin(id);
		this.pins = this.pins.filter((p) => p.id !== id);
	}

	/**
	 * Move the pin at `from` to index `to`.
	 *
	 * Optimistic: the list reorders locally first, because a drag that only
	 * settles after a round-trip reads as a dropped drag. On failure the
	 * previous order is put back — silently accepting the reject would leave
	 * the sidebar disagreeing with the box until the next load().
	 *
	 * `PUT /api/pins/reorder` takes the full url list and assigns sort_order
	 * by position, so it has to be sent whole rather than as a delta.
	 */
	async reorder(from: number, to: number) {
		if (from === to) return;
		if (from < 0 || to < 0 || from >= this.pins.length || to >= this.pins.length) return;

		const previous = this.pins;
		const next = [...this.pins];
		const [moved] = next.splice(from, 1);
		next.splice(to, 0, moved);
		// Keep sort_order in step with position so anything reading the field
		// (rather than array order) doesn't see stale numbers before reload.
		this.pins = next.map((p, i) => ({ ...p, sort_order: i }));

		try {
			await reorderPins(next.map((p) => p.url));
		} catch (e) {
			this.pins = previous;
			console.error('[pinsStore] reorder failed', e);
			throw e;
		}
	}

	isPinned(url: string): boolean {
		return this.pins.some((p) => p.url === url);
	}

	getByUrl(url: string): Pin | undefined {
		return this.pins.find((p) => p.url === url);
	}
}

export const pinsStore = new PinsStore();
