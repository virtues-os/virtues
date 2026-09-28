import { describe, expect, it } from 'vitest';
import { BoxRadioError, boxLabel, fakeBoxRadio } from './boxRadio';

// The fake is what the setup flow is built against at localhost, so it has to
// fail the way a real server fails — same codes, same recoverable states.
const radio = () =>
	fakeBoxRadio({
		speed: 0,
		boxes: [
			{ id: 'a', name: 'Virtues-Quaint-Tern', label: 'Quaint Tern', rssi: -50, state: 'needs-wifi' },
			{ id: 'b', name: 'Virtues-Honest-Kestrel', label: 'Honest Kestrel', rssi: -60, state: 'needs-owner' }
		]
	});

describe('fakeBoxRadio', () => {
	it('refuses the wrong words with `refused`', async () => {
		const r = radio();
		const [box] = await r.discover();
		await expect(r.openSetup(box, 'wrong words here now')).rejects.toMatchObject({ code: 'refused' });
	});

	it('accepts the words however they are spaced', async () => {
		const r = radio();
		const [box] = await r.discover();
		const link = await r.openSetup(box, 'Mango burly  skull dough');
		expect(link.purpose).toBe('setup');
		expect(link.gated).toBe(true);
	});

	it('a wrong password is join-failed and the link stays usable', async () => {
		const r = radio();
		const [box] = await r.discover();
		const link = await r.openSetup(box, 'mango-burly-skull-dough');
		const err = await link.join({ ssid: 'Home', password: 'wrongpassword' }).catch((e) => e);
		expect(err).toBeInstanceOf(BoxRadioError);
		expect(err.code).toBe('join-failed');
		const stages: string[] = [];
		const ok = await link.join({ ssid: 'Home', password: 'right' }, (s) => stages.push(s));
		expect(ok.url).toMatch(/^http:\/\//);
		expect(stages).toEqual(['sent', 'joining', 'joined']);
	});

	it('openOwner with no box finds the one asking for its owner', async () => {
		const link = await radio().openOwner();
		expect(link.box.state).toBe('needs-owner');
		expect(link.purpose).toBe('owner');
		expect((await link.scan()).length).toBeGreaterThan(0);
	});

	it('openOwner says not-found when nothing nearby wants its owner', async () => {
		const r = fakeBoxRadio({ speed: 0 });
		await expect(r.openOwner()).rejects.toMatchObject({ code: 'not-found' });
	});
});

describe('boxLabel', () => {
	it("keeps the brand on a number, the way the server's screen says it", () => {
		expect(boxLabel('Virtues-4812')).toBe('Virtues 4812');
		expect(boxLabel('[Virtues-0371]')).toBe('Virtues 0371');
		expect(boxLabel('Virtues 4812')).toBe('Virtues 4812');
	});

	it("turns an older server's codename into a name for people", () => {
		expect(boxLabel('Virtues-Quaint-Tern')).toBe('Quaint Tern');
		// A leading bracket arrived from CoreBluetooth once (2026-08-13).
		expect(boxLabel('[Virtues Honest Kestrel')).toBe('Honest Kestrel');
		expect(boxLabel('Quaint Tern')).toBe('Quaint Tern');
	});

	it('never says "box" or nothing', () => {
		// The native fallback is "Virtues box"; "Setting up box" reads as a bug.
		expect(boxLabel('Virtues box')).toBe('Your server');
		expect(boxLabel('')).toBe('Your server');
		expect(boxLabel(undefined)).toBe('Your server');
	});
});
