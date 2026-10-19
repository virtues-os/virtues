import { describe, expect, it } from 'vitest';
import { fakeReach, judge, type ReachStatus } from './reconnect';

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
