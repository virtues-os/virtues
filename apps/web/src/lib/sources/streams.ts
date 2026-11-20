/**
 * Sources · Streams, in words: the raw records your server keeps, grouped by
 * the stream that sent them (`/api/lake/streams`, `virtues-core/src/api/lake.rs`).
 */
import { formatBytes } from '$lib/settings/serverStatus';

export interface LakeStream {
	/** The provider that sent it (`ios`, `mac`, `google`), which is also its source's id. */
	source_id: string;
	source_name: string;
	/** `raw_stream` for records, `media` for the files they point to. */
	source_type: string;
	stream_name: string;
	size_bytes: number;
	record_count: number;
	object_count: number;
	earliest_at: string | null;
	latest_at: string | null;
}

const DAY_MS = 86_400_000;

/** A stream's id as words: `heart_rate` is "Heart rate". */
export function streamName(id: string): string {
	const words = id.replace(/[_-]+/g, ' ').trim();
	return words ? words.charAt(0).toUpperCase() + words.slice(1) : id;
}

const monthYear = (d: Date) => d.toLocaleDateString(undefined, { month: 'long', year: 'numeric' });

/**
 * The dates a stream covers: "March 2025 to now", or "March 2025 to
 * October 30" once it has been quiet for more than two days. A stopped
 * stream reads as its last date, with no warning.
 */
export function covers(earliest: string | null, latest: string | null, now = Date.now()): string {
	if (!earliest) return '';
	const from = monthYear(new Date(earliest));
	if (!latest || now - new Date(latest).getTime() <= 2 * DAY_MS) return `${from} to now`;
	const last = new Date(latest);
	const sameYear = last.getFullYear() === new Date(now).getFullYear();
	const to = last.toLocaleDateString(undefined, sameYear ? { month: 'long', day: 'numeric' } : { month: 'long', day: 'numeric', year: 'numeric' });
	return `${from} to ${to}`;
}

/** The page's opening sentence. */
export function streamsLead(streams: LakeStream[]): string {
	if (streams.length === 0) return 'Your server hasn’t received anything yet.';
	const total = streams.reduce((sum, s) => sum + (s.size_bytes || 0), 0);
	const starts = streams.map((s) => s.earliest_at).filter((d): d is string => !!d).sort();
	const n = streams.length;
	const since = starts.length ? `, going back to ${monthYear(new Date(starts[0]))}` : '';
	return `Your server keeps ${n} ${n === 1 ? 'stream' : 'streams'}, ${formatBytes(total)} in all${since}.`;
}
