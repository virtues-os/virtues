import { describe, expect, it } from "vitest";
import {
	audioState,
	nextScheduleChange,
	scheduleMutedAt,
	scheduleWindows,
	windowsToDays,
	type AudioStatus,
	type MuteSchedule,
} from "./deviceTypes";

const NIGHT: [number, number] = [22 * 60, 7 * 60];
const WORK: [number, number] = [9 * 60, 17 * 60];

function sched(days: Record<string, [number, number][]>, default_muted = false): MuteSchedule {
	return { default_muted, days };
}

/** A local Date on a known weekday: 2026-10-05 is a Monday. */
function at(day: number, h: number, m = 0): Date {
	return new Date(2026, 9, 5 + day, h, m);
}

describe("scheduleWindows / windowsToDays", () => {
	it("folds days that share a window into one, and unfolds it back", () => {
		const s = sched({
			mon: [NIGHT, WORK],
			tue: [NIGHT],
			wed: [],
			thu: [NIGHT],
			fri: [NIGHT],
			sat: [],
			sun: [],
		});
		const ws = scheduleWindows(s);
		expect(ws).toHaveLength(2);
		expect(ws[0]).toEqual({ start: NIGHT[0], end: NIGHT[1], days: [true, true, false, true, true, false, false] });
		expect(ws[1].days).toEqual([true, false, false, false, false, false, false]);
		expect(windowsToDays(ws)).toEqual(s.days);
	});
});

describe("scheduleMutedAt", () => {
	it("an overnight window started on Friday covers early Saturday, not early Friday", () => {
		const s = sched({ fri: [NIGHT] });
		expect(scheduleMutedAt(s, at(4, 23))).toBe(true); // Fri 23:00
		expect(scheduleMutedAt(s, at(5, 6, 59))).toBe(true); // Sat 06:59
		expect(scheduleMutedAt(s, at(5, 7))).toBe(false); // Sat 07:00
		expect(scheduleMutedAt(s, at(4, 3))).toBe(false); // Fri 03:00
	});

	it("only-record-during inverts the windows", () => {
		const s = sched({ mon: [WORK] }, true);
		expect(scheduleMutedAt(s, at(0, 10))).toBe(false);
		expect(scheduleMutedAt(s, at(0, 18))).toBe(true);
	});
});

describe("nextScheduleChange", () => {
	it("finds the end of the current window", () => {
		const s = sched({ mon: [NIGHT] });
		const next = nextScheduleChange(s, at(0, 23, 30));
		expect(next?.getDay()).toBe(2); // Tuesday
		expect([next?.getHours(), next?.getMinutes()]).toEqual([7, 0]);
	});

	it("is null for a schedule that never flips", () => {
		expect(nextScheduleChange(sched({}), at(0, 12))).toBeNull();
	});
});

describe("audioState", () => {
	const base: AudioStatus = { authorized: true, enabled: true, recording: true, notify: true };

	it("is live while recording and nothing mutes it", () => {
		expect(audioState(base)).toMatchObject({ label: "Live", live: true });
	});

	it("says when the hours let recording pick up again", () => {
		const s = audioState(
			{ ...base, mutedBy: "schedule", schedule: sched({ mon: [NIGHT] }) },
			at(0, 23),
		);
		expect(s?.live).toBe(false);
		expect(s?.label).toMatch(/^Idle until 7:00/);
	});

	it("names no end when this clock disagrees with the plugin's reason", () => {
		const s = audioState(
			{ ...base, mutedBy: "schedule", schedule: sched({ mon: [NIGHT] }) },
			at(0, 16),
		);
		expect(s?.label).toBe("Idle · not recording");
	});

	it("puts a pause ahead of the hours, and Stop ahead of a pause", () => {
		const paused: AudioStatus = {
			...base,
			override: "silence",
			mutedBy: "pause",
			schedule: sched({ mon: [NIGHT] }),
		};
		expect(audioState(paused, at(0, 23))).toMatchObject({ label: "Paused", live: false });
		expect(audioState({ ...paused, enabled: false, recording: false })?.label).toBe("Stopped");
	});

	it("names when a pause or a record-anyway ends", () => {
		const end = at(0, 15, 40).getTime();
		expect(audioState({ ...base, override: "silence", overrideUntil: end })?.label).toMatch(
			/^Paused until 3:40/,
		);
		const rec = audioState({ ...base, override: "record", overrideUntil: end });
		expect(rec).toMatchObject({ live: true });
		expect(rec?.label).toMatch(/^Live until 3:40/);
	});

	it("tells a stopped mic from a denied one", () => {
		expect(audioState({ ...base, enabled: false, recording: false })?.label).toBe("Stopped");
		expect(audioState({ ...base, authorized: false, mic: "denied", recording: false })?.label).toBe(
			"Microphone access is off",
		);
	});
});
