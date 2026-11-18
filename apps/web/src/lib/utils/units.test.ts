// Distances and temperatures read in the units the owner chose.
import { describe, expect, it } from 'vitest';
import { formatDistance, formatTemperature } from './units';

describe('formatDistance', () => {
	it('reads metric as meters, then kilometres', () => {
		expect(formatDistance(640, 'metric')).toBe('640 m');
		expect(formatDistance(3400, 'metric')).toBe('3.4 km');
		expect(formatDistance(12_400, 'metric')).toBe('12 km');
	});

	it('reads US units as feet, then miles', () => {
		expect(formatDistance(100, 'us')).toBe('330 ft');
		expect(formatDistance(3400, 'us')).toBe('2.1 mi');
		expect(formatDistance(20_000, 'us')).toBe('12 mi');
	});
});

describe('formatTemperature', () => {
	it('rounds to whole degrees in either scale', () => {
		expect(formatTemperature(22, 'metric')).toBe('22°');
		expect(formatTemperature(22, 'us')).toBe('72°');
		expect(formatTemperature(-3.6, 'metric')).toBe('-4°');
	});
});
