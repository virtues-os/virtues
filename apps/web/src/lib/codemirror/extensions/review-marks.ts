/**
 * Review marks — suggestions the writer accepts, instead of edits already made
 *
 * The problem this exists to solve: when the assistant edits a page it writes
 * straight into the buffer, so a bad generation is damage the writer undoes by
 * hand, and a good one is indistinguishable from their own prose. Neither is
 * reviewable. A suggestion should be a PROPOSAL — visible, attributable, and
 * refusable — until someone takes it.
 *
 * The proposals live in the markdown itself, as CriticMarkup:
 *
 *     {++inserted text++}     a proposed addition
 *     {--deleted text--}      a proposed removal (the text stays until accepted)
 *     {>>a note<<}            a remark, resolved rather than applied
 *
 * IN THE TEXT is the load-bearing choice. Page content reaches the assistant,
 * the read path, share links, export, and applets as markdown — anything stored
 * beside the document (a CRDT attribute, a positions table) is invisible to all
 * of them, so an un-accepted deletion would read as ordinary prose and the
 * assistant would faithfully re-process text the writer had already cut. Marks
 * in the body survive every one of those hops, and other markdown tools show
 * them as readable text rather than mangling them.
 *
 * NOT SUPPORTED: CriticMarkup's substitution form, `{~~old~>new~~}`. Its `~~`
 * is GFM strikethrough, so the Lezer parser claims the same characters and
 * live-preview would strike and un-delimit them underneath these decorations.
 * A substitution is written as an adjacent `{--old--}{++new++}` instead, which
 * renders as the same before/after pair with no parser to fight.
 *
 * Delimiters follow the reveal-on-touch rule the rest of the editor uses: the
 * braces hide so the span reads as a diff, and reappear dimmed when the
 * selection reaches them, because a hidden character the caret can still land
 * on is the bug that doctrine exists to prevent (see inline-marks.ts).
 */

import type { EditorState, Extension, Range, Text } from '@codemirror/state';
import { StateField } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView } from '@codemirror/view';
import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';

import { collectCodeRanges, inCode } from './code-context';
import { selectionTouches } from './inline-marks';
import { dragJustEnded, isMouseSelecting } from './mouse-freeze';

/** Every opening and closing delimiter is exactly three characters. */
const DELIMITER_LENGTH = 3;

/** One pass, alternation rather than three scans, so matches cannot overlap. */
const REVIEW_RE = /\{\+\+([\s\S]*?)\+\+\}|\{--([\s\S]*?)--\}|\{>>([\s\S]*?)<<\}/g;

// ── Escaping ────────────────────────────────────────────────────────────────
//
// Two jobs, and they are different problems.
//
// WRITING a proposal: the text being wrapped is arbitrary — it may itself
// contain `--}`, which would end the deletion span in the middle of itself and
// leave markup that parses somewhere nobody chose. So a body is escaped on the
// way in and unescaped on the way out, and accept/reject therefore restores
// the original byte for byte.
//
// READING prose: someone may simply type `{--` while writing about this very
// syntax. The markdown answer applies — a backslash makes it literal — so the
// scanner skips an escaped opener. What it deliberately does NOT do is rewrite
// what the writer types: silently inserting backslashes into someone's document
// as they type is a worse surprise than the mark they can undo.

/** Sequences that would terminate or reopen a span from inside one. */
const ESCAPE_RULES: ReadonlyArray<readonly [RegExp, string]> = [
	// Backslash first, or the escapes added below would be escaped in turn.
	[/\\/g, '\\\\'],
	[/\{--/g, '\\{--'],
	[/\{\+\+/g, '\\{++'],
	[/\{>>/g, '\\{>>'],
	[/--\}/g, '--\\}'],
	[/\+\+\}/g, '++\\}'],
	[/<<\}/g, '<<\\}'],
];

/** Make arbitrary text safe to place inside a review span. */
export function escapeReviewBody(text: string): string {
	let out = text;
	for (const [pattern, replacement] of ESCAPE_RULES) {
		out = out.replace(pattern, replacement);
	}
	return out;
}

/**
 * The inverse. One left-to-right pass, so `\\` is consumed as an escaped
 * backslash before its second character can be read as escaping what follows.
 */
export function unescapeReviewBody(text: string): string {
	return text.replace(/\\([\\{}])/g, '$1');
}

/**
 * Is the character at `index` escaped?
 *
 * Counted, not looked-behind: `\{--` is a literal brace, but `\\{--` is an
 * escaped backslash followed by a real opener. Only an ODD run of backslashes
 * escapes what comes after it.
 */
function isEscaped(text: string, index: number): boolean {
	let slashes = 0;
	for (let i = index - 1; i >= 0 && text[i] === '\\'; i--) slashes++;
	return slashes % 2 === 1;
}

/**
 * A streaming escaper, for text that arrives in chunks.
 *
 * A dangerous sequence can straddle a chunk boundary — `++` ending one chunk
 * and `}` opening the next would slip through an escaper that only ever saw
 * them apart, and close the span early. So up to two trailing characters that
 * could begin such a sequence are held back and prepended to the next chunk.
 * `flush` releases the remainder, which by construction is a strict prefix and
 * so needs no escaping of its own.
 */
