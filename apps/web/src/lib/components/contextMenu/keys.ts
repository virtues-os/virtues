/**
 * The keys an open context menu answers: arrows, Tab, Home, End, Page Up
 * and Page Down to move, a letter to reach the next row it starts, Enter or
 * Space to choose, Escape to close, Right and Left for a submenu.
 *
 * Taken first, on the window in the capture phase, and kept from everything
 * else: a menu opens while the focus stays where it was (a block page's
 * editor keeps it, so its menus can act on its selection), and a key
 * reaching that editor as well would edit the page behind the menu. Enter
 * would split the line under the caret while the menu acted, Tab nest the
 * list item, Backspace join two blocks, a letter type. So every key that
 * edits or moves the caret is the menu's while it is open; a letter with
 * ⌘ or Ctrl is left to the app, as its shortcut.
 */

import { inComposition } from '$lib/utils/ime';

export interface MenuKeyTarget {
	readonly visible: boolean;
	readonly openSubmenuId: string | null;
	readonly focusedIndex: number;
	readonly items: readonly { id: string; label?: string; disabled?: boolean; submenu?: unknown }[];
	closeSubmenu(): void;
	hide(): void;
	focusNext(): void;
	focusPrevious(): void;
	/** Reach the row at `index`. */
	focusAt(index: number): void;
	openSubmenu(id: string): void;
	activateFocused(): void;
}

/** The rows that can be reached, by index. */
function reachable(menu: MenuKeyTarget): number[] {
	return menu.items.flatMap((item, i) => (item.disabled ? [] : [i]));
}

/** The next row after the one reached whose label starts with `char`, wrapping; -1 for none. */
function rowStarting(menu: MenuKeyTarget, char: string): number {
	const rows = reachable(menu);
	const after = rows.filter((i) => i > menu.focusedIndex);
	const lower = char.toLocaleLowerCase();
	return [...after, ...rows].find((i) => (menu.items[i].label ?? '').trim().toLocaleLowerCase().startsWith(lower)) ?? -1;
}

/** Answer `menu`'s keys while it is open. Returns the way to stop. */
export function listenForMenuKeys(menu: MenuKeyTarget, target: Window = window): () => void {
	const onKeydown = (e: KeyboardEvent) => {
		if (!menu.visible || inComposition(e)) return;
		switch (e.key) {
			case 'Escape':
				if (menu.openSubmenuId) menu.closeSubmenu();
				else menu.hide();
				break;
			case 'ArrowDown':
				menu.focusNext();
				break;
			case 'ArrowUp':
				menu.focusPrevious();
				break;
			case 'Tab':
				if (e.shiftKey) menu.focusPrevious();
				else menu.focusNext();
				break;
			case 'Home':
			case 'PageUp': {
				const first = reachable(menu)[0];
				if (first !== undefined) menu.focusAt(first);
				break;
			}
			case 'End':
			case 'PageDown': {
				const last = reachable(menu).at(-1);
				if (last !== undefined) menu.focusAt(last);
				break;
			}
			case 'ArrowRight': {
				const item = menu.focusedIndex >= 0 ? menu.items[menu.focusedIndex] : undefined;
				if (item?.submenu) menu.openSubmenu(item.id);
				break;
			}
			case 'ArrowLeft':
				menu.closeSubmenu();
				break;
			case 'Enter':
			case ' ':
				menu.activateFocused();
				break;
			case 'Backspace':
			case 'Delete':
				break;
			default: {
				// A letter reaches the next row it starts, and types nothing
				// behind the menu; with ⌘ or Ctrl it is the app's shortcut.
				if (e.key.length !== 1 || e.metaKey || e.ctrlKey) return;
				const row = rowStarting(menu, e.key);
				if (row >= 0) menu.focusAt(row);
			}
		}
		e.preventDefault();
		e.stopImmediatePropagation();
	};
	target.addEventListener('keydown', onKeydown, { capture: true });
	return () => target.removeEventListener('keydown', onKeydown, { capture: true });
}
