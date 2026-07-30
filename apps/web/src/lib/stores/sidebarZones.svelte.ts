/**
 * Which sidebar zones are folded shut.
 *
 * Persisted, unlike the sub-navigation mode: a mode is somewhere you go and
 * come back from, but a folded zone is a statement about how you want the rail
 * to look — reopening the app and finding Library expanded again would undo a
 * decision the user made on purpose.
 *
 * Keyed by zone id ('desk', 'grp_workbench', 'grp_library') so a future zone
 * costs one string.
 */

const STORAGE_KEY = 'virtues-sidebar-zones-collapsed';

let collapsed = $state<Record<string, boolean>>({});

if (typeof localStorage !== 'undefined') {
	const stored = localStorage.getItem(STORAGE_KEY);
	if (stored) {
		try {
			collapsed = JSON.parse(stored) as Record<string, boolean>;
		} catch {
			// A corrupt value is not worth a broken sidebar; start fresh.
			collapsed = {};
		}
	}
}

function persist() {
	if (typeof localStorage !== 'undefined') {
		localStorage.setItem(STORAGE_KEY, JSON.stringify(collapsed));
	}
}

export const sidebarZones = {
	isCollapsed(id: string): boolean {
		return collapsed[id] === true;
	},
	toggle(id: string): void {
		collapsed = { ...collapsed, [id]: !collapsed[id] };
		persist();
	},
};
