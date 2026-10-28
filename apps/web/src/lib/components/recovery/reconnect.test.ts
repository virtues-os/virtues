import { describe, expect, it } from 'vitest';
import { fakeReach, judge, pairingProblem, type ReachStatus } from './reconnect';

const status = (session: ReachStatus['session']): ReachStatus => ({ paired: true, session });

describe('judge', () => {
	it('opens the app the moment the server answers, whatever else is true', () => {
		expect(judge(status('authed'), true, false)).toBe('back');
	});

	it('a refusal is its own answer: pairing again, not waiting', () => {
		expect(judge(status('rejected'), true, true)).toBe('rejected');
	});

	it('offers the Bluetooth fix even with this device offline', () => {
		// Bluetooth needs no network, and a server that moved is the likeliest
		// reason both are true at once (a new place, no Wi-Fi yet).
		expect(judge(status('unknown'), true, false)).toBe('asking');
	});

	it('says this device is offline before blaming the server', () => {
		expect(judge(status('unknown'), false, false)).toBe('offline');
		expect(judge(null, false, false)).toBe('offline');
	});

	it('otherwise the server is silent', () => {
		expect(judge(status('unknown'), false, true)).toBe('silent');
		expect(judge(null, false, true)).toBe('silent');
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
	it('stays unreachable until the server re-homes, then comes back', async () => {
		const reach = fakeReach(0);
		expect((await reach.status()).session).toBe('unknown');
		await reach.rehome('http://192.168.1.50:8000');
		// speed 0: "a few seconds later" is now.
		await new Promise((r) => setTimeout(r, 5));
		expect((await reach.status()).session).toBe('authed');
	});
});
