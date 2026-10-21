/**
 * Which room the sidebar is showing.
 *
 * The sidebar is Home's list. The Wiki, Settings and Setup are rooms you
 * step into from it: a door in Home (or Settings at the sidebar's foot)
 * opens the room's page and swaps the sidebar to the room's own list, whose
 * head steps back to Home. Those lists are the rooms' only navigation; their
 * pages have none of their own.
 *
 * Persisted, so the sidebar reopens where you left it. Not derived from
 * which pane has focus: with Settings on the left and a chat on the right
 * there would be no correct answer.
 */

import { DEFAULT_ROOM_ID, roomById, type Room } from '$lib/sidebar/rooms';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { sidebarState } from '$lib/stores/sidebarState.svelte';

const STORAGE_KEY = 'virtues-sidebar-room';

class SidebarRoomStore {
	selectedId = $state<string>(DEFAULT_ROOM_ID);

	constructor() {
		if (typeof localStorage === 'undefined') return;
		const stored = localStorage.getItem(STORAGE_KEY);
		// A room can be renamed or retired between releases; fall back rather
		// than leaving the rail with nothing selected.
		if (stored && roomById(stored)) this.selectedId = stored;
	}

	get selected(): Room {
		return roomById(this.selectedId) ?? roomById(DEFAULT_ROOM_ID)!;
	}

	select(id: string): void {
		if (!roomById(id)) return;
		this.selectedId = id;
		if (typeof localStorage !== 'undefined') {
			localStorage.setItem(STORAGE_KEY, id);
		}
	}

	/** Step into a room: open its page and show its list in the sidebar. */
	enter(id: string): void {
		const room = roomById(id);
		if (!room) return;
		windowShellStore.openTabFromRoute(room.href, { label: room.label, focusExisting: true });
		this.select(id);
		sidebarState.collapsed = false;
	}

	/** Back to Home's list. The page you were on stays open. */
	leave(): void {
		this.select(DEFAULT_ROOM_ID);
	}
}

export const sidebarRoom = new SidebarRoomStore();
