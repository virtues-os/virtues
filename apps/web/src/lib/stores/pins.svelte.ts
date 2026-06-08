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

	isPinned(url: string): boolean {
		return this.pins.some((p) => p.url === url);
	}

	getByUrl(url: string): Pin | undefined {
		return this.pins.find((p) => p.url === url);
	}
}

export const pinsStore = new PinsStore();
