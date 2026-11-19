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
let startQuery = $state("");
/** Where the keyboard was when search opened, so closing can put it back. */
let returnFocus: HTMLElement | null = null;
/** The last close was a hand-off to a page (`$lib/search/handoff`): the card sinks instead of blurring away. */
let sink = false;

export const search = {
	get open(): boolean {
		return open;
	},
	/** What the field holds when it opens: empty, or a scope such as `#pages `. */
	get startQuery(): string {
		return startQuery;
	},
	show(query = ""): void {
		if (!open) returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
		startQuery = query;
		open = true;
	},
	/** Whether the close in progress hands off to a page. */
	get sinking(): boolean {
		return sink;
	},
	/**
	 * Close, and put the keyboard back where it was, unless a result took it
	 * somewhere else. Checked after the flush that opens a chosen result, so
	 * that result can take focus first. `sink` is the hand-off's close: the
	 * card drops away as the page it opened comes up.
	 */
	hide(opts: { sink?: boolean } = {}): void {
		if (!open) return;
		sink = !!opts.sink;
		open = false;
		const back = returnFocus;
		returnFocus = null;
		setTimeout(() => {
			const active = document.activeElement;
			// Search's own field still holds it while it fades away.
			const lost = !active || active === document.body || !!active.closest('[data-search]');
			if (lost && back?.isConnected && back.checkVisibility()) back.focus({ preventScroll: true });
		}, 0);
	},
	/**
	 * For the keyboard chord only. The summon-from-outside path deliberately
	 * calls `show()` instead: that chord arrives from another app, where you
	 * cannot see whether the palette is already up, and a toggle would close it
	 * half the time for no reason the user could have predicted.
	 */
	toggle(): void {
		if (open) search.hide();
		else search.show();
	}
};