const STRADDLE_PREFIXES = new Set(['{', '-', '+', '<', '{-', '{+', '{>', '--', '++', '<<']);

export function createReviewEscaper() {
	let held = '';
	return {
		push(chunk: string): string {
			const text = held + chunk;
			let keep = 0;
			if (STRADDLE_PREFIXES.has(text.slice(-2))) keep = 2;
			else if (STRADDLE_PREFIXES.has(text.slice(-1))) keep = 1;
			held = keep === 0 ? '' : text.slice(text.length - keep);
			return escapeReviewBody(text.slice(0, text.length - keep));
		},
		flush(): string {
			const rest = held;
			held = '';
			return escapeReviewBody(rest);
		},
	};
}

export type ReviewKind = 'insertion' | 'deletion' | 'comment';

export interface ReviewSpan {
	kind: ReviewKind;
	/** Offset of the opening `{`. */
	from: number;
	/** Offset just past the closing `}`. */
	to: number;
	/** Text between the delimiters. */
	body: string;
	/** What replaces the whole span when the suggestion is taken. */
	accepted: string;
	/** What replaces it when the suggestion is refused. */
	rejected: string;
}

/** A comment is resolved, never applied — it proposes no text either way. */
export function isEdit(span: ReviewSpan): boolean {
	return span.kind !== 'comment';
}

// Selection changes rebuild the decorations (that is what reveal-on-touch
// means), and each rebuild would otherwise re-scan the whole document. Text is
// immutable in CodeMirror, so identity is a sound cache key: same Text, same
// spans. Two editors open at once simply take turns.
let scanCache: { doc: Text; spans: ReviewSpan[] } | null = null;

/** Every review span in the document, in document order. */
export function reviewSpans(state: EditorState): ReviewSpan[] {
	if (scanCache && scanCache.doc === state.doc) return scanCache.spans;

	const text = state.doc.toString();
	// A fenced example of the syntax is documentation, not a suggestion.
	const code = collectCodeRanges(state, 0, state.doc.length);
	const spans: ReviewSpan[] = [];

	REVIEW_RE.lastIndex = 0;
	let match: RegExpExecArray | null = REVIEW_RE.exec(text);
	while (match !== null) {
		const from = match.index;
		const to = from + match[0].length;

		if (isEscaped(text, from)) {
			// `\{--` is prose about the syntax, not a suggestion. Resume just past
			// the opener rather than past the whole match — the characters this
			// match swallowed may still hold a real span further along.
			REVIEW_RE.lastIndex = from + 1;
		} else if (!inCode(code, from, to)) {
			// `accepted` / `rejected` are what the document BECOMES, so they carry
			// the unescaped text; `body` stays as written for rendering.
			if (match[1] !== undefined) {
				const body = match[1];
				spans.push({ kind: 'insertion', from, to, body, accepted: unescapeReviewBody(body), rejected: '' });
			} else if (match[2] !== undefined) {
				const body = match[2];
				spans.push({ kind: 'deletion', from, to, body, accepted: '', rejected: unescapeReviewBody(body) });
			} else {
				spans.push({ kind: 'comment', from, to, body: match[3] ?? '', accepted: '', rejected: '' });
			}
		}

		match = REVIEW_RE.exec(text);
	}

	scanCache = { doc: state.doc, spans };
	return spans;
}

/** The span containing `pos`, if any. Boundaries count — they are aimable. */
export function reviewSpanAt(state: EditorState, pos: number): ReviewSpan | null {
	for (const span of reviewSpans(state)) {
		if (pos >= span.from && pos <= span.to) return span;
	}
	return null;
}

// ── Resolving ───────────────────────────────────────────────────────────────

export type ReviewAction = 'accept' | 'reject';

function replacement(span: ReviewSpan, action: ReviewAction): string {
	return action === 'accept' ? span.accepted : span.rejected;
}

/** Take or refuse one suggestion. Either way the markup itself disappears. */
export function resolveReview(view: EditorView, span: ReviewSpan, action: ReviewAction) {
	view.dispatch({
		changes: { from: span.from, to: span.to, insert: replacement(span, action) },
	});
}

/**
 * Take or refuse every proposed EDIT in one transaction — one undo step, and
 * one Yjs update rather than a burst of them.
 *
 * Comments are deliberately untouched: "accept all changes" is a statement
 * about changes, and silently deleting someone's remarks alongside them would
 * be a second, unasked-for decision.
 */
export function resolveAllReviews(view: EditorView, action: ReviewAction) {
	const edits = reviewSpans(view.state).filter(isEdit);
	if (edits.length === 0) return;
	view.dispatch({
		changes: edits.map((span) => ({
			from: span.from,
			to: span.to,
			insert: replacement(span, action),
		})),
	});
}

// ── Rendering ───────────────────────────────────────────────────────────────

