/**
 * Table actions for a block page, for `TableMenu` and the cell's context
 * menu (right-click, a long press where there is no right-click, or
 * Shift-F10): rows and columns added, moved or deleted, the header row
 * toggled, and a column aligned. Alignment is per column, as GFM has it:
 * every cell of the column takes the same `align`. A row or column moves by
 * one at a time, as the CodeMirror editor's drag handles move them, and the
 * header row stays first.
 */

import { Extension, type Editor } from '@tiptap/core';
import type { Node as PmNode } from '@tiptap/pm/model';
import { TextSelection, type EditorState, type Selection } from '@tiptap/pm/state';
import { CellSelection, TableMap, moveTableColumn, moveTableRow, rowIsHeader } from '@tiptap/pm/tables';
import type { ContextMenuItem } from '$lib/stores/contextMenu.svelte';

export type Align = 'left' | 'center' | 'right';

export interface TableAction {
	id: string;
	label: string;
	icon: string;
	/** Destructive actions read as such in a menu. */
	destructive?: boolean;
	/** Starts a new group in a menu. */
	groupStart?: boolean;
	/** Whether this is the column's alignment now. */
	checked?: boolean;
	/** Kept behind More in the table's bar, with the deletions. */
	more?: boolean;
	run: () => void;
}

interface CellAt {
	table: PmNode;
	/** Position of the table node. */
	tablePos: number;
	map: TableMap;
	/** The cell's column, counted from 0. */
	col: number;
	row: number;
}

/**
 * What the table's bar needs above its anchor to show whole: its tallest
 * button (44pt on touch), the bar's padding and border, and its offset
 * (`TableMenu.svelte`).
 */
export const TABLE_BAR_ROOM = 44 + 2 * 4 + 2 * 1 + 6;

/**
 * Where the table's bar is anchored: over the right end of the table's top
 * edge while that is in view; once a tall table's top has scrolled above
 * `top` (the top of the page's visible part), at the top of the view, over
 * the rows still shown, until its last row goes too. Left at the table's
 * top edge it sat above the window, and Alt-F10 put the keyboard in a bar
 * nobody could see.
 */
export function tableBarAnchor(table: { left: number; top: number; bottom: number; width: number }, top: number): { x: number; y: number; width: number } {
	return { x: table.left, y: Math.min(Math.max(table.top, top + TABLE_BAR_ROOM), table.bottom), width: table.width };
}

/**
 * The table and cell the selection is in, if it is in one. For a selection
 * of cells, the cell its head is in: `$headCell` is the position before
 * that cell, in its row.
 */
export function cellAt(state: EditorState): CellAt | null {
	const sel = state.selection;
	if (sel instanceof CellSelection) {
		const $cell = sel.$headCell;
		const table = $cell.node(-1);
		const tablePos = $cell.before(-1);
		const map = TableMap.get(table);
		const rect = map.findCell($cell.pos - tablePos - 1);
		return { table, tablePos, map, col: rect.left, row: rect.top };
	}
	const $pos = sel.$head;
	for (let d = $pos.depth; d > 0; d--) {
		const node = $pos.node(d);
		if (node.type.name !== 'table') continue;
		const tablePos = $pos.before(d);
		const map = TableMap.get(node);
		// The cell is the child of the row the position is in.
		const cellDepth = d + 2;
		if ($pos.depth < cellDepth) return null;
		const cellPos = $pos.before(cellDepth) - tablePos - 1;
		const rect = map.findCell(cellPos);
		return { table: node, tablePos, map, col: rect.left, row: rect.top };
	}
	return null;
}

/** The column's alignment, when every cell in it agrees on one. */
export function columnAlign(state: EditorState): Align | null {
	const at = cellAt(state);
	if (!at) return null;
	let align: Align | null | undefined;
	for (let row = 0; row < at.map.height; row++) {
		const cell = at.table.nodeAt(at.map.map[row * at.map.width + at.col]);
		const value = (cell?.attrs.align ?? null) as Align | null;
		if (align === undefined) align = value;
		else if (align !== value) return null;
	}
	return align ?? null;
}

/** Set `align` on every cell of the selection's column; null clears it. */
export function alignColumn(editor: Editor, align: Align | null): boolean {
	const at = cellAt(editor.state);
	if (!at) return false;
	const tr = editor.state.tr;
	const seen = new Set<number>();
	for (let row = 0; row < at.map.height; row++) {
		const offset = at.map.map[row * at.map.width + at.col];
		if (seen.has(offset)) continue;
		seen.add(offset);
		const pos = at.tablePos + 1 + offset;
		const cell = tr.doc.nodeAt(pos);
		if (cell && cell.attrs.align !== align) tr.setNodeMarkup(pos, undefined, { ...cell.attrs, align });
	}
	if (!tr.docChanged) return false;
	editor.view.dispatch(tr);
	return true;
}

/** Move the selection's row by `by` (-1 up, 1 down); the header row stays first. */
export function moveRow(editor: Editor, by: -1 | 1): boolean {
	const at = cellAt(editor.state);
	if (!at) return false;
	const first = rowIsHeader(at.map, at.table, 0) ? 1 : 0;
	const to = at.row + by;
	if (at.row < first || to < first || to >= at.map.height) return false;
	return moveTableRow({ from: at.row, to, pos: at.tablePos + 1 })(editor.state, editor.view.dispatch);
}

/** Move the selection's column by `by` (-1 left, 1 right). */
export function moveColumn(editor: Editor, by: -1 | 1): boolean {
	const at = cellAt(editor.state);
	if (!at) return false;
	const to = at.col + by;
	if (to < 0 || to >= at.map.width) return false;
	return moveTableColumn({ from: at.col, to, pos: at.tablePos + 1 })(editor.state, editor.view.dispatch);
}

