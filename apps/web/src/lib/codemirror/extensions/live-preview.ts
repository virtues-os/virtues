/**
 * Live Preview Extension
 *
 * Renders markdown with visual formatting via CodeMirror decorations.
 * Headings get serif fonts, bold/italic/code get visual treatment,
 * blockquotes get left border, lists get a hanging indent, etc.
 *
 * Works by walking the Lezer markdown syntax tree and applying decorations.
 * Links are handled separately by entity-links.ts (all links render as pills).
 *
 * Every reveal here — heading `#`, quote `>`, list marker — requires the
 * editor to HAVE FOCUS, not just a caret on the line. The selection survives
 * blur, so without the gate a blurred editor, an inactive split pane, or the
 * read-only view (whose selection sits at 0 forever) holds raw syntax on the
 * caret line. The plugin rebuilds on `focusChanged` so the reveal follows
 * focus in both directions.
 */

import { syntaxTree } from '@codemirror/language';
import type { Extension, Range } from '@codemirror/state';
import { Decoration, type DecorationSet, type EditorView, ViewPlugin, type ViewUpdate, WidgetType } from '@codemirror/view';

import { inlineMarks, selectionTouches } from './inline-marks';
import { dragJustEnded, isMouseSelecting } from './mouse-freeze';

/** Deepest list level with its own indent class; anything deeper shares it. */
const MAX_LIST_DEPTH = 5;

/**
 * A task item as checkboxes.ts recognizes it — the SAME test, so the two
 * extensions agree line by line on who draws the marker. (`* [ ]` and `+ [ ]`
 * parse as tasks too, but the checkbox extension only takes `-`; those keep
 * their bullet and show the brackets as text.)
 */
const TASK_ITEM = /^\s*-\s+\[[ xX]\]/;

