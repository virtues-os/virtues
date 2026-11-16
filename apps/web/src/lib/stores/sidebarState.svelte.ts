/**
 * Shared sidebar state — reactive store consumed by UnifiedSidebar (owner)
 * and the window header, which toggles it and lines its tabs up with it.
 */

const STORAGE_KEY = "virtues-sidebar-collapsed";
const WIDTH_KEY = "virtues-sidebar-width";

/**
 * The sidebar's width.
 *
 * The floor is the narrowest width at which the fixed rows still fit on one
 * line: Setup with its count, and Settings. Chat
 * titles are cut short well before it, so a narrower sidebar would only be a
 * sliver. Dragging on past the floor closes the sidebar on release instead,
 * which is what desktop chat apps have taught people to expect.
 */
export const SIDEBAR_MIN_WIDTH = 140;
export const SIDEBAR_MAX_WIDTH = 420;
// Wide enough that a typical chat title fits without its ellipsis.
export const SIDEBAR_DEFAULT_WIDTH = 236;
/** The old default. A stored width equal to it was never chosen, so it moves to the new one. */
const OLD_DEFAULT_WIDTH = 208;
/**
 * Released with the pointer asking for less than this, the sidebar closes.
 * It sits 32px past the floor, so the panel visibly stops before the drag
 * means "close".
 */
export const SIDEBAR_COLLAPSE_AT = SIDEBAR_MIN_WIDTH - 32;

/**
 * The desk gap between the window's left edge and the sidebar. The pane has
 * none: it runs to the window's right and bottom edges and meets the sidebar
 * and the other pane edge to edge.
 */
export const DESK_GAP_PX = 8;
export const SIDEBAR_INSET = DESK_GAP_PX;

/**
 * The sidebar's width when closed to its glyphs: the desk gap, the panel
 * body's 4px, a 40px row (12px, the glyph, 12px), and the desk gap again
 * on the pane side. The glyphs sit where they sit in the open sidebar.
 */
export const RAIL_WIDTH = SIDEBAR_INSET + 4 + 40 + DESK_GAP_PX;

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
		const n = Number(storedWidth);
		width = clampWidth(n === OLD_DEFAULT_WIDTH ? SIDEBAR_DEFAULT_WIDTH : n);
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
	 * Where the pane starts, in px from the window's left edge: the rail's
	 * edge with the sidebar closed, and the sidebar's edge with it open. The
	 * pane meets both with no gap. The window header aligns the left pane's
	 * tabs to this.
	 */
	get cardLeft() {
		return collapsed ? RAIL_WIDTH : SIDEBAR_INSET + width;
	},
};
