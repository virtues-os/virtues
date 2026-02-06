/**
 * Global Context Menu Store
 *
 * Provides a centralized state management for context menus throughout the app.
 * Components call contextMenu.show() with their items, and ContextMenuProvider renders them.
 */

export interface ContextMenuItem {
	id: string;
	label: string;
	icon?: string;
	action?: () => void | Promise<void>;
	submenu?: ContextMenuItem[];
	disabled?: boolean;
	checked?: boolean;
	loading?: boolean;
	variant?: 'default' | 'destructive';
	dividerBefore?: boolean;
	dividerAfter?: boolean;
	shortcut?: string;
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
		this.openSubmenuId = itemId;
	}

	/**
	 * Close the currently open submenu
	 */
	closeSubmenu() {
		this.openSubmenuId = null;
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
	 * Adjust position to keep menu within viewport
	 */
	private adjustPosition(pos: ContextMenuPosition): ContextMenuPosition {
		// Menu dimensions (estimate, will be refined after render)
		const menuWidth = 200;
		const menuHeight = 300;
		const padding = 8;

		let { x, y } = pos;

		// Check if we're in a browser environment
		if (typeof window !== 'undefined') {
			const viewportWidth = window.innerWidth;
			const viewportHeight = window.innerHeight;

			// Adjust horizontal position
			if (x + menuWidth + padding > viewportWidth) {
				x = viewportWidth - menuWidth - padding;
			}
			if (x < padding) {
				x = padding;
			}

			// Adjust vertical position
			if (y + menuHeight + padding > viewportHeight) {
				y = viewportHeight - menuHeight - padding;
			}
			if (y < padding) {
				y = padding;
			}
		}

		return { x, y };
	}
}

export const contextMenu = new ContextMenuStore();
