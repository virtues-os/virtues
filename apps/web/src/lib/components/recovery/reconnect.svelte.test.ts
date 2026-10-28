/**
 * The recovery screen when this device's own pairing is the trouble: it
 * shows the plugin's sentence and stays put, and never sends the owner to
 * pair again on its own.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ReachPort, ReachStatus } from './reconnect';

vi.mock('$lib/tauri/boxRadio', () => ({
	boxRadio: { available: true, discover: vi.fn(async () => []) },
	BoxRadioError: class extends Error {},
}));
vi.mock('$lib/components/setup/prepair.svelte', () => ({ prePair: { startOver: vi.fn() } }));

const { Reconnect } = await import('./reconnect.svelte');
const { prePair } = await import('$lib/components/setup/prepair.svelte');

const KEYCHAIN = "Virtues could not read this Mac's device key from the Keychain. Quit and reopen Virtues to try again.";
const MISSING = "This Mac's device key is missing. Pair this Mac again to reach your server.";

function port(...answers: ReachStatus[]): ReachPort & { forget: ReturnType<typeof vi.fn> } {
	return {
		status: vi.fn(async () => answers.length > 1 ? answers.shift()! : answers[0]),
		rehome: vi.fn(async () => true),
		forget: vi.fn(async () => {}),
		online: () => true,
	};
}

let replace: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
	replace = vi.spyOn(window.location, 'replace').mockImplementation(() => {});
	vi.mocked(prePair.startOver).mockClear();
});
afterEach(() => replace.mockRestore());

describe('a pairing this device cannot use', () => {
	it('shows the Keychain sentence and does not send the owner to pair again', async () => {
		const reach = port({ paired: true, session: 'unknown', problem: KEYCHAIN, keyMissing: false });
		const r = new Reconnect(reach);
		await r.check();

		expect(r.error).toBe(KEYCHAIN);
		expect(r.problem).toEqual({ message: KEYCHAIN, pairAgain: false });
		expect(r.verdict).toBe('silent');
		expect(reach.forget).not.toHaveBeenCalled();
		expect(prePair.startOver).not.toHaveBeenCalled();
		expect(replace).not.toHaveBeenCalled();
	});

	it('offers pairing again only when the key is missing', async () => {
		const r = new Reconnect(port({ paired: true, session: 'unknown', problem: MISSING, keyMissing: true }));
		await r.check();

		expect(r.problem).toEqual({ message: MISSING, pairAgain: true });
		expect(r.error).toBe(MISSING);
		expect(replace).not.toHaveBeenCalled();
	});

	it('clears the sentence once the pairing loads again', async () => {
		const r = new Reconnect(
			port(
				{ paired: true, session: 'unknown', problem: KEYCHAIN, keyMissing: false },
				{ paired: true, session: 'unknown' },
			),
		);
		await r.check();
		await r.check();

		expect(r.problem).toBeNull();
		expect(r.error).toBeNull();
		expect(r.verdict).toBe('silent');
	});

	it('still sends a device with no pairing to setup', async () => {
		const reach = port({ paired: false, session: 'unpaired' });
		await new Reconnect(reach).check();

		expect(reach.forget).not.toHaveBeenCalled();
		expect(prePair.startOver).toHaveBeenCalled();
		expect(replace).toHaveBeenCalledWith('/setup');
	});
});
