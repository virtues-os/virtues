import { describe, expect, it } from 'vitest';
import { fakeReach, judge, pairingProblem, type ReachStatus } from './reconnect';

const status = (session: ReachStatus['session']): ReachStatus => ({ paired: true, session });

describe('judge', () => {
	it('opens the app the moment the server answers, whatever else is true', () => {
		expect(judge(status('authed'), false)).toBe('back');
	});

	it('a refusal is its own answer: pairing again, not waiting', () => {
		expect(judge(status('rejected'), true)).toBe('rejected');
	});

	it('says this device is offline before blaming the server', () => {
		expect(judge(status('unknown'), false)).toBe('offline');
		expect(judge(null, false)).toBe('offline');
	});

	it('otherwise the server is silent', () => {
		expect(judge(status('unknown'), true)).toBe('silent');
		expect(judge(null, true)).toBe('silent');
	});
});

describe('pairingProblem', () => {
	it('is nothing for a pairing that loads, and for an app too old to report one', () => {
		expect(pairingProblem(status('unknown'))).toBeNull();
		expect(pairingProblem({ paired: true, session: 'unknown', problem: null, keyMissing: false })).toBeNull();
		expect(pairingProblem(null)).toBeNull();
	});

	it('carries the plugin sentence, and pairing again only for a missing key', () => {
		const unreadable = "Virtues could not read this Mac's device key from the Keychain. Quit and reopen Virtues to try again.";
		expect(pairingProblem({ ...status('unknown'), problem: unreadable, keyMissing: false })).toEqual({
			message: unreadable,
			pairAgain: false,
		});
		const missing = "This Mac's device key is missing. Pair this Mac again to reach your server.";
		expect(pairingProblem({ ...status('unknown'), problem: missing, keyMissing: true })).toEqual({
			message: missing,
			pairAgain: true,
		});
	});
});

describe('fakeReach', () => {
	it('comes back after its silent spell', async () => {
		const reach = fakeReach(0);
		// speed 0: the twenty seconds are over at once.
		await new Promise((r) => setTimeout(r, 5));
		expect((await reach.status()).session).toBe('authed');
	});
});
