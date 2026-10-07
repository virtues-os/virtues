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

/** Whether the Health sheet has been shown — the most HealthKit will say.
 * `requested` says nothing about what the person allowed. */
export type HealthPermission = "requested" | "not_requested" | "unknown" | "unavailable";

/** Health's status. `authorized` there is the opt-in, not a grant: HealthKit
 * never reports whether reads were allowed. */
export interface HealthStatus extends StreamStatus {
	/** Absent on a native build that predates it. */
	permission?: HealthPermission;
}

/** The microphone permission, with its two "no"s told apart. */
export type MicPermission = "granted" | "denied" | "not_determined" | "unavailable";

/** One default, and windows that invert it. Minutes since local midnight;
 * start > end wraps past midnight. See agents/plan/audio-schedule-places-plan.md. */
export interface MuteSchedule {
	v?: number;
	default_muted: boolean;
	days: Record<string, [number, number][]>;
}

export interface AudioStatus {
	/** Microphone permission granted. */
	authorized: boolean;
	/** `authorized` as a word, so `denied` (needs Settings) is not confused
	 * with `not_determined` (the sheet will show). Absent on older native builds. */
	mic?: MicPermission;
	/** The person left recording on; true through a call or a CarPlay pause.
	 * Absent on older native builds. */
	enabled?: boolean;
	/** The recorder is capturing right now. */
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
	/** Why chunk writing is paused right now: "pause", "schedule" or "place". */
	mutedBy?: string;
	/** The override set from Control Center: "silence" (paused) or "record"
	 * (recording through your hours or a place). The mic stays on either way,
	 * so ending one needs no restart. Absent when there is none. */
	override?: "silence" | "record";
	/** When the override ends, epoch ms. Absent = until you end it. */
	overrideUntil?: number;
}

export type StreamKey = "location" | "health" | "calendar" | "contacts" | "finance" | "audio";

/** Minutes since midnight → a short local time, "10:00 PM". */
export function fmtClock(m: number): string {
	const d = new Date(2000, 0, 1, Math.floor(m / 60) % 24, m % 60);
	return d.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

function clockAt(d: Date): string {
	return fmtClock(d.getHours() * 60 + d.getMinutes());
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

export function scheduleHasWindows(s: MuteSchedule | null | undefined): boolean {
	return !!s && Object.values(s.days).some((w) => w.length > 0);
}

/** One window and the days it starts on, indexed like `DAYS`. A window whose
 * end is before its start runs past midnight into the next day. */
export interface HoursWindow {
	start: number;
	end: number;
	days: boolean[];
}

/** The schedule's per-day lists, folded into windows: days that share the
 * same start and end are one window with several days ticked. */
export function scheduleWindows(s: MuteSchedule | null | undefined): HoursWindow[] {
	if (!s) return [];
	const out: HoursWindow[] = [];
	DAYS.forEach(([key], di) => {
		for (const [start, end] of s.days[key] ?? []) {
			let w = out.find((x) => x.start === start && x.end === end);
			if (!w) {
				w = { start, end, days: DAYS.map(() => false) };
				out.push(w);
			}
			w.days[di] = true;
		}
	});
	return out;
}

/** Windows back into the per-day lists the plugin stores. */
export function windowsToDays(ws: HoursWindow[]): Record<string, [number, number][]> {
	const days: Record<string, [number, number][]> = {};
	DAYS.forEach(([key], di) => {
		days[key] = ws.filter((w) => w.days[di] && w.start !== w.end).map((w) => [w.start, w.end]);
	});
	return days;
}

/** `Date.getDay()` (0 = Sunday) → the schedule's day key. */
const JS_DAY = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

/** Whether the schedule keeps nothing at `at`. Mirrors the plugin's gate
 * (Audio.swift `MuteSchedule.muted`): a window that wraps past midnight
 * covers the start of the next day too. */
export function scheduleMutedAt(s: MuteSchedule, at: Date): boolean {
	const m = at.getHours() * 60 + at.getMinutes();
	const today = JS_DAY[at.getDay()];
	const yesterday = JS_DAY[(at.getDay() + 6) % 7];
	const inWindow =
		(s.days[today] ?? []).some(([a, b]) => (a < b ? m >= a && m < b : m >= a)) ||
		(s.days[yesterday] ?? []).some(([a, b]) => a > b && m < b);
	return inWindow !== s.default_muted;
}

/** The next minute the schedule flips, within a week; null when it never does. */
export function nextScheduleChange(s: MuteSchedule, from: Date): Date | null {
	const t = new Date(from);
	t.setSeconds(0, 0);
	const now = scheduleMutedAt(s, t);
	for (let i = 0; i < 7 * 24 * 60; i++) {
		t.setMinutes(t.getMinutes() + 1);
		if (scheduleMutedAt(s, t) !== now) return t;
	}
	return null;
}

/** What the microphone is doing right now, as the page and the list show it. */
export interface AudioState {
	/** The status line: "Live", "Idle until 7:00 AM", … */
	label: string;
	/** Audio is being kept right now. */
	live: boolean;
	/** One sentence under the header when the state needs explaining. */
	explain: string | null;
}

const MIC_LIT = "The mic stays on, so your iPhone's orange microphone dot stays lit.";

export function audioState(a: AudioStatus | null, now = new Date()): AudioState | null {
	if (!a) return null;
	if (!a.authorized) {
		return a.mic === "denied"
			? {
					label: "Microphone access is off",
					live: false,
					explain: "To record, allow microphone access for Virtues in iPhone Settings.",
				}
			: null;
	}
	const until = a.overrideUntil ? clockAt(new Date(a.overrideUntil)) : null;
	if (a.override === "silence" && a.enabled) {
		return {
			label: until ? `Paused until ${until}` : "Paused",
			live: false,
			explain: `${MIC_LIT} Virtues keeps nothing until ${until ?? "you resume"}.`,
		};
	}
	if (a.override === "record" && a.enabled && a.recording) {
		return {
			label: until ? `Live until ${until}` : "Live",
			live: true,
			explain: until
				? `Recording anyway until ${until}. After that, your hours and places apply again.`
				: "Recording anyway until your hours or this place stop muting.",
		};
	}
	if (a.mutedBy === "schedule") {
		// Name an end only when this clock agrees the hours are muting now: the
		// plugin's reason is the truth, and a skewed clock or zone would make
		// "until" name the moment muting starts.
		const next =
			a.schedule && scheduleMutedAt(a.schedule, now) ? nextScheduleChange(a.schedule, now) : null;
		return {
			label: next ? `Idle until ${fmtClock(next.getHours() * 60 + next.getMinutes())}` : "Idle · not recording",
			live: false,
			explain: `${MIC_LIT} Nothing is kept until your hours allow it.`,
		};
	}
	if (a.mutedBy === "place") {
		return {
			label: "Idle · not recording here",
			live: false,
			explain: `${MIC_LIT} Nothing is kept while you're at this place.`,
		};
	}
	if (a.recording) return { label: "Live", live: true, explain: null };
	if (a.pausedReason === "carplay") {
		return {
			label: "Paused for CarPlay",
			live: false,
			explain: "Recording picks up again after CarPlay disconnects.",
		};
	}
	if (a.enabled) return { label: "Paused", live: false, explain: null };
	return {
		label: "Stopped",
		live: false,
		explain: "Recording is off. Your hours and places stay saved.",
	};
}
