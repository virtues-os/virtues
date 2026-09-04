/**
 * The bank's lines, for the frontispiece — agents/build/voice.md owns their
 * provenance; the page shows only the line. Home rotates one per day when
 * the record has no sentence of its own for yesterday.
 */
export const BANK: readonly string[] = [
	"This day, honestly seen, is material enough for virtue. Write to yourself, for yourself — no other reader was ever needed.",
	"Most of a life is lost not to anyone's malice, but to nobody writing it down — and the ordinary days, it turns out, were the beautiful ones.",
	"A life unrecorded scatters; a life recorded, and owned, endures.",
	"Anything that knows you this well must belong to you.",
	"The record of a life belongs where the life is lived.",
	"The trouble with data is not that it is collected, but that it is collected by everyone except its owner.",
	"Keeping your own record is the first virtue of the digital age.",
];

/** One line per calendar day, the same all day. */
export function lineForDay(d: Date): string {
	const start = new Date(d.getFullYear(), 0, 0).getTime();
	const day = Math.floor((d.getTime() - start) / 86_400_000);
	return BANK[day % BANK.length];
}

/**
 * The hour's painting, until the plate job draws today's from the record.
 * Morning is the town at sunrise, the day the river, the evening the road,
 * the night the desk by the window.
 */
export function plateForHour(h: number): string {
	if (h >= 5 && h < 10) return "/plates/plate-first-day.jpg";
	if (h >= 10 && h < 16) return "/plates/plate-connect.jpg";
	if (h >= 16 && h < 20) return "/plates/plate-further.jpg";
	return "/plates/plate-letter.jpg";
}
