/**
 * Settings > Assistant > How it talks to you. The notes save when the field is
 * left; these check that a failed save keeps the person's words and can send
 * them again, and the counter near the limit.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

vi.mock('$lib/api/client', () => ({
	updateAssistantProfile: vi.fn(async () => ({})),
}));

const { default: StyleNotesSettings } = await import('./StyleNotesSettings.svelte');
const { updateAssistantProfile } = await import('$lib/api/client');

let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;

beforeEach(() => {
	vi.mocked(updateAssistantProfile).mockReset();
	vi.mocked(updateAssistantProfile).mockResolvedValue({});
	target = document.createElement('div');
	document.body.appendChild(target);
	mounted = mount(StyleNotesSettings, {
		target,
		props: { notes: 'Short answers.', assistantName: 'Nick' },
	});
	flushSync();
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	target.remove();
});

const field = () => target.querySelector('textarea') as HTMLTextAreaElement;
const text = () => (target.textContent ?? '').replace(/\s+/g, ' ');
const tryAgain = () =>
	[...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Try again');

function write(value: string) {
	const area = field();
	area.focus();
	area.value = value;
	area.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
}

describe('StyleNotesSettings', () => {
	it('keeps the new notes when the save fails, and Try again sends them', async () => {
		vi.mocked(updateAssistantProfile).mockRejectedValueOnce(new Error('500'));
		write('Push back when I am wrong.');
		field().blur();
		flushSync();

		await vi.waitFor(() =>
			expect(text()).toContain(
				"Your server couldn't save these notes. It kept your earlier ones.",
			),
		);
		expect(field().value).toBe('Push back when I am wrong.');

		tryAgain()!.click();
		await vi.waitFor(() => expect(tryAgain()).toBeUndefined());
		expect(vi.mocked(updateAssistantProfile).mock.calls).toEqual([
			[{ style_notes: 'Push back when I am wrong.' }],
			[{ style_notes: 'Push back when I am wrong.' }],
		]);
		expect(field().value).toBe('Push back when I am wrong.');
	});

	it('counts only near the limit, with both figures grouped', async () => {
		write('a'.repeat(900));
		expect(text()).not.toContain('of 1,000');
		write('a'.repeat(1200));
		// Svelte puts a corrected value back into the field a tick later.
		await vi.waitFor(() => expect(field().value).toHaveLength(1000));
		expect(text()).toContain('1,000 of 1,000');
		expect(text()).toContain('Nick has a manner of its own. Where yours differs, yours wins.');
	});
});
