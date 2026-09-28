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
 * - Tab → a project row in the sidebar = FILE (the tab's chat or page joins
 *   the project; the tab stays where it was)
 */

import { TRIGGERS } from 'svelte-dnd-action';
import type { DndEvent } from 'svelte-dnd-action';
import { windowShellStore, type Tab } from '$lib/stores/window-shell.svelte';
import { projectStore } from '$lib/stores/project.svelte';
import { fileIntoProject, projectMemberUrl } from '$lib/utils/projectActions';

/**
 * A project row takes a tab by carrying this attribute with the project's id.
 * svelte-dnd-action knows only its own zones, and a sidebar row is not one:
 * a tab let go over it was a drop "outside of any" and simply went home.
 * So while a tab is in the air the pointer is hit-tested against these rows,
 * and a let-go over one files the tab's thing instead.
 */
export const PROJECT_DROP_ATTR = 'data-project-drop';

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

	/** The project row under a dragged tab, for the row to light up. */
	projectTarget = $state<string | null>(null);

	get isDragging(): boolean {
		return this.session !== null;
	}

	/** The member url the dragged tab would file, or null if it has none. */
	get draggedMemberUrl(): string | null {
		const route = this.session?.item.tab?.route;
		return route ? projectMemberUrl(route) : null;
	}

	private trackPointer = (e: MouseEvent | TouchEvent) => {
		if (!this.draggedMemberUrl) return;
		const pt = 'touches' in e ? e.touches[0] : e;
		if (!pt) return;
		let id: string | null = null;
		for (const el of document.elementsFromPoint(pt.clientX, pt.clientY)) {
			const row = el.closest(`[${PROJECT_DROP_ATTR}]`);
			if (row) {
				id = row.getAttribute(PROJECT_DROP_ATTR);
				break;
			}
		}
		if (id !== this.projectTarget) this.projectTarget = id;
	};

	private startTracking(): void {
		window.addEventListener('mousemove', this.trackPointer, { capture: true, passive: true });
		window.addEventListener('touchmove', this.trackPointer, { capture: true, passive: true });
	}

	private stopTracking(): void {
		window.removeEventListener('mousemove', this.trackPointer, { capture: true });
		window.removeEventListener('touchmove', this.trackPointer, { capture: true });
		this.projectTarget = null;
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
				this.startTracking();
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
		const url = this.draggedMemberUrl;
		const target = this.projectTarget ? projectStore.byId(this.projectTarget) : undefined;

		// Always end session
		this.session = null;
		this.stopTracking();

		if (!currentSession) {
			return;
		}

		// Let go over a project row: file the tab's thing there. The tab itself
		// goes back where it was, which is what the library does for a drop
		// outside its zones.
		if (url && target) {
			setItems(items as T[]);
			void fileIntoProject(target, url);
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
			if (!windowShellStore.isSplit) {
				windowShellStore.enableSplit();
			}
			windowShellStore.moveTabToPane(item.tab.id, target.paneId as 'left' | 'right');
			return;
		}

		// Cross-pane tab move
		if (source.type === 'tab-bar' && target.type === 'tab-bar') {
			windowShellStore.moveTabToPane(item.tab.id, target.paneId as 'left' | 'right');
		}
	}

	private executeReorder(items: DndTabItem[], zone: ZoneId): void {
		if (zone.type === 'tab-bar') {
			const tabIds = items.filter((i) => i.tab).map((i) => i.tab.id);
			windowShellStore.setTabOrder(tabIds, zone.paneId);
		}
	}
}

// ============================================================================
// Export singleton
// ============================================================================

export const dndManager = new DndManager();

// Debug access
if (typeof window !== 'undefined') {
	(window as unknown as { dndManager: DndManager }).dndManager = dndManager;
}