const CONTENT_CLASS: Record<ReviewKind, string> = {
	insertion: 'cm-review-ins',
	deletion: 'cm-review-del',
	comment: 'cm-review-note',
};

/**
 * Offsets, within a body, of the backslashes that are ESCAPE MARKERS rather
 * than content — the ones `unescapeReviewBody` would consume.
 *
 * The escaped character is skipped after each hit, so in `\\` only the first
 * backslash is syntax; the second is a real backslash the writer will get back.
 * A backslash before anything else (`\n`, `\d`) is ordinary text and is left
 * alone, matching what unescaping actually does.
 */
function escapeOffsets(body: string): number[] {
	const offsets: number[] = [];
	for (let i = 0; i < body.length; i++) {
		if (body[i] !== '\\') continue;
		const next = body[i + 1];
		if (next === '\\' || next === '{' || next === '}') {
			offsets.push(i);
			i++;
		}
	}
	return offsets;
}

function buildReviewDecorations(state: EditorState): DecorationSet {
	const ranges: Range<Decoration>[] = [];

	for (const span of reviewSpans(state)) {
		const openTo = span.from + DELIMITER_LENGTH;
		const closeFrom = span.to - DELIMITER_LENGTH;

		// Reveal-on-touch, using the same reach test as every other construct:
		// adjacency counts, so a caret parked just outside still exposes the
		// characters it is one keystroke from editing.
		const delimiter = selectionTouches(state, { openFrom: span.from, closeTo: span.to })
			? Decoration.mark({ class: 'cm-formatting-mark' })
			: Decoration.replace({});

		ranges.push(delimiter.range(span.from, openTo));

		if (closeFrom > openTo) {
			ranges.push(Decoration.mark({ class: CONTENT_CLASS[span.kind] }).range(openTo, closeFrom));

			// Escape backslashes are syntax, so they take the same quieting the
			// delimiters get — DIMMED, never hidden. Replacing them would put a
			// caret position behind a character that is not on screen, which is
			// the exact dishonesty reveal-on-touch exists to prevent, and here it
			// would be permanent rather than momentary.
			for (const offset of escapeOffsets(span.body)) {
				const at = openTo + offset;
				ranges.push(Decoration.mark({ class: 'cm-formatting-mark' }).range(at, at + 1));
			}
		}

		ranges.push(delimiter.range(closeFrom, span.to));
	}

	return Decoration.set(ranges, true);
}

const reviewField = StateField.define<DecorationSet>({
	create(state) {
		return buildReviewDecorations(state);
	},
	update(decorations, tr) {
		// Revealing a delimiter widens the line, so — like every other reveal —
		// it is held while the mouse is down and recomputed on release, or the
		// text shifts under a pointer that is mid-drag (mouse-freeze.ts).
		const rebuild =
			tr.docChanged || (tr.selection && !isMouseSelecting(tr.state)) || dragJustEnded(tr);
		return rebuild ? buildReviewDecorations(tr.state) : decorations;
	},
	provide: (field) => EditorView.decorations.from(field),
});

// ── The accept/reject affordance ────────────────────────────────────────────

const ACCEPT_LABEL: Record<ReviewKind, string> = {
	insertion: 'Accept — keep this text',
	deletion: 'Accept — remove this text',
	comment: 'Resolve comment',
};

const REJECT_LABEL: Record<ReviewKind, string> = {
	insertion: 'Reject — discard this text',
	deletion: 'Reject — keep this text',
	comment: '',
};

function menuItems(view: EditorView, span: ReviewSpan): ContextMenuItem[] {
	const items: ContextMenuItem[] = [
		{
			id: 'review-accept',
			label: ACCEPT_LABEL[span.kind],
			icon: 'ri:check-line',
			action: () => resolveReview(view, span, 'accept'),
		},
	];

	// A comment carries no proposed text, so refusing it and resolving it are
	// the same act; offering both would be a distinction without a difference.
	if (span.kind !== 'comment') {
		items.push({
			id: 'review-reject',
			label: REJECT_LABEL[span.kind],
			icon: 'ri:close-line',
			action: () => resolveReview(view, span, 'reject'),
		});
	}

	if (reviewSpans(view.state).filter(isEdit).length > 1) {
		items.push(
			{
				id: 'review-accept-all',
				label: 'Accept all changes',
				icon: 'ri:check-double-line',
				action: () => resolveAllReviews(view, 'accept'),
			},
			{
				id: 'review-reject-all',
				label: 'Reject all changes',
				icon: 'ri:delete-back-2-line',
				action: () => resolveAllReviews(view, 'reject'),
			}
		);
	}

	return items;
}

const reviewContextMenu = EditorView.domEventHandlers({
	contextmenu(event, view) {
		const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
		if (pos === null) return false;

		const span = reviewSpanAt(view.state, pos);
		if (!span) return false;

		event.preventDefault();
		event.stopPropagation();
		contextMenu.show({ x: event.clientX, y: event.clientY }, menuItems(view, span));
		return true;
	},
});

export const reviewMarks: Extension = [reviewField, reviewContextMenu];
