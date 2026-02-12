/**
 * Shared sidebar state — reactive store consumed by both
 * UnifiedSidebar (owner) and WindowTabBar (icon toggle).
 */

const STORAGE_KEY = "virtues-sidebar-collapsed";

let collapsed = $state(false);

// Initialize from localStorage (safe for SSR — guarded)
if (typeof localStorage !== "undefined") {
	const stored = localStorage.getItem(STORAGE_KEY);
	if (stored !== null) {
		collapsed = stored === "true";
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
};
