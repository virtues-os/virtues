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
		expect(animate).toHaveBeenCalledWith('page_abc', treeEdit.edit.replace, ['p0q9z1mm', 't7c2k1aa']);
		expect(animate).toHaveBeenCalledWith('page_md', 'New line.', []);
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
