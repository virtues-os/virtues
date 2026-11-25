import { describe, expect, it } from 'vitest';
import { durationOf, groupByMonth, ratioOf, titleOf } from './shape';

describe('photo favorites', () => {
	it('keeps a panorama and a tall screenshot within the wall', () => {
		expect(ratioOf({ width_px: 8000, height_px: 1000 })).toBe(0.56);
		expect(ratioOf({ width_px: 1000, height_px: 3000 })).toBe(1.6);
		expect(ratioOf({ width_px: 4000, height_px: 3000 })).toBe(0.75);
		expect(ratioOf({ width_px: null, height_px: null })).toBe(1);
	});

	it('names a photo by what it is and when it was taken', () => {
		expect(titleOf({ kind: 'live_photo', occurred_at: '2031-06-14T12:00:00' })).toBe('Photo, June 14, 2031');
		expect(titleOf({ kind: 'video', occurred_at: '2031-06-14T12:00:00' })).toBe('Video, June 14, 2031');
	});

	it('shows a length on a video only', () => {
		expect(durationOf({ kind: 'video', duration_seconds: 42.4 })).toBe('0:42');
		expect(durationOf({ kind: 'live_photo', duration_seconds: 3 })).toBeNull();
	});

	it('groups by the month taken, in order', () => {
		const at = (occurred_at: string) => ({ occurred_at });
		const groups = groupByMonth([at('2031-06-14T12:00:00'), at('2031-06-02T12:00:00'), at('2031-03-02T12:00:00')]);
		expect(groups.map((g) => [g.label, g.items.length])).toEqual([
			['June 2031', 2],
			['March 2031', 1],
		]);
	});
});
