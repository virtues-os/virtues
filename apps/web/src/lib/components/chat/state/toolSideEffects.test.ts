/**
 * What an `edit_page` result does outside the transcript, and how its card
 * reads it: a block page's result names the blocks it wrote, which the
 * animation flashes; results stored before block pages render as they did.
 */

import { describe, expect, it, vi } from 'vitest';

const animate = vi.hoisted(() => vi.fn());
vi.mock('$lib/ai/aiPresence', () => ({ animateChatEdit: animate }));

import { ToolSideEffects, isFullReplace } from './toolSideEffects';

function editTurn(toolCallId: string, output: unknown) {
	return {
		role: 'assistant',
		parts: [{ type: 'tool-edit_page', state: 'output-available', toolCallId, output }],
	};
}

const treeEdit = {
	applied: true,
	edit: {
		edit_id: 'edit_tree',
		page_id: 'page_abc',
		format: 'tree',
		find: 'Lunch with [@Nick](/person/person_1).',
		replace: 'Lunch with [@Nick](/person/person_1) on Friday.',
		blocks: ['p0q9z1mm', 't7c2k1aa'],
	},
};

const markdownEdit = {
	applied: true,
	edit: { edit_id: 'edit_md', page_id: 'page_md', find: 'Old line.', replace: 'New line.' },
};

describe('the edit animation', () => {
	it('passes a block page edit its blocks, and a markdown edit none', () => {
		const effects = new ToolSideEffects();
		effects.animateNewEdits([]); // the seeding run, over an empty history
		effects.animateNewEdits([editTurn('c1', treeEdit), editTurn('c2', markdownEdit)]);
		expect(animate).toHaveBeenCalledWith('page_abc', ['p0q9z1mm', 't7c2k1aa']);
		expect(animate).toHaveBeenCalledWith('page_md', []);
	});

	it('animates nothing for a refused edit, which has no edit to show', () => {
		animate.mockClear();
		const effects = new ToolSideEffects();
		effects.animateNewEdits([]);
		effects.animateNewEdits([
			editTurn('c3', { applied: false, status: 'refused', refused: [{ at: 'op 1', message: 'x' }], base: 'b' }),
		]);
		expect(animate).not.toHaveBeenCalled();
	});
});

describe('the edit card', () => {
	it('shows a stored markdown result as it always has: an empty find is the whole page', () => {
		expect(isFullReplace({ find: '', replace: '# New page' })).toBe(true);
		expect(isFullReplace({ find: 'Old line.', replace: 'New line.' })).toBe(false);
	});

	it('shows a block page result as a change, even when it only added blocks', () => {
		expect(isFullReplace(treeEdit.edit)).toBe(false);
		expect(isFullReplace({ find: '', replace: '- [ ] Book seats', blocks: ['t7c2k1aa'] })).toBe(false);
	});
});

function setupTurn(toolCallId: string, output: unknown) {
	return {
		role: 'assistant',
		parts: [{ type: 'tool-applet', state: 'output-available', toolCallId, input: { action: 'setup' }, output }],
	};
}

describe('opening what the assistant made', () => {
	it('opens nothing that was already in the chat, then each new page once', () => {
		const effects = new ToolSideEffects();
		const old = {
			role: 'assistant',
			parts: [{ type: 'tool-create_page', state: 'output-available', toolCallId: 'c1', output: { page_id: 'page_1', title: 'Notes' } }],
		};
		expect(effects.collectNewPages([old])).toEqual([]);
		const fresh = {
			role: 'assistant',
			parts: [{ type: 'tool-create_page', state: 'output-available', toolCallId: 'c2', output: { page_id: 'page_2' } }],
		};
		expect(effects.collectNewPages([old, fresh])).toEqual([{ pageId: 'page_2', title: '' }]);
		expect(effects.collectNewPages([old, fresh])).toEqual([]);
	});

	it('opens a site when setup made or rebuilt one with a view', () => {
		const effects = new ToolSideEffects();
		const old = setupTurn('s1', { status: 'created', applet_id: 'applet_user__water_log', name: 'Water log', has_view: true });
		expect(effects.collectNewSites([old])).toEqual([]);

		const rebuilt = setupTurn('s2', { status: 'updated', applet_id: 'applet_user__water_log', name: 'Water log', has_view: true });
		expect(effects.collectNewSites([old, rebuilt])).toEqual([{ appletId: 'applet_user__water_log', name: 'Water log' }]);
		expect(effects.collectNewSites([old, rebuilt])).toEqual([]);
	});

	it('leaves alone a setup with no view, a refused one, and other applet actions', () => {
		const effects = new ToolSideEffects();
		expect(effects.collectNewSites([])).toEqual([]);
		const turns = [
			setupTurn('s1', { status: 'created', applet_id: 'applet_user__week_in_review', name: 'Week in review', has_view: false }),
			setupTurn('s2', { status: 'check_failed', applet_id: 'applet_user__water_log', has_view: true }),
			{
				role: 'assistant',
				parts: [{ type: 'tool-applet', state: 'output-available', toolCallId: 's3', input: { action: 'run' }, output: { status: 'created', applet_id: 'applet_user__water_log', has_view: true } }],
			},
		];
		expect(effects.collectNewSites(turns)).toEqual([]);
	});

	it('starts over on a new transcript', () => {
		const effects = new ToolSideEffects();
		const made = setupTurn('s1', { status: 'created', applet_id: 'applet_user__water_log', name: 'Water log', has_view: true });
		effects.collectNewSites([]);
		expect(effects.collectNewSites([made])).toHaveLength(1);
		effects.reset();
		expect(effects.collectNewSites([made])).toEqual([]);
	});
});
