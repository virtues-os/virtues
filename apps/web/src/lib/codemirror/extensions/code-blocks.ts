/**
 * Code Block Decorations
 *
 * Adds a header widget to fenced code blocks with language label and copy button.
 * The actual syntax highlighting is handled by CM6's built-in markdown + language-data.
 *
 * Uses StateField (not ViewPlugin) because block widgets require direct decoration
 * provision via EditorView.decorations facet.
 */

import { syntaxTree } from '@codemirror/language';
import { type EditorState, type Extension, type Range, StateField } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, WidgetType } from '@codemirror/view';

class CodeBlockHeaderWidget extends WidgetType {
	constructor(private language: string) {
		super();
	}

	toDOM(view: EditorView) {
		const header = document.createElement('div');
		header.className = 'cm-code-header';

		const lang = document.createElement('span');
		lang.className = 'cm-code-language';
		lang.textContent = this.language || 'plain';
		header.appendChild(lang);

		const copyBtn = document.createElement('button');
		copyBtn.className = 'cm-code-copy';
		copyBtn.type = 'button';
		copyBtn.title = 'Copy code';

		const icon = document.createElement('iconify-icon');
		icon.setAttribute('icon', 'ri:file-copy-line');
		icon.setAttribute('width', '14');
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
		return header;
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

	// Active-line exclusion
	const cursorHead = state.selection.main.head;
	const cursorLine = state.doc.lineAt(cursorHead);

	syntaxTree(state).iterate({
		enter(node) {
			if (node.name === 'FencedCode') {
				const { from, to } = node;

				// Check if cursor is inside this code block
				const nodeStartLine = state.doc.lineAt(from).number;
				const nodeEndLine = state.doc.lineAt(Math.min(Math.max(to - 1, from), state.doc.length)).number;
				const cursorInside =
					cursorLine.number >= nodeStartLine && cursorLine.number <= nodeEndLine;

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

				if (!cursorInside) {
					// Hide fence lines when cursor is outside the block
					builder.push(Decoration.replace({}).range(from, firstLine.to));
					if (hasClosingFence) {
						builder.push(Decoration.replace({}).range(lastLine.from, lastLine.to));
					}
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
		if (tr.docChanged || tr.selection) {
			return buildCodeBlockDecorations(tr.state);
		}
		return decos;
	},
	provide: (field) => EditorView.decorations.from(field),
});

export const codeBlocks: Extension = codeBlockField;
