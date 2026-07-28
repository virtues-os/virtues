/**
 * Which sub-navigation mode the sidebar is in, if any.
 *
 * Session state, not persisted: a mode is somewhere you go and come back from,
 * so reopening the app inside Settings would be wrong.
 */

import { SIDEBAR_MODES, type SidebarMode } from '$lib/sidebar/modes';

class SidebarModeStore {
	/** `null` is the normal sidebar. */
	activeId = $state<string | null>(null);

	get active(): SidebarMode | null {
		return this.activeId ? (SIDEBAR_MODES[this.activeId] ?? null) : null;
	}

	enter(id: string): void {
		if (SIDEBAR_MODES[id]) this.activeId = id;
	}

	exit(): void {
		this.activeId = null;
	}
}

export const sidebarMode = new SidebarModeStore();
