/**
 * DnD Manager (Simplified)
 *
 * Handles drag-and-drop for window management (tabs + split).
 * Sidebar DnD is separate (uses different type).
 *
 * Supported Operations:
 * - Tab reorder within same pane = REORDER
 * - Tab → Split Overlay = MOVE (tab moves to that pane)
 * - Tab cross-pane = MOVE (tab moves to other pane)
 * - Sidebar reorder = REORDER (handled locally, not here)
 */

import { TRIGGERS } from 'svelte-dnd-action';
import type { DndEvent } from 'svelte-dnd-action';
import { spaceStore, type Tab } from '$lib/stores/space.svelte';
import { reorderViewItems } from '$lib/api/client';

// ============================================================================
// Zone Types
// ============================================================================

export type ZoneType = 'tab-bar' | 'split-overlay';

export interface ZoneId {
	type: ZoneType;
	paneId?: 'left' | 'right';
}

// ============================================================================
// DnD Item (for tabs)
// ============================================================================

export interface DndTabItem {
	id: string;
	url: string;
	label: string;
	icon?: string;
	source: ZoneId;
	tab: Tab;
}

// ============================================================================
// Operation Types
// ============================================================================

export type DropOperation = 'move' | 'reorder' | 'none';

export interface DragSession {
	item: DndTabItem;
	sourceZone: ZoneId;
	startedAt: number;
}

// ============================================================================
// Semantic Rules (Simplified)
// ============================================================================

function isSameZone(a: ZoneId, b: ZoneId): boolean {
	if (a.type !== b.type) return false;
	return a.paneId === b.paneId;
}

function determineOperation(sourceZone: ZoneId, targetZone: ZoneId): DropOperation {
	// Same zone = REORDER
	if (isSameZone(sourceZone, targetZone)) {
		return 'reorder';
	}

	// Tab → Split Overlay = MOVE
	if (sourceZone.type === 'tab-bar' && targetZone.type === 'split-overlay') {
		return 'move';
	}

	// Cross-pane tab move
	if (
		sourceZone.type === 'tab-bar' &&
		targetZone.type === 'tab-bar' &&
		sourceZone.paneId !== targetZone.paneId
	) {
		return 'move';
	}

	return 'none';
}

// ============================================================================
// Manager Class
// ============================================================================

class DndManager {
	// Session tracking - public for reactive access (needed for split overlay visibility)
	session = $state<DragSession | null>(null);

	get isDragging(): boolean {
		return this.session !== null;
	}

	// ============================================================================
	// Event Handlers
	// ============================================================================

	/**
	 * Handle svelte-dnd-action's `consider` event.
	 */
	handleConsider<T extends DndTabItem>(
		e: CustomEvent<DndEvent<T>>,
		zoneId: ZoneId,
		setItems: (items: T[]) => void,
		originalItems?: T[]
	): void {
		const { items, info } = e.detail;

		// Update items (optimistic)
		setItems(items as T[]);

		// Start session on drag start
		if (info.trigger === TRIGGERS.DRAG_STARTED) {
			const draggedItem = originalItems?.find((i) => i.id === info.id);
			if (draggedItem) {
				this.session = {
					item: draggedItem as DndTabItem,
					sourceZone: zoneId,
					startedAt: Date.now()
				};
			}
		}
	}

	/**
	 * Handle svelte-dnd-action's `finalize` event.
	 */
	async handleFinalize<T extends DndTabItem>(
		e: CustomEvent<DndEvent<T>>,
		zoneId: ZoneId,
		setItems: (items: T[]) => void
	): Promise<void> {
		const { items, info } = e.detail;
		const currentSession = this.session;

		// Always end session
		this.session = null;

		if (!currentSession) {
			return;
		}

		// Only handle meaningful drops
		if (
			info.trigger !== TRIGGERS.DROPPED_INTO_ZONE &&
			info.trigger !== TRIGGERS.DROPPED_INTO_ANOTHER
		) {
			return;
		}

		const operation = determineOperation(currentSession.sourceZone, zoneId);

		switch (operation) {
			case 'move':
				this.executeMove(currentSession.item, currentSession.sourceZone, zoneId);
				break;

			case 'reorder':
				setItems(items as T[]);
				this.executeReorder(items as DndTabItem[], zoneId);
				break;
		}
	}

	// ============================================================================
	// Operation Executors
	// ============================================================================

	private executeMove(item: DndTabItem, source: ZoneId, target: ZoneId): void {
		if (!item.tab?.id) return;

		// Tab → Split Overlay
		if (source.type === 'tab-bar' && target.type === 'split-overlay') {
			if (!spaceStore.isSplit) {
				spaceStore.enableSplit();
			}
			spaceStore.moveTabToPane(item.tab.id, target.paneId as 'left' | 'right');
			return;
		}

		// Cross-pane tab move
		if (source.type === 'tab-bar' && target.type === 'tab-bar') {
			spaceStore.moveTabToPane(item.tab.id, target.paneId as 'left' | 'right');
		}
	}

	private executeReorder(items: DndTabItem[], zone: ZoneId): void {
		if (zone.type === 'tab-bar') {
			const tabIds = items.filter((i) => i.tab).map((i) => i.tab.id);
			spaceStore.setTabOrder(tabIds, zone.paneId);
		}
	}
}

// ============================================================================
// Sidebar DnD Helpers (used by sidebar components with SortableJS)
// ============================================================================

export interface SidebarDndItem {
	id: string;
	url: string;
	label: string;
	icon?: string;
}

/**
 * Persist sidebar reorder to backend.
 * Called directly by sidebar components after dnd finalize.
 */
export async function persistSidebarReorder(
	items: SidebarDndItem[],
	workspaceId: string
): Promise<void> {
	const urls = items.map((i) => i.url);
	await spaceStore.reorderSpaceItems(urls, workspaceId);
}

/**
 * Persist folder reorder to backend.
 */
export async function persistFolderReorder(
	items: SidebarDndItem[],
	folderId: string
): Promise<void> {
	const urls = items.map((i) => i.url);
	await reorderViewItems(folderId, urls);
	spaceStore.invalidateViewCache();
}

// ============================================================================
// Export singleton
// ============================================================================

export const dndManager = new DndManager();

// Debug access
if (typeof window !== 'undefined') {
	(window as unknown as { dndManager: DndManager }).dndManager = dndManager;
}
