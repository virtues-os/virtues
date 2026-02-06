/**
 * Mark Decorations Plugin for ProseMirror
 *
 * Shows markdown delimiters (**, *, `, ~~) when cursor is inside formatted text.
 * This reveals the underlying markdown syntax for the current line, Typora-style.
 */

import { Plugin, PluginKey } from 'prosemirror-state';
import type { EditorState } from 'prosemirror-state';
import { Decoration, DecorationSet } from 'prosemirror-view';
import type { Mark } from 'prosemirror-model';

// =============================================================================
// PLUGIN KEY
// =============================================================================

export const markDecorationsKey = new PluginKey<DecorationSet>('markDecorations');

// =============================================================================
// TYPES
// =============================================================================

interface MarkDelimiters {
	open: string;
	close: string;
}

// Map mark types to their markdown delimiters
const MARK_DELIMITERS: Record<string, MarkDelimiters> = {
	strong: { open: '**', close: '**' },
	em: { open: '*', close: '*' },
	code: { open: '`', close: '`' },
	strikethrough: { open: '~~', close: '~~' },
};

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/**
 * Find all mark ranges in the current paragraph/block where cursor is located.
 * Returns ranges with their mark types and positions.
 */
function findMarkRangesInCurrentBlock(
	state: EditorState
): Array<{ from: number; to: number; mark: Mark }> {
	const { $from } = state.selection;
	const ranges: Array<{ from: number; to: number; mark: Mark }> = [];

	// Get the current block (paragraph, heading, etc.)
	const blockStart = $from.start();
	const blockEnd = $from.end();

	// Walk through the block content to find mark ranges
	state.doc.nodesBetween(blockStart, blockEnd, (node, pos) => {
		if (node.isText && node.marks.length > 0) {
			for (const mark of node.marks) {
				if (MARK_DELIMITERS[mark.type.name]) {
					// Check if this mark extends beyond this text node
					// We need to find the full extent of the mark
					let markFrom = pos;
					let markTo = pos + node.nodeSize;

					// Look backwards for same mark
					let checkPos = pos - 1;
					while (checkPos >= blockStart) {
						const $check = state.doc.resolve(checkPos);
						if ($check.parent.isText) {
							const prevNode = $check.parent;
							if (mark.isInSet(prevNode.marks)) {
								markFrom = checkPos - prevNode.nodeSize + 1;
								checkPos = markFrom - 1;
							} else {
								break;
							}
						} else {
							break;
						}
					}

					// Look forwards for same mark
					checkPos = pos + node.nodeSize;
					while (checkPos < blockEnd) {
						const nodeAfter = state.doc.nodeAt(checkPos);
						if (nodeAfter?.isText && mark.isInSet(nodeAfter.marks)) {
							markTo = checkPos + nodeAfter.nodeSize;
							checkPos = markTo;
						} else {
							break;
						}
					}

					// Avoid duplicates
					const exists = ranges.some(
						(r) => r.from === markFrom && r.to === markTo && r.mark.type.name === mark.type.name
					);
					if (!exists) {
						ranges.push({ from: markFrom, to: markTo, mark });
					}
				}
			}
		}
		return true; // Continue traversing
	});

	return ranges;
}

/**
 * Simpler approach: find mark boundaries by examining the text node marks
 */
function findMarkBoundaries(state: EditorState): Array<{ from: number; to: number; markType: string }> {
	const { $from } = state.selection;
	const boundaries: Array<{ from: number; to: number; markType: string }> = [];

	// Get the current block
	const blockStart = $from.start();
	const blockEnd = $from.end();

	// Track current marks and their start positions
	const activeMarks: Map<string, number> = new Map();

	let pos = blockStart;
	state.doc.nodesBetween(blockStart, blockEnd, (node, nodePos) => {
		if (!node.isText) return true;

		const nodeMarks = new Set(node.marks.map((m) => m.type.name));

		// Check for marks that ended
		for (const [markType, startPos] of activeMarks) {
			if (!nodeMarks.has(markType)) {
				// Mark ended at previous position
				if (MARK_DELIMITERS[markType]) {
					boundaries.push({ from: startPos, to: nodePos, markType });
				}
				activeMarks.delete(markType);
			}
		}

		// Check for marks that started
		for (const mark of node.marks) {
			const markType = mark.type.name;
			if (!activeMarks.has(markType) && MARK_DELIMITERS[markType]) {
				activeMarks.set(markType, nodePos);
			}
		}

		pos = nodePos + node.nodeSize;
		return true;
	});

	// Close any remaining marks
	for (const [markType, startPos] of activeMarks) {
		if (MARK_DELIMITERS[markType]) {
			boundaries.push({ from: startPos, to: pos, markType });
		}
	}

	return boundaries;
}

/**
 * Create widget decoration for delimiter
 */
function createDelimiterWidget(delimiter: string, position: 'open' | 'close'): HTMLSpanElement {
	const span = document.createElement('span');
	span.className = `pm-mark-delimiter pm-mark-delimiter-${position}`;
	span.textContent = delimiter;
	span.contentEditable = 'false';
	return span;
}

// =============================================================================
// PLUGIN
// =============================================================================

export function createMarkDecorationsPlugin() {
	return new Plugin({
		key: markDecorationsKey,

		state: {
			init() {
				return DecorationSet.empty;
			},

			apply(tr, decorations, oldState, newState) {
				// Only update if selection or doc changed
				if (!tr.selectionSet && !tr.docChanged) {
					return decorations.map(tr.mapping, tr.doc);
				}

				const { $from, empty } = newState.selection;

				// Only show decorations when cursor is collapsed (not selecting)
				if (!empty) {
					return DecorationSet.empty;
				}

				// Don't show in code blocks
				if ($from.parent.type.name === 'code_block') {
					return DecorationSet.empty;
				}

				// Find mark boundaries in current block
				const boundaries = findMarkBoundaries(newState);

				if (boundaries.length === 0) {
					return DecorationSet.empty;
				}

				// Create decorations for each mark boundary
				const decorationList: Decoration[] = [];

				for (const { from, to, markType } of boundaries) {
					const delimiters = MARK_DELIMITERS[markType];
					if (!delimiters) continue;

					// Opening delimiter (widget before the marked text)
					decorationList.push(
						Decoration.widget(from, () => createDelimiterWidget(delimiters.open, 'open'), {
							side: -1, // Position before
							marks: [],
						})
					);

					// Closing delimiter (widget after the marked text)
					decorationList.push(
						Decoration.widget(to, () => createDelimiterWidget(delimiters.close, 'close'), {
							side: 1, // Position after
							marks: [],
						})
					);
				}

				return DecorationSet.create(newState.doc, decorationList);
			},
		},

		props: {
			decorations(state) {
				return markDecorationsKey.getState(state);
			},
		},
	});
}
