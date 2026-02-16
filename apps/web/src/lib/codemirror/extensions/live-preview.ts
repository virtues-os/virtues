/**
 * Live Preview Extension
 *
 * Renders markdown with visual formatting via CodeMirror decorations.
 * Headings get serif fonts, bold/italic/code get visual treatment,
 * blockquotes get left border, etc.
 *
 * Works by walking the Lezer markdown syntax tree and applying decorations.
 * Links are handled separately by entity-links.ts (all links render as pills).
 */

import { syntaxTree } from '@codemirror/language';
import type { Extension, Range } from '@codemirror/state';
import { Decoration, type DecorationSet, type EditorView, ViewPlugin, type ViewUpdate, WidgetType } from '@codemirror/view';
/** Minimal node shape from Lezer syntax tree (avoids @lezer/common version mismatch) */
interface TreeNode { name: string; from: number; to: number; }

/**
 * Build decorations for the visible viewport
 */
function buildDecorations(view: EditorView): DecorationSet {
	const builder: Range<Decoration>[] = [];
	const doc = view.state.doc;
	const { from: vpFrom, to: vpTo } = view.viewport;

	// Active-line exclusion: don't decorate the line the cursor is on (Obsidian-style)
	const cursorHead = view.state.selection.main.head;
	const cursorLine = doc.lineAt(cursorHead);

	// Track ranges handled by syntax tree (for fallback detection)
	const hrLines = new Set<number>();
	const codeBlockRanges: { from: number; to: number }[] = [];

	syntaxTree(view.state).iterate({
		from: vpFrom,
		to: vpTo,
		enter(node) {
			const { name, from, to } = node;

			// Track code block ranges for fallback HR detection
			if (name === 'FencedCode') {
				codeBlockRanges.push({ from, to });
			}

			// Skip decorations that overlap the cursor line
			const nodeStartLine = doc.lineAt(from).number;
			const nodeEndLine = doc.lineAt(Math.min(to, doc.length)).number;
			const overlapsActiveLine =
				cursorLine.number >= nodeStartLine && cursorLine.number <= nodeEndLine;

			// --- Headings ---
			if (name.startsWith('ATXHeading') && !overlapsActiveLine) {
				const level = name.charAt(name.length - 1);
				const lineFrom = doc.lineAt(from).from;
				builder.push(
					Decoration.line({ attributes: { class: `cm-heading-${level}` } }).range(lineFrom)
				);
			}

			// Hide heading markers (# ## ### etc.) and trailing space when not on active line
			if (name === 'HeaderMark' && !overlapsActiveLine) {
				let hideEnd = to;
				if (hideEnd < doc.length && view.state.sliceDoc(hideEnd, hideEnd + 1) === ' ') {
					hideEnd += 1;
				}
				builder.push(Decoration.replace({}).range(from, hideEnd));
			}

			// --- Inline formatting (hide delimiters, style content) ---
			if (name === 'StrongEmphasis' && !overlapsActiveLine) {
				const inner = getInnerRange(view, node);
				if (inner) {
					builder.push(Decoration.mark({ class: 'cm-strong' }).range(inner.from, inner.to));
					if (inner.from > from) builder.push(Decoration.replace({}).range(from, inner.from));
					if (inner.to < to) builder.push(Decoration.replace({}).range(inner.to, to));
				}
			}

			if (name === 'Emphasis' && !overlapsActiveLine) {
				const inner = getInnerRange(view, node);
				if (inner) {
					builder.push(Decoration.mark({ class: 'cm-emphasis' }).range(inner.from, inner.to));
					if (inner.from > from) builder.push(Decoration.replace({}).range(from, inner.from));
					if (inner.to < to) builder.push(Decoration.replace({}).range(inner.to, to));
				}
			}

			if (name === 'Strikethrough' && !overlapsActiveLine) {
				const inner = getInnerRange(view, node);
				if (inner) {
					builder.push(Decoration.mark({ class: 'cm-strikethrough' }).range(inner.from, inner.to));
					if (inner.from > from) builder.push(Decoration.replace({}).range(from, inner.from));
					if (inner.to < to) builder.push(Decoration.replace({}).range(inner.to, to));
				}
			}

			if (name === 'InlineCode' && !overlapsActiveLine) {
				const inner = getInnerRange(view, node);
				if (inner) {
					builder.push(Decoration.mark({ class: 'cm-inline-code' }).range(inner.from, inner.to));
					if (inner.from > from) builder.push(Decoration.replace({}).range(from, inner.from));
					if (inner.to < to) builder.push(Decoration.replace({}).range(inner.to, to));
				}
			}

			// --- Blockquotes (left border always, hide > marker when not on line) ---
			if (name === 'Blockquote') {
				const startLine = doc.lineAt(from);
				const endLine = doc.lineAt(Math.min(to, doc.length));
				for (let lineNum = startLine.number; lineNum <= endLine.number; lineNum++) {
					const line = doc.line(lineNum);
					builder.push(
						Decoration.line({ attributes: { class: 'cm-blockquote-line' } }).range(line.from)
					);
				}
			}

			// Hide blockquote > markers and trailing space
			if (name === 'QuoteMark' && !overlapsActiveLine) {
				let hideEnd = to;
				if (hideEnd < doc.length && view.state.sliceDoc(hideEnd, hideEnd + 1) === ' ') {
					hideEnd += 1;
				}
				builder.push(Decoration.replace({}).range(from, hideEnd));
			}

			// --- Horizontal rules ---
			if (name === 'HorizontalRule') {
				hrLines.add(nodeStartLine);
				if (!overlapsActiveLine) {
					builder.push(Decoration.replace({}).range(from, to));
					builder.push(
						Decoration.widget({
							widget: new HorizontalRuleWidget(),
							side: 1,
						}).range(to)
					);
				}
			}

			// --- List line decorations (padding) ---
			if (name === 'BulletList' || name === 'OrderedList') {
				const startLine = doc.lineAt(from);
				const endLine = doc.lineAt(Math.min(to, doc.length));
				for (let lineNum = startLine.number; lineNum <= endLine.number; lineNum++) {
					const line = doc.line(lineNum);
					builder.push(
						Decoration.line({ attributes: { class: 'cm-list-line' } }).range(line.from)
					);
				}
			}

			// --- List markers ---
			if (name === 'ListMark' && !overlapsActiveLine) {
				const markerText = view.state.sliceDoc(from, to);
				if (markerText === '-' || markerText === '*' || markerText === '+') {
					// Bullet markers → replace with dot character
					builder.push(
						Decoration.replace({
							widget: new BulletDotWidget(),
						}).range(from, to)
					);
				} else {
					// Ordered list markers (1., 2.) — just dim them
					builder.push(
						Decoration.mark({ class: 'cm-list-marker' }).range(from, to)
					);
				}
			}
		},
	});

	// --- Fallback HR detection ---
	// Lezer only parses --- as HorizontalRule with a blank line above.
	// Without a blank line, it becomes a SetextHeading marker. Detect these
	// and render as HR anyway for better UX.
	const startLine = doc.lineAt(vpFrom).number;
	const endLine = doc.lineAt(Math.min(vpTo, doc.length)).number;

	for (let lineNum = startLine; lineNum <= endLine; lineNum++) {
		if (hrLines.has(lineNum)) continue;
		if (lineNum === cursorLine.number) continue;

		const line = doc.line(lineNum);
		if (!/^(-{3,}|\*{3,}|_{3,})\s*$/.test(line.text)) continue;

		// Skip if inside a code block
		const inCodeBlock = codeBlockRanges.some(r => line.from >= r.from && line.to <= r.to);
		if (inCodeBlock) continue;

		builder.push(Decoration.replace({}).range(line.from, line.to));
		builder.push(
			Decoration.widget({
				widget: new HorizontalRuleWidget(),
				side: 1,
			}).range(line.to)
		);
	}

	// --- Inline HTML underline: <u>text</u> ---
	const UNDERLINE_REGEX = /<u>(.*?)<\/u>/g;
	for (let lineNum = startLine; lineNum <= endLine; lineNum++) {
		if (lineNum === cursorLine.number) continue;
		const line = doc.line(lineNum);
		UNDERLINE_REGEX.lastIndex = 0;
		for (let m = UNDERLINE_REGEX.exec(line.text); m !== null; m = UNDERLINE_REGEX.exec(line.text)) {
			const from = line.from + m.index;
			const tagOpenEnd = from + 3;                    // after <u>
			const tagCloseStart = from + 3 + m[1].length;   // start of </u>
			const to = from + m[0].length;                   // after </u>
			builder.push(Decoration.replace({}).range(from, tagOpenEnd));
			builder.push(Decoration.mark({ class: 'cm-underline' }).range(tagOpenEnd, tagCloseStart));
			builder.push(Decoration.replace({}).range(tagCloseStart, to));
		}
	}

	// Decoration.set with sort=true handles ordering
	return Decoration.set(builder, true);
}

