import type { PageLoad } from './$types';

// Attached data types
export interface AttachedMessage {
	id: string;
	timestamp: string;
	channel: string;
	direction: string;
	from_name: string | null;
	body_preview: string;
}

export interface AttachedTranscript {
	id: string;
	recorded_at: string;
	duration_seconds: number;
	transcript_preview: string;
	speaker_count: number | null;
}

export interface AttachedCalendarEvent {
	id: string;
	start_time: string;
	end_time: string;
	title: string;
	location_name: string | null;
}

export interface AttachedEmail {
	id: string;
	timestamp: string;
	direction: string;
	from_name: string | null;
	subject: string | null;
}

export interface AttachedHealthEvent {
	id: string;
	event_type: string;
	timestamp: string;
	description: string;
}

// Chunk types
export interface LocationChunk {
	type: 'location';
	start_time: string;
	end_time: string;
	duration_minutes: number;
	place_id: string | null;
	place_name: string | null;
	latitude: number;
	longitude: number;
	is_known_place: boolean;
	messages: AttachedMessage[];
	transcripts: AttachedTranscript[];
	calendar_events: AttachedCalendarEvent[];
	emails: AttachedEmail[];
	health_events: AttachedHealthEvent[];
}

export interface TransitChunk {
	type: 'transit';
	start_time: string;
	end_time: string;
	duration_minutes: number;
	distance_km: number;
	avg_speed_kmh: number;
	from_place: string | null;
	to_place: string | null;
	messages: AttachedMessage[];
	transcripts: AttachedTranscript[];
}

export interface MissingDataChunk {
	type: 'missing_data';
	start_time: string;
	end_time: string;
	duration_minutes: number;
	likely_reason: 'sleep' | 'indoors' | 'phone_off' | 'unknown';
	last_known_location: string | null;
	next_known_location: string | null;
	messages: AttachedMessage[];
	transcripts: AttachedTranscript[];
}

export type Chunk = LocationChunk | TransitChunk | MissingDataChunk;

export interface DayView {
	date: string;
	chunks: Chunk[];
	total_location_minutes: number;
	total_transit_minutes: number;
	total_missing_minutes: number;
}

export const load: PageLoad = async ({ fetch, url }) => {
	// Get date from URL params, default to Nov 10, 2025 (Monday in Rome seed data)
	const dateParam = url.searchParams.get('date') || '2025-11-10';

	try {
		const response = await fetch(`/api/timeline/day/${dateParam}`);

		if (!response.ok) {
			throw new Error(`API returned ${response.status}`);
		}

		const dayView = await response.json() as DayView;

		return {
			dayView,
			selectedDate: dateParam,
			error: null
		};
	} catch (error) {
		console.error('Failed to load timeline data:', error);
		return {
			dayView: {
				date: dateParam,
				chunks: [],
				total_location_minutes: 0,
				total_transit_minutes: 0,
				total_missing_minutes: 0
			} as DayView,
			selectedDate: dateParam,
			error: error instanceof Error ? error.message : 'Failed to load timeline data'
		};
	}
};
