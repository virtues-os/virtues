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

/** Whether a box answering `body` is below the floor. The answer is cached, so it is asked afresh first. */
async function below(body: unknown, ok = true): Promise<boolean> {
	await boxApiVersion(answering(body, ok), true);
	return boxBelowFloor(answering(body, ok));
}

describe('boxBelowFloor', () => {
	it('stands at the box that holds pages in blocks under contract 2', () => {
		// Raising it stops every box below it from opening this app until it
		// updates: make sure that was deliberate and has a release note.
		expect(MIN_BOX_API).toBe(4);
	});

	it('blocks a box below the floor, a box too old to report one included', async () => {
		expect(await below({ api_version: 3 })).toBe(true);
		expect(await below({ version: '0.1.9' })).toBe(true);
	});

	it('lets through a box at the floor or above it', async () => {
		expect(await below({ api_version: 4 })).toBe(false);
		expect(await below({ api_version: 9 })).toBe(false);
	});

	it('never blocks a box it could not reach', async () => {
		expect(await below({}, false)).toBe(false);
	});
});
