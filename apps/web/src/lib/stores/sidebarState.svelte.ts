/**
 * Shared sidebar state — reactive store consumed by UnifiedSidebar (owner)
 * and the window header, which toggles it and lines its tabs up with it.
 */

const STORAGE_KEY = "virtues-sidebar-collapsed";
const WIDTH_KEY = "virtues-sidebar-width";

/**
 * The sidebar's width.
 *
 * The floor is a real floor: below it the serif room title starts truncating
 * and chat rows become unreadable stubs, so dragging further is never what
 * someone wants. `COLLAPSE_AT` is what they DO want — past it the sidebar
 * closes rather than becoming a useless sliver, which is the behaviour every
 * desktop chat app has trained people to expect.
 */
export const SIDEBAR_MIN_WIDTH = 180;
export const SIDEBAR_MAX_WIDTH = 420;
export const SIDEBAR_DEFAULT_WIDTH = 208;
export const SIDEBAR_COLLAPSE_AT = 148;

/**
 * The desk: the one gap between the window's edge and a card, and between
 * any two cards (the sidebar and a pane, or two panes). `--pane-inset` in
 * themes.css and the cards' `ml-2` / `mx-2` / `mb-2` are the same 8px.
 */
export const DESK_GAP_PX = 8;
export const SIDEBAR_INSET = DESK_GAP_PX;

function clampWidth(v: number): number {
	if (!Number.isFinite(v)) return SIDEBAR_DEFAULT_WIDTH;
	return Math.max(SIDEBAR_MIN_WIDTH, Math.min(SIDEBAR_MAX_WIDTH, Math.round(v)));
}

let collapsed = $state(false);
let width = $state(SIDEBAR_DEFAULT_WIDTH);
// True while the seam is being dragged. Not persisted. The window header
// lines its tab strips up with the cards below, and it drops its width
// transition while this is on so the strips follow the pointer.
let resizing = $state(false);

// Initialize from localStorage (safe for SSR — guarded)
if (typeof localStorage !== "undefined") {
	const stored = localStorage.getItem(STORAGE_KEY);
	if (stored !== null) {
		collapsed = stored === "true";
	}
	const storedWidth = localStorage.getItem(WIDTH_KEY);
	if (storedWidth !== null) {
		// Clamp on read as well as on write: the bounds can move between
		// releases, and a stored 600 from an older build must not survive as a
		// sidebar half the window wide.
		width = clampWidth(Number(storedWidth));
	}
}

export const sidebarState = {
	get collapsed() {
		return collapsed;
	},
	set collapsed(val: boolean) {
		collapsed = val;
		if (typeof localStorage !== "undefined") {
			localStorage.setItem(STORAGE_KEY, String(val));
		}
	},
	toggle() {
		sidebarState.collapsed = !collapsed;
	},

	/** Panel width in px, always within [MIN, MAX]. */
	get width() {
		return width;
	},
	set width(val: number) {
		width = clampWidth(val);
		if (typeof localStorage !== "undefined") {
			localStorage.setItem(WIDTH_KEY, String(width));
		}
	},
	resetWidth() {
		sidebarState.width = SIDEBAR_DEFAULT_WIDTH;
	},

	get resizing() {
		return resizing;
	},
	set resizing(val: boolean) {
		resizing = val;
	},

	/**
	 * Where the pane card starts, in px from the window's left edge: past
	 * the desk's 12px, and when the sidebar is open past its card and the
	 * desk gap between the two cards. The window header aligns the left
	 * pane's tabs to this.
	 */
	get cardLeft() {
		return SIDEBAR_INSET + (collapsed ? 0 : width + SIDEBAR_INSET);
	},
};
