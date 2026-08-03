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

import { inlineMarks, selectionTouches } from './inline-marks';
import { dragJustEnded, isMouseSelecting } from './mouse-freeze';

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
			// The line class is applied UNCONDITIONALLY. Dropping it on the active
			// line (as this once did) collapsed an h1 from 68px to 26px — family,
			// size, line-height and the 1.75rem padding all going at once — and
			// shoved every line below it up by 42px on a single click. That was the
			// single largest source of reflow in the editor. The markers may hide;
			// the type never moves.
			if (name.startsWith('ATXHeading')) {
				const level = name.charAt(name.length - 1);
				const lineFrom = doc.lineAt(from).from;
				builder.push(
					Decoration.line({ attributes: { class: `cm-heading-${level}` } }).range(lineFrom)
				);
			}

			// Hide the heading markers (# ## ###) and their trailing space. With the
			// caret on the line the opening marker comes back as a margin-hung
			// widget — absolutely positioned, so it is legible and editable but
			// occupies no inline width and cannot push the text sideways.
			if (name === 'HeaderMark') {
				let hideEnd = to;
				if (hideEnd < doc.length && view.state.sliceDoc(hideEnd, hideEnd + 1) === ' ') {
					hideEnd += 1;
				}
				// Only the OPENING marker gets the widget; a closing `#` in `# Foo #`
				// or a setext underline just hides.
				const markLine = doc.lineAt(from);
				const isOpeningMark = from === markLine.from && markLine.number === nodeStartLine;
				const onActiveLine = markLine.number === cursorLine.number;
				const deco =
					isOpeningMark && onActiveLine
						? Decoration.replace({
								widget: new HeadingMarkWidget(view.state.sliceDoc(from, to)),
							})
						: Decoration.replace({});
				builder.push(deco.range(from, hideEnd));
			}

			// Inline formatting is handled once, after this walk, from
			// inline-marks.ts — the same description the atomic ranges use.

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
			// Always the rule, never the `---`. Swapping a 1px line for three
			// characters of text and back is the same flicker as everything else
			// here; the line is still selectable and deletable as a line.
			if (name === 'HorizontalRule') {
				hrLines.add(nodeStartLine);
				builder.push(Decoration.replace({}).range(from, to));
				builder.push(
					Decoration.widget({
						widget: new HorizontalRuleWidget(),
						side: 1,
					}).range(to)
				);
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
				// Task items (`- [ ]` / `- [x]`) are rendered entirely by the
				// checkboxes extension (which replaces `- [ ] ` with a checkbox).
				// Skip the bullet dot here so we don't get BOTH a • and a checkbox.
				const isTaskItem = /^\s*[-*+]\s+\[[ xX]\]/.test(doc.lineAt(from).text);
				if (isTaskItem) {
					// handled by checkboxes.ts — no marker decoration
				} else if (markerText === '-' || markerText === '*' || markerText === '+') {
					// Bullet markers → a dot; glyph varies by nesting depth.
					const bulletLine = doc.lineAt(from);
					const indent = bulletLine.text.length - bulletLine.text.trimStart().length;
					const depth = Math.floor(indent / 2);
					builder.push(
						Decoration.replace({
							widget: new BulletDotWidget(BULLET_GLYPHS[depth % BULLET_GLYPHS.length]),
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

	// --- Inline marks: bold, italic, strike, code, highlight, underline ---
	// Reveal-on-touch: the styling is always applied, and the delimiters of THE
	// construct the selection touches appear in place, dimmed — every other
	// construct keeps its delimiters hidden. Only ever a horizontal shift, only
	// ever for the one construct being edited. Positions come from
	// inline-marks.ts so this stays the single definition of a mark's extent.
	for (const mark of inlineMarks(view.state, vpFrom, vpTo)) {
		builder.push(
			Decoration.mark({ class: mark.cls }).range(mark.openTo, mark.closeFrom)
		);
		if (selectionTouches(view.state, mark)) {
			builder.push(
				Decoration.mark({ class: 'cm-formatting-mark' }).range(mark.openFrom, mark.openTo)
			);
			builder.push(
				Decoration.mark({ class: 'cm-formatting-mark' }).range(mark.closeFrom, mark.closeTo)
			);
		} else {
			builder.push(Decoration.replace({}).range(mark.openFrom, mark.openTo));
			builder.push(Decoration.replace({}).range(mark.closeFrom, mark.closeTo));
		}
	}

	// Decoration.set with sort=true handles ordering
	return Decoration.set(builder, true);
}

/**
 * Widget for rendering bullet list markers as a dot. The glyph varies by nesting
 * depth (• → ◦ → ▪) so nested bullet levels read as an outline.
 */
const BULLET_GLYPHS = ['•', '◦', '▪'];

class BulletDotWidget extends WidgetType {
	constructor(private glyph: string = '•') {
		super();
	}

	toDOM() {
		const span = document.createElement('span');
		span.className = 'cm-bullet-dot';
		span.textContent = this.glyph;
		return span;
	}

	eq(other: BulletDotWidget) {
		return other.glyph === this.glyph;
	}
}

/**
 * The `#` markers, hung in the left margin while the caret is on the line.
 *
 * `position: absolute` takes it out of flow, so the marker is visible without
 * occupying any inline width — the heading text does not shift when the caret
 * arrives. That is the whole trick: reveal the syntax, never the reflow.
 */
class HeadingMarkWidget extends WidgetType {
	constructor(private marks: string) {
		super();
	}

	toDOM() {
		const span = document.createElement('span');
		span.className = 'cm-heading-mark';
		span.textContent = this.marks;
		return span;
	}

	eq(other: HeadingMarkWidget) {
		return other.marks === this.marks;
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
			// Selection-driven rebuilds are held while the mouse is down (see
			// mouse-freeze.ts) so a reveal cannot shift text under a drag in
			// progress; the rebuild fires on release instead.
			const rebuild =
				update.docChanged ||
				update.viewportChanged ||
				(update.selectionSet && !isMouseSelecting(update.state)) ||
				dragJustEnded(update);
			if (rebuild) {
				this.decorations = buildDecorations(update.view);
			}
		}
	},
	{
		decorations: (v) => v.decorations,
	}
);

export const livePreview: Extension = livePreviewPlugin;
