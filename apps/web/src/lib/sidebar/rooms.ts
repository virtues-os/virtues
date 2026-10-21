/**
 * The sidebar's rooms.
 *
 * The sidebar is one column on the desk, and it shows one room's list at a
 * time. Home is the ground: its list is the doors (New chat, Pages, Tasks,
 * Applets, Wiki), Pinned, Projects and the recents, with Settings at the
 * foot. Every one of them opens a page: the Wiki's overview carries an index
 * of its sections, and every Settings page carries Settings' section list
 * (SectionNav), so the sidebar never changes for them.
 *
 * One room is stepped into from Home and back out again
 * (`sidebarRoom.enter` and `leave`): Setup, from a door at the top of Home
 * that shows only while a step of Setup is not done (skipped counts as not
 * done). Its steps are a short checklist, and it goes once they are done.
 *
 * There was a rail of room tiles beside the sidebar. It went when the rooms
 * came down to these few, because a column of tiles for a handful of places
 * cost a column of the window and a third alignment edge in the header.
 */

/** What fills the panel under the room's title. */
export type RoomPanel =
	/** The Home panel: doors, Pinned, Projects, the recents. */
	| { kind: 'home' }
	/** Setup: the steps, and the way back into them. */
	| { kind: 'setup' };

export interface Room {
	id: string;
	label: string;
	/** The panel's title when the label is not the room's name. */
	title?: string;
	/** The room's page, opened when you step into it. */
	href: string;
	panel: RoomPanel;
}

export const ROOMS: Room[] = [
	{
		// One word for the process everywhere: the flow, the door, the panel.
		// In the app, the steps open as a tab at /setup.
		id: 'setup',
		label: 'Setup',
		href: '/setup',
		panel: { kind: 'setup' },
	},
	{
		id: 'home',
		label: 'Home',
		href: '/chat',
		panel: { kind: 'home' },
	},
];

export const DEFAULT_ROOM_ID = 'home';

export function roomById(id: string | null | undefined): Room | null {
	if (!id) return null;
	return ROOMS.find((r) => r.id === id) ?? null;
}
