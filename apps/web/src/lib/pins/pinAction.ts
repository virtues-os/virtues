/**
 * Pinning, as a verb on anything with a URL.
 *
 * Pinning used to live in exactly one place: `WindowTabBar`'s tab context
 * menu. `pinsStore.add` had a single caller in the whole app. So the only way
 * to pin anything was to open it in a tab first — which meant the only things
 * anyone ever pinned were the destinations already in the sidebar, and Pinned
 * ended up looking like a weak duplicate of the nav directly beneath it.
 *
 * A pin is a route you chose to keep. Anywhere the app already shows you
 * something with a URL — a sidebar row, a table row, a card, a citation, an
 * external link — is somewhere you might want to keep it. This module is the
 * one implementation those surfaces share, so the label, the icon, the
 * toggle semantics and the error handling can't drift between five copies.
 *
 * NOT to be confused with `data_content_bookmark` (ingested saved links —
 * GitHub stars, browser bookmarks) or with `app_notebook_items.role`, which is
 * retrieval scope. Same English word, three different concepts; this is the
 * navigational one.
 */
import { pinsStore } from '$lib/stores/pins.svelte';
import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { iconPickerStore } from '$lib/stores/iconPicker.svelte';

export interface PinTarget {
	/** Route or absolute URL. External `http(s)` urls are allowed and open out. */
	url: string;
	/** What to call it in the sidebar. Falls back to the url when absent. */
	label?: string | null;
	/** Iconify id, e.g. `ri:file-text-line`. */
	icon?: string | null;
}

/** Is this url already pinned? */
export function isPinned(url: string): boolean {
	return pinsStore.isPinned(url);
}

/**
 * Pin or unpin, whichever the current state implies.
 *
 * Returns the resulting state so a caller can update an optimistic control
 * without re-reading the store. Throws on failure rather than swallowing —
 * a pin that silently didn't happen is worse than an error, because the
 * sidebar is the feedback and it simply won't change.
 */
export async function togglePin(target: PinTarget): Promise<boolean> {
	const existing = pinsStore.getByUrl(target.url);
	if (existing) {
		await pinsStore.remove(existing.id);
		return false;
	}
	await pinsStore.add(target.url, target.label ?? null, target.icon ?? null);
	return true;
}

/**
 * "Change icon" for a pinned url — opens the shared picker, which carries the
 * color swatches with it, so a pin's glyph and its color are chosen in one
 * place like every other icon in the app.
 *
 * A pin's rows were dots on purpose ("a pinned thing has no natural glyph").
 * That holds right up until the user wants to pick one; the dot stays as the
 * default for pins that never do.
 */
export function pinIconMenuItem(
	url: string,
	anchor?: { x: number; y: number; width: number; height: number },
): ContextMenuItem | null {
	const pin = pinsStore.getByUrl(url);
	if (!pin) return null;

	return {
		id: 'pin-icon',
		label: 'Change icon',
		icon: 'ri:emotion-line',
		action: () => {
			iconPickerStore.show(
				pin.icon ?? null,
				(icon) => {
					void pinsStore.setIcon(pin.id, icon);
				},
				{
					color: pin.color ?? null,
					onColorSelect: (color) => {
						void pinsStore.setColor(pin.id, color);
					},
					anchor,
				},
			);
		},
	};
}

/**
 * The shared context-menu entry. Drop it into any menu builder:
 *
 *   contextMenu.show(pos, [...myItems, pinMenuItem({ url, label, icon })]);
 *
 * `dividerBefore` defaults on because this is nearly always appended to a
 * menu of item-specific actions and wants separating from them.
 */
export function pinMenuItem(
	target: PinTarget,
	opts: { dividerBefore?: boolean } = {},
): ContextMenuItem {
	const pinned = isPinned(target.url);
	return {
		id: 'pin-sidebar',
		// Says what happens, and names where it goes — "Pin" alone doesn't
		// answer "pin it to what?" when three different things in this app
		// could plausibly be the destination. The destination has a name now,
		// so the verb uses it: the Desk is where things you're working on go,
		// and "add to desk" is the same sentence the zone header speaks.
		label: pinned ? 'Take off the desk' : 'Add to desk',
		icon: pinned ? 'ri:pushpin-fill' : 'ri:pushpin-line',
		dividerBefore: opts.dividerBefore ?? true,
		action: async () => {
			try {
				await togglePin(target);
			} catch (err) {
				console.error('[pins] toggle failed:', err);
			}
		},
	};
}
