/**
 * scale.ts - a zone's offset from UTC, which the wiki's day line and day
 * article use to read an instant on the day's own clock.
 */

/** A zone's offset east of UTC at an instant, in ms. */
export function zoneOffset(t: number, zone: string): number {
	const parts = new Intl.DateTimeFormat("en-US", {
		timeZone: zone,
		hourCycle: "h23",
		year: "numeric",
		month: "2-digit",
		day: "2-digit",
		hour: "2-digit",
		minute: "2-digit",
		second: "2-digit",
	}).formatToParts(new Date(t));
	const n = (type: string) => Number(parts.find((p) => p.type === type)?.value);
	const asUtc = Date.UTC(n("year"), n("month") - 1, n("day"), n("hour") % 24, n("minute"), n("second"));
	return asUtc - Math.floor(t / 1000) * 1000;
}
