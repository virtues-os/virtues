/**
 * Shapes the native collector plugins resolve, shared by the This-device
 * screen and its stream pages. One place, so a page and the list cannot
 * disagree about what a status looks like.
 */
import type { MutedPlace } from "$lib/mobile/audioPlaces";

export interface OutboxStats {
	queued: number;
	failing: number;
	/** unix seconds, 0 if empty */
	oldest: number;
}

/** Health, Calendar, Contacts, Finance all resolve this. */
export interface StreamStatus {
	authorized: boolean;
	collecting: boolean;
}

/** One default, and windows that invert it. Minutes since local midnight;
 * start > end wraps past midnight. See agents/plan/audio-schedule-places-plan.md. */
export interface MuteSchedule {
	v?: number;
	default_muted: boolean;
	days: Record<string, [number, number][]>;
}

export interface AudioStatus {
	authorized: boolean;
	recording: boolean;
	notify: boolean;
	/** Chunks shipped metadata-only because they measured silent. */
	silentDropped?: number;
	/** Quiet-hours window, minutes since local midnight; -1 or absent = off.
	 * The legacy pair — a native build with `schedule` mirrors it. */
	quietStart?: number;
	quietEnd?: number;
	/** Paused for a reason the user did not choose: "carplay" while a car
	 * audio route is present. Recording is still ON — the control offers
	 * Stop, not Resume, and a Resume would just re-evict the car. */
	pausedReason?: string;
	/** The weekly mute schedule. Absent on a native build that predates it. */
	schedule?: MuteSchedule;
	/** The plugin's cache of the box's muted places. Same absence rule. */
	places?: MutedPlace[];
	/** Why chunk writing is paused right now: "schedule" or "place". */
	mutedBy?: string;
}

export type StreamKey = "location" | "health" | "calendar" | "contacts" | "finance" | "audio";

/** Minutes since midnight → a short local time, "10:00 PM". */
export function fmtClock(m: number): string {
	const d = new Date(2000, 0, 1, Math.floor(m / 60) % 24, m % 60);
	return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

export function minToTime(m: number): string {
	const h = Math.floor(m / 60) % 24;
	return `${String(h).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
}

export function timeToMin(t: string): number {
	const [h, m] = t.split(":").map(Number);
	return (h || 0) * 60 + (m || 0);
}

export const DAYS: [string, string][] = [
	["mon", "Monday"],
	["tue", "Tuesday"],
	["wed", "Wednesday"],
	["thu", "Thursday"],
	["fri", "Friday"],
	["sat", "Saturday"],
	["sun", "Sunday"],
];

/** The one window shared by all seven days, if that is what the schedule is. */
export function uniformWindow(s: MuteSchedule | null | undefined): [number, number] | null {
	if (!s) return null;
	const lists = DAYS.map(([k]) => s.days[k] ?? []);
	const first = lists[0];
	if (first.length !== 1) return null;
	const same = lists.every(
		(l) => l.length === 1 && l[0][0] === first[0][0] && l[0][1] === first[0][1],
	);
	return same ? first[0] : null;
}

export function scheduleHasWindows(s: MuteSchedule | null | undefined): boolean {
	return !!s && Object.values(s.days).some((w) => w.length > 0);
}

/** The schedule in one phrase, for a value row: "Always", "Except 10:00 PM – 7:00 AM", … */
export function describeSchedule(s: MuteSchedule | null | undefined): string {
	if (!s) return "Always";
	const windows = scheduleHasWindows(s);
	if (!windows) return s.default_muted ? "Never" : "Always";
	const u = uniformWindow(s);
	if (u) {
		const span = `${fmtClock(u[0])} – ${fmtClock(u[1])}`;
		return s.default_muted ? `${span} daily` : `Except ${span}`;
	}
	return "Varies by day";
}
