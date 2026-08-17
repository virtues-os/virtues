/**
 * Code Block Decorations
 *
 * Adds a header widget to fenced code blocks — language picker and copy
 * button. The actual syntax highlighting is handled by CM6's built-in
 * markdown + language-data.
 *
 * Uses StateField (not ViewPlugin) because block widgets require direct
 * decoration provision via EditorView.decorations facet.
 */

import { syntaxTree } from '@codemirror/language';
import { type EditorState, type Extension, type Range, StateField } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, WidgetType } from '@codemirror/view';
import { contextMenu } from '$lib/stores/contextMenu.svelte';

import { createWidgetIcon, disconnectRemeasure, remeasureOnResize } from '../widget-height';
import { dragJustEnded, isMouseSelecting } from './mouse-freeze';

/**
 * The offer in the language picker. Deliberately a short, curated list, not
 * the ~150 languages CodeMirror can highlight — a context menu is a menu,
 * not a search index. The fence's info string still accepts anything when
 * typed (on the fence line or in raw mode); this is the fast path for the
 * common cases. Each entry is (info-string, display label).
 */
const LANGUAGE_CHOICES: [string, string][] = [
	['', 'Plain text'],
	['js', 'JavaScript'],
	['ts', 'TypeScript'],
	['python', 'Python'],
	['rust', 'Rust'],
	['go', 'Go'],
	['sh', 'Shell'],
	['sql', 'SQL'],
	['json', 'JSON'],
	['yaml', 'YAML'],
	['html', 'HTML'],
	['css', 'CSS'],
	['swift', 'Swift'],
	['java', 'Java'],
	['cpp', 'C++'],
	['md', 'Markdown'],
];

class CodeBlockHeaderWidget extends WidgetType {
	constructor(private language: string) {
		super();
	}

