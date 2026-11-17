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
 * Air between the window's edge and what floats on it: the ∴, the sidebar
 * and the panes all stand this far in (themes.css, --window-air).
 */
export const DESK_GAP_PX = 12;
export const SIDEBAR_INSET = DESK_GAP_PX;
/** Air between the sidebar and the pane beside it (themes.css, --pane-gap). */
export const PANE_GAP_PX = 10;

/**
 * The rail, the sidebar closed to its glyphs: a glass column 44px wide
 * (themes.css, --rail-w), so each glyph has 10px of air on either side of
 * it. It is wider than a tab is tall: the header stays low and the rail
 * does not look pinched.
 */
export const RAIL_GLASS_PX = 44;
/** A header control's height and a tab's (themes.css, --chrome-tab-h). */
export const CHROME_TAB_PX = 34;
/** The aside's width with the sidebar closed: the air, the rail, the gap. */
export const RAIL_WIDTH = SIDEBAR_INSET + RAIL_GLASS_PX + PANE_GAP_PX;

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
		// Opening and closing run on different clocks (themes.css). The
		// root says which before the state changes, so every part of the
		// shell that moves with the sidebar picks the same one.
		if (typeof document !== "undefined" && val !== collapsed) {
			document.documentElement.dataset.sidebarMotion = val ? "closing" : "opening";
		}
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
	 * Where the pane starts, in px from the window's left edge: one gap past
	 * the rail with the sidebar closed, and one gap past the sidebar with it
	 * open. The window header aligns the left pane's tabs to this.
	 */
	get cardLeft() {
		return collapsed ? RAIL_WIDTH : SIDEBAR_INSET + width + PANE_GAP_PX;
	},
};
