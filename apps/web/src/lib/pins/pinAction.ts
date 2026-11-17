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
 * GitHub stars, browser bookmarks) or with `app_project_items.role`, which is
 * retrieval scope. Same English word, three different concepts; this is the
 * navigational one.
 *
 * Pins live under "Pinned" at the top of the Home panel. They were "the
 * Desk" behind a Home tile on the rail; the tile is gone (2026-09-21) and so
 * is the word — "pin" and "unpin" are the verbs, and "Pinned" is the place.
 * A chat, a project, an applet or a page can be pinned; anything with a url
 * can, in fact, but those four are what the menus offer.
 */
import { pinsStore } from '$lib/stores/pins.svelte';
import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { iconPickerStore } from '$lib/stores/iconPicker.svelte';
import { ownedRef, pinIdentity, setRefLook } from '$lib/refs/identity.svelte';
import { clothFor } from '$lib/sidebar/pin-colors';
import { isNewChat } from '$lib/components/chat/state/chatRoute';

export interface PinTarget {
	/** Route or absolute URL. External `http(s)` urls are allowed and open out. */
	url: string;
	/**
	 * What to call it, kept only for a pin with no record behind it (an
	 * external URL, an app screen). A chat, page or project is named by
	 * itself, whatever was passed here.
	 */
	label?: string | null;
	/** Iconify id, e.g. `mynaui:file-text`. Same rule as `label`. */
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
	const owned = ownedRef(target.url);
	await pinsStore.add(target.url, owned ? null : (target.label ?? null), owned ? null : (target.icon ?? null));
	return true;
}

/**
 * Whether a dragged url can become a pin. A new chat has no thing behind it
 * yet, so there is nothing to keep.
 */
export function canPin(url: string): boolean {
	return !!url && !isNewChat(url);
}

/**
 * Pin `target` into the gap before pin number `gap` (`pins.length` is the
 * end), from a drop. A url that is already pinned moves to that gap instead
 * of pinning twice. A new pin shows in the gap before the box answers
 * (`pinsStore.insertAt`), and the store changes before this first awaits, so
 * a drop handler's own cleanup never sees an empty Pinned in between.
 */
export async function pinAtGap(target: PinTarget, gap: number): Promise<void> {
	if (!canPin(target.url)) return;
	const from = pinsStore.pins.findIndex((p) => p.url === target.url);
	if (from >= 0) {
		const to = from < gap ? gap - 1 : gap;
		await pinsStore.reorder(from, Math.min(to, pinsStore.pins.length - 1));
		return;
	}
	const owned = ownedRef(target.url);
	await pinsStore.insertAt(
		gap,
		target.url,
		owned ? null : (target.label ?? null),
		owned ? null : (target.icon ?? null),
		target.label,
	);
}

/**
 * "Change icon" for a pinned url — opens the shared picker, which carries the
 * color swatches with it, so a glyph and its color are chosen in one place
 * like every other icon in the app.
 *
 * The icon and color are the thing's: picking one here changes the chat,
 * page or project itself, everywhere it shows. Only a pin with no record
 * behind it keeps them on the pin. A thing with no icon shows the dot.
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
		icon: 'mynaui:smile',
		action: () => {
			const look = pinIdentity(pin);
			iconPickerStore.show(
				look.icon,
				(icon) => {
					void setRefLook(url, { icon });
				},
				{
					color: look.color,
					onColorSelect: (color) => {
						void setRefLook(url, { color });
					},
					anchor,
					label: look.title,
					autoColor: clothFor({ url }),
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
		// "Pin" used to be "Add to desk", when the destination was a room with
		// a name. Now the destination is the section labeled Pinned, at the top
		// of the Home panel, and the plain verb is the one the label speaks.
		label: pinned ? 'Unpin' : 'Pin',
		icon: pinned ? 'mynaui:pin-solid' : 'mynaui:pin',
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