/**
 * Table actions as a menu's rows: an alignment is one of a set of choices,
 * so its tick reads as selected.
 */
export function tableMenuItems(actions: TableAction[]): ContextMenuItem[] {
	return actions.map((a) => ({
		id: a.id,
		label: a.label,
		icon: a.icon,
		checked: a.checked,
		role: a.checked === undefined ? undefined : ('menuitemradio' as const),
		dividerBefore: a.groupStart,
		variant: a.destructive ? ('destructive' as const) : undefined,
		action: a.run,
	}));
}

/** What can be done to the table at the selection, in menu order. */
export function tableActions(editor: Editor): TableAction[] {
	const at = cellAt(editor.state);
	if (!at) return [];
	const chain = () => editor.chain().focus();
	const align = columnAlign(editor.state);
	const alignTo = (a: Align) => () => alignColumn(editor, align === a ? null : a);
	const first = rowIsHeader(at.map, at.table, 0) ? 1 : 0;
	const moves: TableAction[] = [
		...(at.row > first
			? [{ id: 'move-row-up', label: 'Move row up', icon: 'ri:arrow-up-line', run: () => void moveRow(editor, -1) }]
			: []),
		...(at.row >= first && at.row < at.map.height - 1
			? [{ id: 'move-row-down', label: 'Move row down', icon: 'ri:arrow-down-line', run: () => void moveRow(editor, 1) }]
			: []),
		...(at.col > 0
			? [{ id: 'move-col-left', label: 'Move column left', icon: 'ri:arrow-left-line', run: () => void moveColumn(editor, -1) }]
			: []),
		...(at.col < at.map.width - 1
			? [{ id: 'move-col-right', label: 'Move column right', icon: 'ri:arrow-right-line', run: () => void moveColumn(editor, 1) }]
			: []),
	].map((a, i) => ({ ...a, more: true, groupStart: i === 0 }));
	return [
		{ id: 'row-above', label: 'Row above', icon: 'ri:insert-row-top', run: () => chain().addRowBefore().run() },
		{ id: 'row-below', label: 'Row below', icon: 'ri:insert-row-bottom', run: () => chain().addRowAfter().run() },
		{ id: 'col-left', label: 'Column left', icon: 'ri:insert-column-left', run: () => chain().addColumnBefore().run() },
		{ id: 'col-right', label: 'Column right', icon: 'ri:insert-column-right', run: () => chain().addColumnAfter().run() },
		{ id: 'align-left', label: 'Align left', icon: 'ri:align-left', groupStart: true, checked: align === 'left', run: alignTo('left') },
		{ id: 'align-center', label: 'Align center', icon: 'ri:align-center', checked: align === 'center', run: alignTo('center') },
		{ id: 'align-right', label: 'Align right', icon: 'ri:align-right', checked: align === 'right', run: alignTo('right') },
		{ id: 'header-row', label: 'Header row', icon: 'ri:layout-top-line', groupStart: true, run: () => chain().toggleHeaderRow().run() },
		...moves,
		{ id: 'delete-row', label: 'Delete row', icon: 'ri:delete-row', destructive: true, groupStart: true, run: () => chain().deleteRow().run() },
		{ id: 'delete-col', label: 'Delete column', icon: 'ri:delete-column', destructive: true, run: () => chain().deleteColumn().run() },
		{ id: 'delete-table', label: 'Delete table', icon: 'ri:delete-bin-line', destructive: true, run: () => chain().deleteTable().run() },
	];
}

/**
 * The selection a cell's menu acts on, opened at `pos`: a selection of
 * cells holding the cell at `pos` stays, so Delete row deletes each row in
 * it and an alignment takes each column; anywhere else the caret goes to
 * `pos`.
 */
export function selectionForCellMenu(state: EditorState, pos: number): Selection {
	const sel = state.selection;
	if (sel instanceof CellSelection) {
		let holds = false;
		sel.forEachCell((cell, at) => {
			if (pos > at && pos < at + cell.nodeSize) holds = true;
		});
		if (holds) return sel;
	}
	return TextSelection.near(state.doc.resolve(pos));
}

/**
 * Enter in a cell's own line moves to the cell below, in the same column,
 * as it does in the CodeMirror editor's tables; in the last row it does
 * nothing. Shift-Enter breaks the line in the cell. A list, a quote or a
 * code block inside a cell takes Enter as it does anywhere.
 */
export const TableKeys = Extension.create({
	name: 'tableKeys',
	// Ahead of the editor's own Enter, which would split the cell's line.
	priority: 1000,
	addKeyboardShortcuts() {
		return {
			Enter: ({ editor }) => {
				const { state } = editor;
				const { $head } = state.selection;
				if (!(state.selection instanceof TextSelection) || $head.depth < 2) return false;
				const cell = $head.node(-1);
				if (cell.type.name !== 'tableCell' && cell.type.name !== 'tableHeader') return false;
				if ($head.parent.type.spec.code) return false;
				const at = cellAt(state);
				if (!at) return true;
				if (at.row + 1 >= at.map.height) return true;
				const below = at.tablePos + 1 + at.map.map[(at.row + 1) * at.map.width + at.col];
				const node = state.doc.nodeAt(below);
				if (!node) return true;
				const end = TextSelection.near(state.doc.resolve(below + node.nodeSize - 1), -1);
				editor.view.dispatch(state.tr.setSelection(end).scrollIntoView());
				return true;
			},
		};
	},
});
