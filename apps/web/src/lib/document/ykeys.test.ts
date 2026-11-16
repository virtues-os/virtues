import { describe, expect, it } from 'vitest';
import { getSchema } from '@tiptap/core';
import { EditorState } from '@tiptap/pm/state';
import { ySyncPluginKey, yUndoPluginKey } from '@tiptap/y-tiptap';
import { Plugin } from '@tiptap/pm/state';
import { contractExtensions } from './schema';
import { isChangeOrigin, yUndoState } from './ykeys';

// ykeys.ts names y-tiptap's plugin keys by string so the composer never
// imports yjs. These go through real transactions, so a y-tiptap upgrade
// that renames a key fails here.
describe('ykeys', () => {
	const schema = getSchema(contractExtensions());

	it('reads the binding meta y-tiptap sets', () => {
		const state = EditorState.create({ schema });
		expect(isChangeOrigin(state.tr)).toBe(false);
		expect(isChangeOrigin(state.tr.setMeta(ySyncPluginKey, { isChangeOrigin: true }))).toBe(true);
	});

	it('reads the y-undo plugin state', () => {
		const undo = new Plugin({ key: yUndoPluginKey, state: { init: () => 'undo-state', apply: (_, v) => v } });
		const state = EditorState.create({ schema, plugins: [undo] });
		expect(yUndoState(state)).toBe('undo-state');
		expect(yUndoState(EditorState.create({ schema }))).toBeUndefined();
	});
});
