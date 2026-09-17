/**
 * Which room the sidebar panel is showing.
 *
 * PERSISTED, and that is a deliberate reversal of `sidebarMode`, which was
 * session-only because "a mode is somewhere you go and come back from". That
 * held while a mode was an exception. It does not hold for a rail: there is
 * always a selected room, so "come back from" has no destination, and a rail
 * that reset every launch would throw away the one piece of state the user set
 * by hand.
 *
 * Deliberately not derived from which pane has focus. Deriving it is incoherent
 * with split panes — Settings left and a chat right leaves no correct answer —
 * the same conclusion `modes.ts` reached. The rail shows TWO things instead:
 * the selected room (this store, a fill) and the occupied rooms (derived from
 * the panes, full contrast, no fill). Usually the same room. When they are not,
 * that is information rather than a contradiction to hide.
 */

import { DEFAULT_ROOM_ID, roomById, type Room } from '$lib/sidebar/rooms';

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
}

export const sidebarRoom = new SidebarRoomStore();
