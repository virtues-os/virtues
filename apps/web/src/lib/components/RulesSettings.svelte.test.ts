/**
 * Settings > Assistant > Rules. Every save replaces the whole set, so these
 * check what each action sends, that the interview's pending proposals are
 * never this screen's to drop, and that a failed save puts back what the
 * server has.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, unmount } from 'svelte';

type Rule = { id: string; rule: string; kind: 'avoid' | 'defend'; active: boolean };

let serverRules: Rule[] = [];

vi.mock('$lib/api/client', () => ({
	getNarrativeRules: vi.fn(async () => ({
		rules: serverRules,
		proposed: [{ id: 'proposed_000', rule: 'my old job', kind: 'avoid', active: false }],
	})),
	saveNarrativeRules: vi.fn(async () => {}),
}));

const { default: RulesSettings } = await import('./RulesSettings.svelte');
const { saveNarrativeRules } = await import('$lib/api/client');

let target: HTMLElement;
let mounted: ReturnType<typeof mount> | null = null;

beforeEach(() => {
	serverRules = [
		{ id: 'rule_000', rule: 'my ex', kind: 'avoid', active: true },
		{ id: 'rule_001', rule: 'no work email on Sundays', kind: 'defend', active: true },
	];
	vi.mocked(saveNarrativeRules).mockReset();
	vi.mocked(saveNarrativeRules).mockResolvedValue(undefined);
	target = document.createElement('div');
	document.body.appendChild(target);
});

afterEach(() => {
	vi.useRealTimers();
	if (mounted) unmount(mounted);
	mounted = null;
	target.remove();
	document.body.replaceChildren();
});

const text = () => (target.textContent ?? '').replace(/\s+/g, ' ');
const button = (label: string) =>
	[...target.querySelectorAll('button')].find(
		(b) => b.getAttribute('aria-label') === label || b.textContent?.trim() === label,
	) as HTMLButtonElement;
const lastSent = () => {
	const calls = vi.mocked(saveNarrativeRules).mock.calls;
	return calls[calls.length - 1];
};

function type(input: HTMLInputElement, value: string) {
	input.value = value;
	input.dispatchEvent(new Event('input', { bubbles: true }));
	flushSync();
}

function press(input: HTMLElement, key: string) {
	input.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true }));
	flushSync();
}

async function open() {
	mounted = mount(RulesSettings, { target, props: { assistantName: 'Nick' } });
	flushSync();
	await vi.waitFor(() => expect(text()).toContain('no work email on Sundays'));
}

describe('RulesSettings', () => {
	it('lists the rules in force, oldest first, and never the proposals', async () => {
		await open();
		const rows = [...target.querySelectorAll('li')].map((li) => li.textContent?.replace(/\s+/g, ' ').trim());
		expect(rows).toEqual(['Never bring up my ex', 'Hold me to no work email on Sundays']);
		expect(text()).not.toContain('my old job');
		expect(text()).toContain('Nick follows these in chat.');
	});

	it('adds a rule at the bottom and sends the whole set, keeping proposals', async () => {
		await open();
		// The kind select is not driven here: happy-dom never matches
		// `:checked` on an option, which is what Svelte's select binding reads.
		const input = target.querySelector('.add input') as HTMLInputElement;
		expect(target.querySelector('.add .placeholder')?.textContent?.trim()).toBe('my ex');
		expect(button('Add rule').disabled).toBe(true);
		type(input, '  the move ');
		expect(button('Add rule').disabled).toBe(false);
		press(input, 'Enter');

		await vi.waitFor(() => expect(saveNarrativeRules).toHaveBeenCalledOnce());
		expect(lastSent()).toEqual([
			[
				{ rule: 'my ex', kind: 'avoid' },
				{ rule: 'no work email on Sundays', kind: 'defend' },
				{ rule: 'the move', kind: 'avoid' },
			],
			{ keepProposed: true },
		]);
		expect(input.value).toBe('');
	});

	it('edits in place: Enter saves, Escape leaves the rule as it was', async () => {
		await open();
		button('Edit rule').click();
		flushSync();
		let input = target.querySelector('li input') as HTMLInputElement;
		type(input, 'my ex, or anything about him');
		press(input, 'Escape');
		expect(saveNarrativeRules).not.toHaveBeenCalled();
		expect(text()).toContain('Never bring up my ex');

		button('Edit rule').click();
		flushSync();
		input = target.querySelector('li input') as HTMLInputElement;
		type(input, 'my ex, or anything about him');
		press(input, 'Enter');
		await vi.waitFor(() => expect(saveNarrativeRules).toHaveBeenCalledOnce());
		expect(lastSent()?.[0][0]).toEqual({ rule: 'my ex, or anything about him', kind: 'avoid' });
	});

	it('waits six seconds before a removal is saved, and Undo keeps the rule', async () => {
		vi.useFakeTimers();
		await open();
		button('Remove rule').click();
		flushSync();
		expect(text()).toContain('Removed.');
		button('Undo').click();
		flushSync();
		vi.advanceTimersByTime(6000);
		expect(saveNarrativeRules).not.toHaveBeenCalled();
		expect(text()).toContain('Never bring up my ex');

		button('Remove rule').click();
		flushSync();
		vi.advanceTimersByTime(5999);
		expect(saveNarrativeRules).not.toHaveBeenCalled();
		vi.advanceTimersByTime(1);
		await vi.waitFor(() => expect(saveNarrativeRules).toHaveBeenCalledOnce());
		expect(lastSent()).toEqual([
			[{ rule: 'no work email on Sundays', kind: 'defend' }],
			{ keepProposed: true },
		]);
	});

	it('moves focus from Undo to the next rule when the removal is saved', async () => {
		vi.useFakeTimers();
		await open();
		button('Remove rule').click();
		flushSync();
		expect(target.querySelector('[role="status"]')?.textContent).toBe('Removed.');
		button('Undo').focus();
		expect(document.activeElement).toBe(button('Undo'));

		vi.advanceTimersByTime(6000);
		await vi.waitFor(() => expect(saveNarrativeRules).toHaveBeenCalledOnce());
		await vi.waitFor(() => expect(document.activeElement).toBe(button('Edit rule')));
		expect(target.querySelectorAll('li')).toHaveLength(1);
	});

	it('commits a removal still waiting out its Undo when the page is left', async () => {
		await open();
		button('Remove rule').click();
		flushSync();
		unmount(mounted!);
		mounted = null;
		await vi.waitFor(() => expect(saveNarrativeRules).toHaveBeenCalledOnce());
		expect(lastSent()?.[0]).toEqual([{ rule: 'no work email on Sundays', kind: 'defend' }]);
	});

	it('puts back what the server has when a save fails, and says so', async () => {
		await open();
		vi.mocked(saveNarrativeRules).mockRejectedValueOnce(new Error('500'));
		const input = target.querySelector('.add input') as HTMLInputElement;
		type(input, 'the move');
		press(input, 'Enter');

		await vi.waitFor(() =>
			expect(text()).toContain(
				"Your server couldn't save the rules. They're as they were. Try again in a moment.",
			),
		);
		expect(target.querySelectorAll('li')).toHaveLength(2);
		expect(input.value).toBe('the move');
	});

	it('offers no way to save over rules it could not load', async () => {
		const { getNarrativeRules } = await import('$lib/api/client');
		vi.mocked(getNarrativeRules).mockRejectedValueOnce(new Error('offline'));
		mounted = mount(RulesSettings, { target, props: {} });
		flushSync();
		await vi.waitFor(() => expect(text()).toContain("Your server couldn't load the rules."));
		expect(target.querySelector('.add')).toBeNull();
		expect(text()).toContain('Ari follows these in chat');
	});
});
