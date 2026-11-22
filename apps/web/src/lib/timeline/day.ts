/**
 * day.ts - one local day of the record, fetched whole.
 *
 * `/api/timeline/days/:date` serves the day in the zone it woke up in (local
 * midnight to local midnight): its visits, fixes, nights, audio sessions,
 * step bins and calendar, all read from the tables every other room uses.
 * The Inbox's Map view draws it as today's route.
 */
import { apiGet } from "$lib/api/client";
import type { TimelineDayPoint } from "$lib/wiki/api";

/** A place's title (`api::place_review`): the name the person gave, Home or
 *  Work from where the time went, the map's clear match, else its city. */
export interface PlaceTitle {
	title: string;
	source: "named" | "home" | "work" | "map" | "city" | "none";
}

/** One local day, as `/api/timeline/days/:date` serves it. */
export interface TimelineDay {
	date: string;
	zone: string;
	started_at: string;
	ended_at: string;
	/** Visits overlapping the day, unclipped. */
	stays: {
		id: string;
		started_at: string;
		ended_at: string;
		latitude: number;
		longitude: number;
		place_id: string | null;
		place_name: string | null;
		place_is_named: boolean;
	}[];
	points: TimelineDayPoint[];
	/** Each stay's place by id: its title and where it came from. */
	place_titles: Record<string, PlaceTitle>;
	last_point_before: TimelineDayPoint | null;
	/** Nights overlapping the day, each joined from all of its records. */
	nights: { started_at: string; ended_at: string; asleep_minutes: number }[];
	/** Nights Apple Health didn't record: the person's usual night, to be cut
	 *  where the day's record shows them awake or elsewhere. */
	estimated_nights?: { started_at: string; ended_at: string; from_nights: number }[];
	/** speaker_mode: 0 ambient, 1 one voice, 2 a conversation, 3 a group.
	 *  title: the transcriber's heading for its longest stretch of speech. */
	sessions: {
		id: string;
		started_at: string;
		ended_at: string;
		speaker_mode: number;
		title: string | null;
		content: string | null;
	}[];
	/** Ten-minute bins, stamped at their middle. */
	steps: { at: string; steps: number }[];
	step_scale: number;
	calendar: {
		id: string;
		title: string;
		started_at: string;
		ended_at: string;
		is_all_day: boolean;
		calendar_name: string | null;
		location_name: string | null;
		status: string | null;
		response_status: string | null;
	}[];
}

export async function fetchDay(slug: string): Promise<TimelineDay> {
	return apiGet<TimelineDay>(`/timeline/days/${slug}`);
}

/** The slug `n` days from `slug` (negative steps back). */
export function stepDay(slug: string, n: number): string {
	const [y, m, d] = slug.split("-").map(Number);
	const t = new Date(y, m - 1, d + n);
	return `${t.getFullYear()}-${String(t.getMonth() + 1).padStart(2, "0")}-${String(t.getDate()).padStart(2, "0")}`;
}
