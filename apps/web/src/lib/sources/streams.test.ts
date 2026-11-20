// Streams in words. Every stream, size and date here is invented.
import { describe, expect, it } from 'vitest';
import { covers, streamName, streamsLead, type LakeStream } from './streams';

const NOW = new Date('2026-11-19T12:00:00').getTime();
const at = (local: string) => new Date(local).toISOString();

const stream = (over: Partial<LakeStream> = {}): LakeStream => ({
	source_id: 'ios',
	source_name: 'ios',
	source_type: 'raw_stream',
	stream_name: 'heart_rate',
	size_bytes: 2 * 1024 ** 3,
	record_count: 1000,
	object_count: 10,
	earliest_at: at('2025-03-04T08:00:00'),
	latest_at: at('2026-11-19T11:00:00'),
	...over,
});

describe('streamName', () => {
	it('turns an id into words', () => {
		expect(streamName('heart_rate')).toBe('Heart rate');
		expect(streamName('microphone')).toBe('Microphone');
	});
});

describe('covers', () => {
	it('says to now while records still arrive', () => {
		expect(covers(at('2025-03-04T08:00:00'), at('2026-11-18T20:00:00'), NOW)).toBe('March 2025 to now');
	});

	it('gives the last date once a stream has been quiet', () => {
		expect(covers(at('2025-06-02T08:00:00'), at('2026-10-30T20:00:00'), NOW)).toBe('June 2025 to October 30');
		expect(covers(at('2025-06-02T08:00:00'), at('2025-10-30T20:00:00'), NOW)).toBe('June 2025 to October 30, 2025');
	});

	it('says nothing without a first date', () => {
		expect(covers(null, null, NOW)).toBe('');
	});
});

describe('streamsLead', () => {
	it('counts the streams, adds up their size, and names the first month', () => {
		const lead = streamsLead([stream(), stream({ stream_name: 'location', earliest_at: at('2025-06-01T08:00:00') })]);
		expect(lead).toBe('Your server keeps 2 streams, 4.0 GB in all, going back to March 2025.');
	});

	it('says when nothing has arrived', () => {
		expect(streamsLead([])).toBe('Your server hasn’t received anything yet.');
	});
});
