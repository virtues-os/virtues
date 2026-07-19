/**
 * Shiki Syntax Highlighting Extension
 *
 * Async ViewPlugin that applies Shiki token-level mark decorations
 * inside fenced code blocks. Uses the theme's --shiki-theme CSS variable
 * to match the active app theme.
 *
 * Mark decorations are inline, so ViewPlugin is fine (no block restriction).
 * Falls back gracefully — code is unstyled until Shiki loads, then re-renders.
 */

import { syntaxTree } from '@codemirror/language';
import type { Extension, Range } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';
import { highlightCode } from '$lib/shiki/highlighter';

// ---------------------------------------------------------------------------
// ViewPlugin (highlighter singleton lives in $lib/shiki/highlighter)
// ---------------------------------------------------------------------------

const shikiPlugin = ViewPlugin.fromClass(
	class {
		decorations: DecorationSet = Decoration.none;
		version = 0;
		prevBlockCount = 0;

		constructor(view: EditorView) {
			this.computeDecorations(view);
		}

		update(update: ViewUpdate) {
			if (update.docChanged) {
				this.computeDecorations(update.view);
				return;
			}

			// Recompute when the syntax tree changes (Lezer parses incrementally)
			// Check if FencedCode nodes appeared that weren't there before
			let blockCount = 0;
			syntaxTree(update.state).iterate({
				enter(node) {
					if (node.name === 'FencedCode') blockCount++;
				},
			});
			if (blockCount !== this.prevBlockCount) {
				this.computeDecorations(update.view);
			}
		}

		async computeDecorations(view: EditorView) {
			const myVersion = ++this.version;

			// Collect code blocks from syntax tree
			const blocks: { from: number; lang: string; code: string; startLineNum: number }[] = [];

			syntaxTree(view.state).iterate({
				enter(node) {
					if (node.name !== 'FencedCode') return;

					const state = view.state;
					const firstLine = state.doc.lineAt(node.from);
					const fenceMatch = firstLine.text.match(/^```(\w+)/);
					if (!fenceMatch) return;

					const lang = fenceMatch[1];
					const codeStart = firstLine.to + 1;

					// Find closing fence
					const lastLine = state.doc.lineAt(Math.max(node.to - 1, node.from));
					const codeEnd = lastLine.text.startsWith('```') && lastLine.number !== firstLine.number
						? lastLine.from
						: node.to;

					if (codeStart >= codeEnd) return;

					// Trim trailing newline — Shiki adds an empty line for it
					let code = state.sliceDoc(codeStart, codeEnd);
					if (code.endsWith('\n')) code = code.slice(0, -1);

					blocks.push({
						from: codeStart,
						lang,
						code,
						startLineNum: firstLine.number + 1,
					});
				},
			});

			this.prevBlockCount = blocks.length;

			if (blocks.length === 0) {
				if (this.decorations !== Decoration.none) {
					this.decorations = Decoration.none;
					this.triggerRedraw(view);
				}
				return;
			}

			// Highlight all blocks (parallel)
			const results = await Promise.all(
				blocks.map(async (block) => ({
					...block,
					tokens: await highlightCode(block.code, block.lang),
				}))
			);

			// Stale check — doc may have changed during async
			if (this.version !== myVersion) return;

			const builder: Range<Decoration>[] = [];

			for (const { startLineNum, tokens } of results) {
				if (!tokens) continue;

				for (let lineIdx = 0; lineIdx < tokens.length; lineIdx++) {
					const docLineNum = startLineNum + lineIdx;
					if (docLineNum > view.state.doc.lines) break;
					const docLine = view.state.doc.line(docLineNum);

					for (const token of tokens[lineIdx]) {
						const tokenFrom = docLine.from + token.offset;
						const tokenTo = tokenFrom + token.content.length;

						// Bounds check
						if (tokenFrom < docLine.from || tokenTo > docLine.to) continue;
						if (tokenFrom >= tokenTo) continue;

						if (token.color) {
							builder.push(
								Decoration.mark({
									attributes: { style: `color: ${token.color}` },
								}).range(tokenFrom, tokenTo)
							);
						}
					}
				}
			}

			this.decorations = Decoration.set(builder, true);
			this.triggerRedraw(view);
		}

		triggerRedraw(view: EditorView) {
			// Empty dispatch to make CM6 pick up new decorations
			requestAnimationFrame(() => {
				view.dispatch();
			});
		}
	},
	{
		decorations: (v) => v.decorations,
	}
);

export const shikiHighlight: Extension = shikiPlugin;
