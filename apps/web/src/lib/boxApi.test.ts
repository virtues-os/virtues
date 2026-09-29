import { describe, expect, it } from 'vitest';
import { boxApiVersion, boxBelowFloor, MIN_BOX_API } from './boxApi';

const answering = (body: unknown, ok = true) =>
	(async () => ({ ok, json: async () => body })) as unknown as typeof fetch;

describe('boxApiVersion', () => {
	it('reads the number the box reports', async () => {
		expect(await boxApiVersion(answering({ api_version: 3 }), true)).toBe(3);
	});

	it('a box that predates the field is 0, not unknown', async () => {
		// Every box released before 2026-09-29: it answers, just without the
		// number. Reading that as "unknown" would let a too-old box through.
		expect(await boxApiVersion(answering({ version: '0.1.9' }), true)).toBe(0);
	});

	it('an unreachable box is unknown, not old', async () => {
		const failing = (async () => {
			throw new Error('offline');
		}) as unknown as typeof fetch;
		expect(await boxApiVersion(failing, true)).toBeNull();
		expect(await boxApiVersion(answering({}, false), true)).toBeNull();
	});
});

describe('boxBelowFloor', () => {
	it('never blocks while the floor is 0', async () => {
		// Shipping the floor at 0 is what keeps every existing box working on
		// the day this lands. If this fails, MIN_BOX_API was raised: make sure
		// that was deliberate and has a release note.
		expect(MIN_BOX_API).toBe(0);
		expect(await boxBelowFloor(answering({ version: 'ancient' }))).toBe(false);
	});
});