	toDOM(view: EditorView) {
		// A transparent wrapper carries the gap above the block as PADDING.
		// The header used to hold `margin-top: 0.5rem` itself — margin is
		// invisible to getBoundingClientRect, so the heightmap was 8px short
		// under every code block. See widget-height.ts, rule 1.
		const wrapper = document.createElement('div');
		wrapper.className = 'cm-code-header-wrap';

		const header = document.createElement('div');
		header.className = 'cm-code-header';
		wrapper.appendChild(header);

		// The language label is the picker. With the fences hidden in normal
		// use, this is how the info string gets set without raw mode.
		const lang = document.createElement('button');
		lang.type = 'button';
		lang.className = 'cm-code-language';
		lang.textContent = this.language || 'plain';
		lang.title = 'Change language';
		lang.addEventListener('click', (e) => {
			e.preventDefault();
			e.stopPropagation();
			contextMenu.show(
				{ x: e.clientX, y: e.clientY },
				LANGUAGE_CHOICES.map(([info, label]) => ({
					id: `lang-${info || 'plain'}`,
					label,
					action: () => {
						// Re-derive the fence line from DOM at click time, same as
						// the copy button — positions shift under remote edits.
						const pos = view.posAtDOM(header);
						const fenceLine = view.state.doc.lineAt(pos);
						const match = /^(`{3,})\s*\S*/.exec(fenceLine.text);
						if (!match) return;
						view.dispatch({
							changes: {
								from: fenceLine.from,
								to: fenceLine.from + match[0].length,
								insert: `${match[1]}${info}`,
							},
						});
					},
				})),
			);
		});
		header.appendChild(lang);

		const copyBtn = document.createElement('button');
		copyBtn.className = 'cm-code-copy';
		copyBtn.type = 'button';
		copyBtn.title = 'Copy code';

		// Box reserved before the SVG resolves — an unsized <iconify-icon>
		// measures as nothing and then grows the header. widget-height.ts, rule 2.
		const icon = createWidgetIcon('ri:file-copy-line', 14);
		copyBtn.appendChild(icon);

		copyBtn.addEventListener('click', (e) => {
			e.preventDefault();
			e.stopPropagation();
			// Re-derive code range from DOM at click time (handles remote edits shifting positions)
			const pos = view.posAtDOM(header);
			const doc = view.state.doc;
			const fenceLine = doc.lineAt(pos);
			const codeStart = fenceLine.to + 1;
			let codeEnd = doc.length;
			for (let ln = fenceLine.number + 1; ln <= doc.lines; ln++) {
				const line = doc.line(ln);
				if (line.text.startsWith('```')) {
					codeEnd = line.from;
					break;
				}
			}
			const code = doc.sliceString(codeStart, codeEnd);
			navigator.clipboard.writeText(code).then(() => {
				icon.setAttribute('icon', 'ri:check-line');
				setTimeout(() => icon.setAttribute('icon', 'ri:file-copy-line'), 1500);
			});
		});

		header.appendChild(copyBtn);

		// Belt and braces: the two rules above should make this never fire, but
		// a theme change or a font swap can still move the header, and a silent
		// heightmap drift is the one failure nobody notices until the caret
		// jumps to position 0.
		remeasureOnResize(view, wrapper);
		return wrapper;
	}

	destroy(dom: HTMLElement) {
		disconnectRemeasure(dom);
	}

	eq(other: CodeBlockHeaderWidget) {
		return other.language === this.language;
	}

	ignoreEvent() {
		return false;
	}
}

function buildCodeBlockDecorations(state: EditorState): DecorationSet {
	const builder: Range<Decoration>[] = [];

	// Fence-line reveal keys off the caret's line — see below.
	const cursorLine = state.doc.lineAt(state.selection.main.head);

	syntaxTree(state).iterate({
		enter(node) {
			if (node.name === 'FencedCode') {
				const { from, to } = node;

				const nodeEndLine = state.doc.lineAt(Math.min(Math.max(to - 1, from), state.doc.length)).number;

				// Extract language from the opening fence line
				const firstLine = state.doc.lineAt(from);
				const fenceMatch = firstLine.text.match(/^```(\w*)/);
				const language = fenceMatch?.[1] || '';

				// Add header widget before the code block
				builder.push(
					Decoration.widget({
						widget: new CodeBlockHeaderWidget(language),
						side: -1,
						block: true,
					}).range(from)
				);

				// Add line decorations for code block container (background)
				// Include fence lines so the background is seamless with the header
				const lastLine = to > from ? state.doc.lineAt(to - 1) : firstLine;
				const hasClosingFence = lastLine.text.startsWith('```') && lastLine.number !== firstLine.number;
				const closingFenceLine = hasClosingFence ? lastLine.number : nodeEndLine;
				const contentStartLine = firstLine.number + 1;

				for (let ln = firstLine.number; ln <= closingFenceLine; ln++) {
					if (ln > state.doc.lines) break;
					const line = state.doc.line(ln);
					let cls = 'cm-codeblock-line';
					if (ln === contentStartLine) cls += ' cm-codeblock-first';
					if (ln === closingFenceLine) cls += ' cm-codeblock-last';
					builder.push(
						Decoration.line({ attributes: { class: cls } }).range(line.from)
					);
				}

				// Per-fence-LINE reveal, not per-block. The old rule revealed both
				// fences the moment the caret entered the block's interior — a
				// two-line vertical shift on every entry, the last construct in
				// the editor that still flipped. A fence now shows only when the
				// caret is ON that fence's own line (reached deliberately, by
				// arrowing to the edge); editing inside the block moves nothing.
				// The header's language picker covers the common reason anyone
				// needed the opening fence visible at all.
				if (cursorLine.number !== firstLine.number) {
					builder.push(Decoration.replace({}).range(from, firstLine.to));
				}
				if (hasClosingFence && cursorLine.number !== lastLine.number) {
					builder.push(Decoration.replace({}).range(lastLine.from, lastLine.to));
				}
			}
		},
	});

	return Decoration.set(builder, true);
}

const codeBlockField = StateField.define<DecorationSet>({
	create(state) {
		return buildCodeBlockDecorations(state);
	},
	update(decos, tr) {
		// Fence reveal is a two-line VERTICAL shift, the most violent reveal
		// left in the editor — so it must never happen under a pressed mouse
		// button. Held while dragging, recomputed on release (mouse-freeze.ts).
		const rebuild =
			tr.docChanged ||
			(tr.selection && !isMouseSelecting(tr.state)) ||
			dragJustEnded(tr);
		if (rebuild) {
			return buildCodeBlockDecorations(tr.state);
		}
		return decos;
	},
	provide: (field) => EditorView.decorations.from(field),
});

export const codeBlocks: Extension = codeBlockField;
