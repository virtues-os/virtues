/**
 * How much an open thinking block shows: one sentence of what it used, one
 * line per step, or every step with its full text. Kept in this browser,
 * like the page display settings. Steps is the default.
 */

export type ThinkingDetail = 'brief' | 'steps' | 'all';

const STORAGE_KEY = 'virtues-thinking-detail';
const DETAILS: ThinkingDetail[] = ['brief', 'steps', 'all'];

function load(): ThinkingDetail {
	try {
		const saved = localStorage.getItem(STORAGE_KEY);
		if (DETAILS.includes(saved as ThinkingDetail)) return saved as ThinkingDetail;
	} catch {
		// No storage (private window, SSR): steps.
	}
	return 'steps';
}

class ThinkingDetailPref {
	value = $state<ThinkingDetail>(load());

	set(detail: ThinkingDetail) {
		this.value = detail;
		try {
			localStorage.setItem(STORAGE_KEY, detail);
		} catch {
			// The choice still holds until the app reloads.
		}
	}
}

export const thinkingDetail = new ThinkingDetailPref();
