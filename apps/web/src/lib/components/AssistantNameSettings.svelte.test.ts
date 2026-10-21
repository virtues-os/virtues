/**
 * Settings > Assistant > Name. The name saves when the field is left; these
 * check that a failed save keeps what the person typed and can be sent again,
 * and that clearing the field goes back to the default by name.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

vi.mock('$lib/api/client', () => ({
	updateAssistantProfile: vi.fn(async () => ({})),
}));

const { default: AssistantNameSettings } = await import('./AssistantNameSettings.svelte');
const { updateAssistantProfile } = await import('$lib/api/client');

let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;

beforeEach(() => {
	vi.mocked(updateAssistantProfile).mockReset();
	vi.mocked(updateAssistantProfile).mockResolvedValue({});
	target = document.createElement('div');
	document.body.appendChild(target);
	mounted = mount(AssistantNameSettings, { target, props: { name: 'Nick' } });
	flushSync();
});

afterEach(() => {
	if (mounted) unmount(mounted);
	mounted = null;
	target.remove();
});

const field = () => target.querySelector('input') as HTMLInputElement;
const text = () => (target.textContent ?? '').replace(/\s+/g, ' ');
const tryAgain = () =>
	[...target.querySelectorAll('button')].find((b) => b.textContent?.trim() === 'Try again');

function enter(value: string) {
	const input = field();
	input.focus();
	input.value = value;
	input.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
	input.blur();
	flushSync();
}

describe('AssistantNameSettings', () => {
	it('keeps the typed name when the save fails, and Try again sends it', async () => {
		vi.mocked(updateAssistantProfile).mockRejectedValueOnce(new Error('500'));
		enter('David Okafor');

		await vi.waitFor(() =>
			expect(text()).toContain("Your server couldn't save the name. It kept Nick."),
		);
		expect(field().value).toBe('David Okafor');

		tryAgain()!.click();
		await vi.waitFor(() => expect(tryAgain()).toBeUndefined());
		expect(vi.mocked(updateAssistantProfile).mock.calls).toEqual([
			[{ assistant_name: 'David Okafor' }],
			[{ assistant_name: 'David Okafor' }],
		]);
		expect(text()).not.toContain("couldn't save");
		expect(field().value).toBe('David Okafor');
	});

	it('holds the field while a save is out', async () => {
		let finish!: () => void;
		vi.mocked(updateAssistantProfile).mockReturnValueOnce(
			new Promise((resolve) => (finish = () => resolve({}))),
		);
		enter('David');
		await vi.waitFor(() => expect(field().disabled).toBe(true));
		finish();
		await vi.waitFor(() => expect(field().disabled).toBe(false));
		expect(field().value).toBe('David');
	});

	it('sends the default by name when the field is cleared', async () => {
		enter('   ');
		await vi.waitFor(() => expect(updateAssistantProfile).toHaveBeenCalledOnce());
		expect(updateAssistantProfile).toHaveBeenCalledWith({ assistant_name: 'Ari' });
		expect(field().value).toBe('Ari');
	});

	it('keeps a pasted paragraph to 100 characters', async () => {
		const input = field();
		input.focus();
		input.value = 'N'.repeat(140);
		input.dispatchEvent(new Event('input', { bubbles: true }));
		// Svelte puts a corrected value back into the field a tick later.
		await vi.waitFor(() => expect(input.value).toHaveLength(100));
	});
});
