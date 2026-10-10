/**
 * Shared sidebar state — reactive store consumed by both
 * UnifiedSidebar (owner) and the rail's mark.
 */

const STORAGE_KEY = "virtues-sidebar-collapsed";
const WIDTH_KEY = "virtues-sidebar-width";
const COMPACT_KEY = "virtues-rail-compact";

/**
 * The PANEL's width — the rail is fixed, so this is the only part that resizes.
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
 * The rail's width with and without its labels. The aside sizes itself as
 * rail + gap + panel, so both read these — a rail narrower than the aside
 * thinks it is opens a strip of desk between the panel and the pane.
 */
export const RAIL_WIDTH = 72;
export const RAIL_COMPACT_WIDTH = 52;

function clampWidth(v: number): number {
	if (!Number.isFinite(v)) return SIDEBAR_DEFAULT_WIDTH;
	return Math.max(SIDEBAR_MIN_WIDTH, Math.min(SIDEBAR_MAX_WIDTH, Math.round(v)));
}

let collapsed = $state(false);
let width = $state(SIDEBAR_DEFAULT_WIDTH);
// The rail without its labels: glyphs only, narrower. Independent of
// `collapsed`, which is the panel beside it.
let railCompact = $state(false);

// Initialize from localStorage (safe for SSR — guarded)
if (typeof localStorage !== "undefined") {
	const stored = localStorage.getItem(STORAGE_KEY);
	if (stored !== null) {
		collapsed = stored === "true";
	}
	railCompact = localStorage.getItem(COMPACT_KEY) === "true";
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

	/** Rail shows glyphs only — toggled by right-clicking the ∴ mark. */
	get railCompact() {
		return railCompact;
	},
	get railWidth() {
		return railCompact ? RAIL_COMPACT_WIDTH : RAIL_WIDTH;
	},
	toggleRailCompact() {
		railCompact = !railCompact;
		if (typeof localStorage !== "undefined") {
			localStorage.setItem(COMPACT_KEY, String(railCompact));
		}
	},
};
