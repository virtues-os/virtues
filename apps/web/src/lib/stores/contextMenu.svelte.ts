/**
 * Global Context Menu Store
 *
 * Provides a centralized state management for context menus throughout the app.
 * Components call contextMenu.show() with their items, and ContextMenuProvider renders them.
 */

import { fitMenu } from '$lib/components/contextMenu/fit';

export interface ContextMenuItem {
	id: string;
	label: string;
	/** A second line, for a row that explains a choice rather than naming one. */
	description?: string;
	icon?: string;
	/**
	 * Paints the icon. The only caller is a swatch — a menu row that IS a
	 * color choice — so it takes a CSS color (`var(--cat-orange)`), not a
	 * hex, and the theme's own override decides what that means.
	 */
	iconColor?: string;
	action?: () => void | Promise<void>;
	submenu?: ContextMenuItem[];
	disabled?: boolean;
	checked?: boolean;
	/**
	 * A row that is one of a set of choices (a callout's kind, a column's
	 * alignment): `checked` then reads as selected, not as "you are here".
	 */
	role?: 'menuitemradio' | 'menuitemcheckbox';
	loading?: boolean;
	variant?: 'default' | 'destructive';
	dividerBefore?: boolean;
	dividerAfter?: boolean;
	shortcut?: string;
	onMouseEnter?: () => void;
	onMouseLeave?: () => void;
}

export interface ContextMenuPosition {
	x: number;
	y: number;
}

export interface ContextMenuAnchor {
	x: number;
	y: number;
	width: number;
	height: number;
}

export type ContextMenuPlacement = 'top-start' | 'top-end' | 'bottom-start' | 'bottom-end' | 'left-start' | 'left-end' | 'right-start' | 'right-end';

class ContextMenuStore {
	// Core state
	visible = $state(false);
	position = $state<ContextMenuPosition>({ x: 0, y: 0 });
	items = $state<ContextMenuItem[]>([]);

	// Anchor-based positioning (for Floating UI)
	anchor = $state<ContextMenuAnchor | null>(null);
	placement = $state<ContextMenuPlacement>('bottom-start');

	// Keyboard navigation state
	focusedIndex = $state(-1);
	openSubmenuId = $state<string | null>(null);

	// Loading state for async actions
	loadingItemId = $state<string | null>(null);

	/**
	 * Show the context menu at the given position with the provided items
	 * @param pos - Fallback position (used if no anchor provided)
	 * @param items - Menu items to display
	 * @param options - Optional anchor and placement for Floating UI positioning
	 */
	show(
		pos: ContextMenuPosition,
		items: ContextMenuItem[],
		options?: { anchor?: ContextMenuAnchor; placement?: ContextMenuPlacement }
	) {
		// Store anchor for Floating UI positioning in the provider
		this.anchor = options?.anchor ?? null;
		this.placement = options?.placement ?? 'bottom-start';

		// Use fallback position adjustment if no anchor provided
		const adjustedPos = this.anchor ? pos : this.adjustPosition(pos);

		this.position = adjustedPos;
		this.items = items;
		this.focusedIndex = -1;
		this.openSubmenuId = null;
		this.loadingItemId = null;
		this.visible = true;
	}

	/**
	 * Hide the context menu
	 */
	hide() {
		this.cancelSubmenuClose();
		this.visible = false;
		this.focusedIndex = -1;
		this.openSubmenuId = null;
		this.loadingItemId = null;
		this.anchor = null;
	}

	/**
	 * Execute an item's action
	 */
	async executeAction(item: ContextMenuItem) {
		if (item.disabled || item.submenu || !item.action) return;

		try {
			this.loadingItemId = item.id;
			await item.action();
		} catch (error) {
			console.error('Context menu action failed:', error);
		} finally {
			this.loadingItemId = null;
			this.hide();
		}
	}

	/**
	 * Open a submenu
	 */
	openSubmenu(itemId: string) {
		this.cancelSubmenuClose();
		this.openSubmenuId = itemId;
	}

	/**
	 * Close the currently open submenu
	 */
	closeSubmenu() {
		this.cancelSubmenuClose();
		this.openSubmenuId = null;
	}

	/**
	 * The pointer left a submenu's row or the submenu itself. Close after a
	 * short grace, so the diagonal trip from the row into the submenu (which
	 * crosses neither) does not shut it; arriving in either cancels.
	 */
	private submenuCloseTimer: ReturnType<typeof setTimeout> | null = null;

	scheduleSubmenuClose(delayMs = 150) {
		this.cancelSubmenuClose();
		this.submenuCloseTimer = setTimeout(() => {
			this.submenuCloseTimer = null;
			this.openSubmenuId = null;
		}, delayMs);
	}

	cancelSubmenuClose() {
		if (this.submenuCloseTimer) {
			clearTimeout(this.submenuCloseTimer);
			this.submenuCloseTimer = null;
		}
	}

	/**
	 * Navigate to next focusable item
	 */
	focusNext() {
		const enabledItems = this.items.filter(i => !i.disabled);
		if (enabledItems.length === 0) return;

		let nextIndex = this.focusedIndex + 1;
		while (nextIndex < this.items.length && this.items[nextIndex].disabled) {
			nextIndex++;
		}

		if (nextIndex >= this.items.length) {
			// Wrap to start
			nextIndex = this.items.findIndex(i => !i.disabled);
		}

		this.focusedIndex = nextIndex;
	}

	/**
	 * Navigate to previous focusable item
	 */
	focusPrevious() {
		const enabledItems = this.items.filter(i => !i.disabled);
		if (enabledItems.length === 0) return;

		let prevIndex = this.focusedIndex - 1;
		while (prevIndex >= 0 && this.items[prevIndex].disabled) {
			prevIndex--;
		}

		if (prevIndex < 0) {
			// Wrap to end
			for (let i = this.items.length - 1; i >= 0; i--) {
				if (!this.items[i].disabled) {
					prevIndex = i;
					break;
				}
			}
		}

		this.focusedIndex = prevIndex;
	}

	/** Reach the row at `index`, when it can be chosen. */
	focusAt(index: number) {
		if (index >= 0 && index < this.items.length && !this.items[index].disabled) this.focusedIndex = index;
	}

	/**
	 * A menu opened from the keyboard (Shift-F10, the menu key) starts on its
	 * first row that can be chosen, as a menu opened so does: the focus then
	 * names that row at once (`aria-activedescendant`), so a screen reader
	 * says the menu opened and reads the row, and Enter chooses it. One
	 * opened by the pointer reaches no row until a key moves.
	 */
	reachFirstRow() {
		if (this.visible && this.focusedIndex < 0) this.focusNext();
	}

	/**
	 * Activate the currently focused item
	 */
	activateFocused() {
		if (this.focusedIndex >= 0 && this.focusedIndex < this.items.length) {
			const item = this.items[this.focusedIndex];
			if (item.submenu) {
				this.openSubmenu(item.id);
			} else {
				this.executeAction(item);
			}
		}
	}

	/**
	 * Where a menu with no anchor opens: at `pos`, kept inside the window and
	 * above the software keyboard (`fitMenu`), by a size guessed until the
	 * menu is drawn; the provider fits it again by its real size then.
	 */
	private adjustPosition(pos: ContextMenuPosition): ContextMenuPosition {
		return fitMenu(pos, { width: 200, height: 300 });
	}
}

export const contextMenu = new ContextMenuStore();