/**
 * Get the content range inside delimiter marks (e.g., content between ** and **)
 */
function getInnerRange(
	view: EditorView,
	node: TreeNode
): { from: number; to: number } | null {
	const text = view.state.sliceDoc(node.from, node.to);

	// Find delimiter length (1 for *, 2 for **, ~~ etc.)
	let delimLen = 0;
	if (text.startsWith('**') || text.startsWith('~~')) delimLen = 2;
	else if (text.startsWith('*') || text.startsWith('_') || text.startsWith('`')) delimLen = 1;
	else return null;

	const from = node.from + delimLen;
	const to = node.to - delimLen;
	if (from >= to) return null;
	return { from, to };
}

/**
 * Widget for rendering bullet list markers as a dot
 */
class BulletDotWidget extends WidgetType {
	toDOM() {
		const span = document.createElement('span');
		span.className = 'cm-bullet-dot';
		span.textContent = '•';
		return span;
	}

	eq() {
		return true;
	}
}

/**
 * Widget for rendering a horizontal rule
 */
class HorizontalRuleWidget extends WidgetType {
	toDOM() {
		const hr = document.createElement('hr');
		hr.className = 'cm-hr-widget';
		return hr;
	}

	eq() {
		return true;
	}
}

/**
 * The live preview plugin
 */
const livePreviewPlugin = ViewPlugin.fromClass(
	class {
		decorations: DecorationSet;

		constructor(view: EditorView) {
			this.decorations = buildDecorations(view);
		}

		update(update: ViewUpdate) {
			if (update.docChanged || update.viewportChanged || update.selectionSet) {
				this.decorations = buildDecorations(update.view);
			}
		}
	},
	{
		decorations: (v) => v.decorations,
	}
);

export const livePreview: Extension = livePreviewPlugin;