/** What the list pass knows about one physical line of a list. */
interface ListLine {
	/** Nesting depth, 0 for a top-level item. */
	depth: number;
	/** Column where the item's content starts: after the marker and one space. */
	contentCol: number;
	/** The ListMark on this line, if this is an item's first line. */
	mark: { from: number; to: number } | null;
}

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

	// No reveal without focus — see the module comment. One gate for every
	// construct below, the same one the inline-marks pass uses.
	const canReveal = view.hasFocus;

	// Track ranges handled by syntax tree (for fallback detection)
	const hrLines = new Set<number>();
	const codeBlockRanges: { from: number; to: number }[] = [];

	// Lists are collected during the walk and emitted after it: a line inside
	// a nested item is visited by every enclosing item, and the deepest one
	// owns it. Parent-first traversal makes "last writer wins" the same as
	// "deepest wins", but the max is kept explicit.
	const listLines = new Map<number, ListLine>();
	let itemDepth = 0;

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
			// caret on the line of a focused editor the opening marker comes back
			// as a margin-hung widget — absolutely positioned, so it is legible and
			// editable but occupies no inline width and cannot push the text
			// sideways.
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
					canReveal && isOpeningMark && onActiveLine
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

			// Hide blockquote > markers and trailing space, except on the caret
			// line of a focused editor.
			if (name === 'QuoteMark' && !(canReveal && overlapsActiveLine)) {
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

			// --- Lists ---
			// Depth comes from the tree, not from counting spaces: CommonMark
			// allows up to three leading spaces on a top-level item, and an
			// ordered item's children sit at the marker's width, not at two.
			//
			// A list node claims every line it spans — including the blank lines
			// of a loose list, which no item owns — at the depth of the items
			// around it, so the caret on such a line keeps the indent.
			if (name === 'BulletList' || name === 'OrderedList') {
				const depth = Math.min(itemDepth, MAX_LIST_DEPTH);
				const startLine = doc.lineAt(from);
				const endLine = doc.lineAt(Math.min(to, doc.length));
				for (let lineNum = startLine.number; lineNum <= endLine.number; lineNum++) {
					const known = listLines.get(lineNum);
					if (known && known.depth >= depth) continue;
					listLines.set(lineNum, { depth, contentCol: 0, mark: null });
				}
			}

			// An item claims its own lines: the marker line, and every
			// continuation line (a wrapped paragraph, a second paragraph in a
			// loose item) up to where the item ends. Nested items come later in
			// the walk and re-claim theirs at the deeper level.
			if (name === 'ListItem') {
				itemDepth++;
				const depth = Math.min(itemDepth - 1, MAX_LIST_DEPTH);
				const firstLine = doc.lineAt(from);
				const lastLine = doc.lineAt(Math.min(to, doc.length));
				const first = node.node.firstChild;
				const mark = first && first.name === 'ListMark' ? { from: first.from, to: first.to } : null;
				let contentCol = 0;
				if (mark) {
					contentCol = mark.to - firstLine.from;
					if (mark.to < doc.length && view.state.sliceDoc(mark.to, mark.to + 1) === ' ') {
						contentCol += 1;
					}
				}
				for (let lineNum = firstLine.number; lineNum <= lastLine.number; lineNum++) {
					const known = listLines.get(lineNum);
					if (known && known.depth > depth) continue;
					listLines.set(lineNum, {
						depth,
						contentCol,
						mark: lineNum === firstLine.number ? mark : null,
					});
				}
			}
		},
		leave(node) {
			if (node.name === 'ListItem') itemDepth--;
		},
	});

	// --- List lines: the hanging indent, and one widget per marker ---
	// The indent is a per-depth CSS class (margin-left; see theme.css for why
	// it can never be padding). The marker, the space after it and the
	// structural leading spaces are replaced by ONE widget, absolutely
	// positioned in the alcove the margin leaves — the same hang as the
	// heading `#` — so wrapped rows align under the first row's text, and
	// the caret line's raw marker (`-`, `1.`) shows in the same widget
	// without moving anything.
	for (const [lineNum, info] of listLines) {
		const line = doc.line(lineNum);
		builder.push(
			Decoration.line({
				attributes: { class: `cm-list-line cm-list-depth-${info.depth}` },
			}).range(line.from)
		);

		if (info.mark) {
			if (TASK_ITEM.test(line.text)) {
				// checkboxes.ts replaces `- [ ] ` with the checkbox; this pass only
				// swallows the indentation in front of it, so the two decorations
				// sit side by side and no line gets both a bullet and a box.
				if (info.mark.from > line.from) {
					builder.push(Decoration.replace({}).range(line.from, info.mark.from));
				}
				continue;
			}
			const markerText = view.state.sliceDoc(info.mark.from, info.mark.to);
			const isBullet = markerText === '-' || markerText === '*' || markerText === '+';
			const onCaretLine = canReveal && lineNum === cursorLine.number;
			let widget: ListMarkerWidget;
			if (onCaretLine) {
				widget = new ListMarkerWidget(markerText, 'cm-list-marker cm-list-marker-raw');
			} else if (isBullet) {
				widget = new ListMarkerWidget(
					BULLET_GLYPHS[info.depth % BULLET_GLYPHS.length],
					'cm-list-marker cm-list-marker-bullet'
				);
			} else {
				widget = new ListMarkerWidget(markerText, 'cm-list-marker cm-list-marker-ordered');
			}
			builder.push(
				Decoration.replace({ widget }).range(line.from, line.from + info.contentCol)
			);
			continue;
		}

		// Continuation line: hide the structural indent (up to the item's
		// content column) so the text lands on the margin, not one indent past
		// it. Whatever is indented further than that is the writer's own.
		const firstNonSpace = line.text.search(/\S/);
		const leading = firstNonSpace < 0 ? line.length : firstNonSpace;
		const hide = Math.min(leading, info.contentCol);
		if (hide > 0) {
			builder.push(Decoration.replace({}).range(line.from, line.from + hide));
		}
	}

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
	// With focus, the pass also returns OPEN marks — `**foo |` with no closer
	// yet — so the styling holds while a construct is still being typed instead
	// of blinking off at every space. An open mark's closer is empty
	// (closeFrom === closeTo); Decoration.mark throws on an empty range.
	for (const mark of inlineMarks(view.state, vpFrom, vpTo, { focused: canReveal })) {
		const hasCloser = mark.closeFrom < mark.closeTo;
		builder.push(
			Decoration.mark({ class: mark.cls }).range(mark.openTo, mark.closeFrom)
		);
		if (canReveal && selectionTouches(view.state, mark)) {
			builder.push(
				Decoration.mark({ class: 'cm-formatting-mark' }).range(mark.openFrom, mark.openTo)
			);
			if (hasCloser) {
				builder.push(
					Decoration.mark({ class: 'cm-formatting-mark' }).range(mark.closeFrom, mark.closeTo)
				);
			}
		} else {
			builder.push(Decoration.replace({}).range(mark.openFrom, mark.openTo));
			if (hasCloser) builder.push(Decoration.replace({}).range(mark.closeFrom, mark.closeTo));
		}
	}

	// Decoration.set with sort=true handles ordering
	return Decoration.set(builder, true);
}

/**
 * Bullet glyphs by nesting depth (• → ◦ → ▪) so nested levels read as an
 * outline.
 */
const BULLET_GLYPHS = ['•', '◦', '▪'];

/**
 * The list marker, hung in the alcove to the left of the item's text.
 *
 * One widget stands in for the marker, the space after it, and the structural
 * indentation before it — on every item line, active or not. `position:
 * absolute` (theme.css) takes it out of flow, so the text starts at the
 * margin and wrapped rows return under it, and swapping the glyph for the raw
 * marker on the caret line changes nothing about where the text sits.
 */
class ListMarkerWidget extends WidgetType {
	constructor(
		private text: string,
		private cls: string
	) {
		super();
	}

	toDOM() {
		const span = document.createElement('span');
		span.className = this.cls;
		span.textContent = this.text;
		return span;
	}

	eq(other: ListMarkerWidget) {
		return other.text === this.text && other.cls === this.cls;
	}

	ignoreEvent() {
		return false;
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
				update.focusChanged ||
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
