/**
 * "And the next chapter?" — the one line of the Timeline that looks forward
 * (McAdams' future script opens the same way). It has no row of its own:
 * chapters are a gapless partition up to now, and a hope is not a span. So
 * it rides from the Timeline to the Interview on this device, and the
 * interview hears it as part of the person's first answer, which is where it
 * lands in the record.
 */
const KEY = 'virtues-next-chapter';

export function readNextChapter(): string {
	try {
		return localStorage.getItem(KEY) ?? '';
	} catch {
		return '';
	}
}

export function writeNextChapter(text: string) {
	try {
		const t = text.trim();
		if (t) localStorage.setItem(KEY, t);
		else localStorage.removeItem(KEY);
	} catch {
		// Private window or blocked storage: the line is optional, so it simply
		// doesn't travel.
	}
}
