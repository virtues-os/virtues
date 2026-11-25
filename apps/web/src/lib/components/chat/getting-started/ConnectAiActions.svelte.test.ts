/**
 * Connect AI's emailed sign-in: after the link is sent, the page names the
 * link code the email names too, so the owner can match the two before
 * choosing Attach box. A server too old to send the code shows no code line.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';

const api = vi.hoisted(() => ({
	setupLoginStart: vi.fn(async (): Promise<{ status: string; user_code?: string }> => ({ status: 'sent' })),
	setupLinkPoll: vi.fn(async () => ({ status: 'pending' })),
	setByoKey: vi.fn(async () => {}),
}));
vi.mock('$lib/api/client', () => api);
vi.mock('$lib/api/checkout', () => ({ startCheckout: vi.fn() }));
vi.mock('$lib/tauri/bridge', () => ({ openExternal: vi.fn(async () => {}) }));
vi.mock('$lib/stores/gettingStarted.svelte', () => ({
	gettingStarted: { refresh: vi.fn(async () => {}), aiConnected: false },
}));
vi.mock('$lib/stores/motion.svelte', () => ({ reducedMotion: () => true }));

const { default: ConnectAiActions } = await import('./ConnectAiActions.svelte');

let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;
const text = () => (target.textContent ?? '').replace(/\s+/g, ' ');
const button = (name: string) =>
	[...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === name) as HTMLButtonElement | undefined;

async function sendLink() {
	button('Sign in')!.click();
	flushSync();
	const email = target.querySelector<HTMLInputElement>('#gs-email')!;
	email.value = 'owner@example.com';
	email.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
	button('Send a link')!.click();
	// The send awaits the server, then the page re-renders.
	for (let i = 0; i < 5; i++) await tick();
	flushSync();
}

beforeEach(() => {
	vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] });
	vi.clearAllMocks();
	target = document.createElement('div');
	document.body.appendChild(target);
	mounted = mount(ConnectAiActions, { target });
	flushSync();
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	target.remove();
	vi.useRealTimers();
});

describe('the emailed sign-in', () => {
	it('names the link code the email names', async () => {
		api.setupLoginStart.mockResolvedValueOnce({ status: 'sent', user_code: 'K7QM-3RXP' });
		await sendLink();
		expect(api.setupLoginStart).toHaveBeenCalledWith('owner@example.com');
		expect(text()).toContain('Your link code is K7QM-3RXP. Check that the email shows the same one.');
	});

	it('shows no code line for a server that sends none', async () => {
		await sendLink();
		expect(text()).toContain('A sign-in link is on its way to your email.');
		expect(text()).not.toContain('link code');
	});
});
