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
	updatePin,
	deletePin,
	reorderPins,
	type Pin
} from '$lib/api/client';

/** The id a pin wears while the box has not answered yet. */
const PROVISIONAL = 'provisional:';
let provisionalCount = 0;

class PinsStore {
	pins = $state<Pin[]>([]);
	loaded = $state(false);
	/**
	 * A pin that began as a provisional one keeps the provisional id as its
	 * render key, so its row stays the same element when the box's pin
	 * replaces it, instead of leaving and arriving again.
	 */
	private keys = new Map<string, string>();

	/** The key a list of pins should use for this pin's row. */
	keyOf(pin: Pin): string {
		return this.keys.get(pin.id) ?? pin.id;
	}

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

	/**
	 * Pin a URL that is not pinned yet into the gap before pin number `gap`
	 * (`pins.length` is the end), from a drop.
	 *
	 * Optimistic: a provisional pin takes the gap at once, so the row lands
	 * where it was dropped and an empty Pinned does not close while the box
	 * answers. The box's pin then replaces it, and the order is saved. If the
	 * box refuses the pin, the provisional one goes and the error is thrown.
	 * If it takes the pin but not the order, the list reloads to agree with it.
	 */
	async insertAt(gap: number, url: string, label: string | null, icon: string | null, title?: string | null) {
		const key = `${PROVISIONAL}${++provisionalCount}`;
		const provisional: Pin = {
			id: key,
			url,
			title: title ?? label ?? undefined,
			label,
			icon,
			sort_order: gap,
			pinned_at: new Date().toISOString(),
			color: null,
		};
		const at = Math.max(0, Math.min(gap, this.pins.length));
		this.pins = [...this.pins.slice(0, at), provisional, ...this.pins.slice(at)].map((p, i) => ({ ...p, sort_order: i }));

		let pin: Pin;
		try {
			pin = await createPin({ url, label, icon });
		} catch (e) {
			this.pins = this.pins.filter((p) => p.id !== key);
			console.error('[pinsStore] insertAt failed', e);
			throw e;
		}

		// The box may answer with a pin it already had for this url, written
		// its own way, and a load() may have run meanwhile: whatever is in the
		// list now, the box's pin takes the provisional pin's place.
		const placed = this.pins.findIndex((p) => p.id === key);
		const next = this.pins.filter((p) => p.id !== key && p.id !== pin.id);
		const index =
			placed >= 0 ? this.pins.slice(0, placed).filter((p) => p.id !== pin.id).length : Math.min(at, next.length);
		next.splice(index, 0, pin);
		this.keys.set(pin.id, key);
		this.pins = next.map((p, i) => ({ ...p, sort_order: i }));

		try {
			await reorderPins(next.map((p) => p.url));
		} catch (e) {
			console.error('[pinsStore] insertAt reorder failed', e);
			await this.load();
		}
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

	/**
	 * Set (or clear, with null) a pin's ribbon colour.
	 *
	 * Optimistic like reorder, and for the same reason: the sidebar IS the
	 * feedback, so a color that only appears after a round-trip reads as the
	 * click not registering.
	 */
	async setColor(id: string, color: string | null) {
		const previous = this.pins;
		this.pins = this.pins.map((p) => (p.id === id ? { ...p, color } : p));
		try {
			await updatePin(id, { color });
		} catch (e) {
			this.pins = previous;
			console.error('[pinsStore] setColor failed', e);
			throw e;
		}
	}

	/**
	 * The pin's own label. Only a pin with no record behind it (an external
	 * URL, an app screen) has one; everything else is named by the thing
	 * (refs/identity).
	 */
	async setLabel(id: string, label: string | null) {
		const previous = this.pins;
		this.pins = this.pins.map((p) => (p.id === id ? { ...p, label, title: label ?? p.url } : p));
		try {
			await updatePin(id, { label });
		} catch (e) {
			this.pins = previous;
			console.error('[pinsStore] setLabel failed', e);
			throw e;
		}
	}

	/** Same contract as `setColor`, for the pin's glyph. */
	async setIcon(id: string, icon: string | null) {
		const previous = this.pins;
		this.pins = this.pins.map((p) => (p.id === id ? { ...p, icon } : p));
		try {
			await updatePin(id, { icon });
		} catch (e) {
			this.pins = previous;
			console.error('[pinsStore] setIcon failed', e);
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
