/**
 * Whether the search/ask palette is up.
 *
 * This is a store rather than local state in `UnifiedSidebar` because the
 * palette had exactly one door: the sidebar's search row, and the ⌘K
 * registered beside it. The sidebar does not render on the phone shell and a
 * phone has no ⌘, so the app's only way to *find* anything — a page, a chat, a
 * person — did not exist there at all. You could browse and nothing else.
 *
 * The modal now mounts once at the app layout, above both shells, and anything
 * that can reach a store can open it.
 */

let open = $state(false);

export const search = {
	get open(): boolean {
		return open;
	},
	show(): void {
		open = true;
	},
	hide(): void {
		open = false;
	},
	/**
	 * For the keyboard chord only. The summon-from-outside path deliberately
	 * calls `show()` instead: that chord arrives from another app, where you
	 * cannot see whether the palette is already up, and a toggle would close it
	 * half the time for no reason the user could have predicted.
	 */
	toggle(): void {
		open = !open;
	}
};
