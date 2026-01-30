/**
 * CriticMarkup CodeMirror Extension
 *
 * Provides inline rendering of CriticMarkup syntax for AI-proposed edits:
 * - {++addition++} - highlighted in green with Accept/Reject buttons
 * - {--deletion--} - highlighted in red with Accept/Reject buttons
 *
 * Users can accept or reject each change inline, and the markers are
 * automatically cleaned up based on the action.
 *
 * Features:
 * - Code block protection: markers inside ``` fences are ignored
 * - Escape handling: \{++ and \{-- are treated as literal text
 * - Keyboard shortcuts: Cmd+Enter to accept, Cmd+Backspace to reject (when cursor in marker)
 */

import {
	EditorView,
	Decoration,
	type DecorationSet,
	ViewPlugin,
	type ViewUpdate,
	WidgetType,
	keymap
} from '@codemirror/view';
import { RangeSetBuilder } from '@codemirror/state';

// Regex patterns for CriticMarkup (non-escaped only)
// Uses negative lookbehind to skip escaped markers
// Uses [\s\S] instead of [^] for cross-line matching
const ADDITION_PATTERN = /(?<!\\)\{\+\+([\s\S]*?)\+\+\}/g;
const DELETION_PATTERN = /(?<!\\)\{--([\s\S]*?)--\}/g;

interface CriticMarkupMatch {
	type: 'addition' | 'deletion';
	from: number;
	to: number;
	content: string;
}

/**
 * Configuration for CriticMarkup extension
 */
export interface CriticMarkupConfig {
	/** Called when user accepts a change */
	onAccept?: (type: 'addition' | 'deletion', content: string, from: number, to: number) => void;
	/** Called when user rejects a change */
	onReject?: (type: 'addition' | 'deletion', content: string, from: number, to: number) => void;
}

/**
 * Widget for Accept/Reject buttons
 */
class CriticMarkupButtonWidget extends WidgetType {
	constructor(
		readonly matchType: 'addition' | 'deletion',
		readonly content: string,
		readonly from: number,
		readonly to: number,
		readonly config: CriticMarkupConfig
	) {
		super();
	}

	toDOM(view: EditorView): HTMLElement {
		const wrapper = document.createElement('span');
		wrapper.className = 'cm-critic-buttons';

		const acceptBtn = document.createElement('button');
		acceptBtn.className = 'cm-critic-btn cm-critic-accept';
		acceptBtn.innerHTML = '✓';
		acceptBtn.title = 'Accept';
		acceptBtn.onclick = (e) => {
			e.preventDefault();
			e.stopPropagation();
			this.handleAccept(view);
		};

		const rejectBtn = document.createElement('button');
		rejectBtn.className = 'cm-critic-btn cm-critic-reject';
		rejectBtn.innerHTML = '✗';
		rejectBtn.title = 'Reject';
		rejectBtn.onclick = (e) => {
			e.preventDefault();
			e.stopPropagation();
			this.handleReject(view);
		};

		wrapper.appendChild(acceptBtn);
		wrapper.appendChild(rejectBtn);

		return wrapper;
	}

	handleAccept(view: EditorView) {
		// Accept: keep the content, remove markers
		// {++text++} → text (keep addition)
		// {--text--} → "" (remove deletion)
		const replacement = this.matchType === 'addition' ? this.content : '';

		view.dispatch({
			changes: { from: this.from, to: this.to, insert: replacement }
		});

		this.config.onAccept?.(this.matchType, this.content, this.from, this.to);
	}

	handleReject(view: EditorView) {
		// Reject: remove the content or restore it
		// {++text++} → "" (reject addition)
		// {--text--} → text (reject deletion, keep text)
		const replacement = this.matchType === 'deletion' ? this.content : '';

		view.dispatch({
			changes: { from: this.from, to: this.to, insert: replacement }
		});

		this.config.onReject?.(this.matchType, this.content, this.from, this.to);
	}

	eq(other: WidgetType): boolean {
		return (
			other instanceof CriticMarkupButtonWidget &&
			other.matchType === this.matchType &&
			other.from === this.from &&
			other.to === this.to
		);
	}

	ignoreEvent(): boolean {
		return false;
	}
}

/**
 * Find all CriticMarkup matches in the document
 */
function findCriticMarkupMatches(doc: string): CriticMarkupMatch[] {
	const matches: CriticMarkupMatch[] = [];

	// Find additions {++...++}
	ADDITION_PATTERN.lastIndex = 0;
	let additionMatch: RegExpExecArray | null = ADDITION_PATTERN.exec(doc);
	while (additionMatch !== null) {
		matches.push({
			type: 'addition',
			from: additionMatch.index,
			to: additionMatch.index + additionMatch[0].length,
			content: additionMatch[1]
		});
		additionMatch = ADDITION_PATTERN.exec(doc);
	}

	// Find deletions {--...--}
	DELETION_PATTERN.lastIndex = 0;
	let deletionMatch: RegExpExecArray | null = DELETION_PATTERN.exec(doc);
	while (deletionMatch !== null) {
		matches.push({
			type: 'deletion',
			from: deletionMatch.index,
			to: deletionMatch.index + deletionMatch[0].length,
			content: deletionMatch[1]
		});
		deletionMatch = DELETION_PATTERN.exec(doc);
	}

	// Sort by position
	matches.sort((a, b) => a.from - b.from);

	return matches;
}

/**
 * Check if a position is inside a code block
 */
function isInsideCodeBlock(doc: string, pos: number): boolean {
	// Find all code fence positions
	const fencePattern = /```[^\n]*\n[\s\S]*?```/g;
	let fenceMatch: RegExpExecArray | null = fencePattern.exec(doc);
	while (fenceMatch !== null) {
		if (pos >= fenceMatch.index && pos < fenceMatch.index + fenceMatch[0].length) {
			return true;
		}
		fenceMatch = fencePattern.exec(doc);
	}
	return false;
}

/**
 * Build decorations for CriticMarkup
 */
function buildDecorations(view: EditorView, config: CriticMarkupConfig): DecorationSet {
	const builder = new RangeSetBuilder<Decoration>();
	const doc = view.state.doc.toString();
	const matches = findCriticMarkupMatches(doc);

	for (const match of matches) {
		// Skip matches inside code blocks
		if (isInsideCodeBlock(doc, match.from)) {
			continue;
		}

		// Opening marker decoration (hide it)
		const openMarkerLen = 3; // {++ or {--
		builder.add(
			match.from,
			match.from + openMarkerLen,
			Decoration.mark({ class: 'cm-critic-marker' })
		);

		// Content decoration
		const contentStart = match.from + openMarkerLen;
		const contentEnd = match.to - 3; // ++} or --}
		const contentClass =
			match.type === 'addition' ? 'cm-critic-addition' : 'cm-critic-deletion';
		builder.add(contentStart, contentEnd, Decoration.mark({ class: contentClass }));

		// Closing marker decoration (hide it)
		builder.add(
			match.to - 3,
			match.to,
			Decoration.mark({ class: 'cm-critic-marker' })
		);

		// Widget with buttons at the end
		builder.add(
			match.to,
			match.to,
			Decoration.widget({
				widget: new CriticMarkupButtonWidget(
					match.type,
					match.content,
					match.from,
					match.to,
					config
				),
				side: 1
			})
		);
	}

	return builder.finish();
}

/**
 * Create the CriticMarkup view plugin
 */
function createCriticMarkupPlugin(config: CriticMarkupConfig) {
	return ViewPlugin.fromClass(
		class {
			decorations: DecorationSet;

			constructor(view: EditorView) {
				this.decorations = buildDecorations(view, config);
			}

			update(update: ViewUpdate) {
				if (update.docChanged || update.viewportChanged) {
					this.decorations = buildDecorations(update.view, config);
				}
			}
		},
		{
			decorations: (v) => v.decorations
		}
	);
}

/**
 * Theme for CriticMarkup decorations
 */
const criticMarkupTheme = EditorView.baseTheme({
	// Hide the markers
	'.cm-critic-marker': {
		fontSize: '0',
		opacity: '0',
		width: '0',
		display: 'inline'
	},

	// Addition styling
	'.cm-critic-addition': {
		backgroundColor: 'var(--color-success-subtle)',
		borderBottom: '2px solid var(--color-success)',
		borderRadius: '2px',
		padding: '0 2px'
	},

	// Deletion styling
	'.cm-critic-deletion': {
		backgroundColor: 'var(--color-error-subtle)',
		textDecoration: 'line-through',
		opacity: '0.8',
		borderRadius: '2px',
		padding: '0 2px'
	},

	// Button container
	'.cm-critic-buttons': {
		display: 'inline-flex',
		gap: '2px',
		marginLeft: '4px',
		verticalAlign: 'middle'
	},

	// Button base style
	'.cm-critic-btn': {
		display: 'inline-flex',
		alignItems: 'center',
		justifyContent: 'center',
		width: '18px',
		height: '18px',
		border: 'none',
		borderRadius: '4px',
		fontSize: '11px',
		fontWeight: 'bold',
		cursor: 'pointer',
		transition: 'all 0.15s ease',
		lineHeight: '1'
	},

	// Accept button
	'.cm-critic-accept': {
		backgroundColor: 'var(--color-success)',
		color: 'white'
	},
	'.cm-critic-accept:hover': {
		backgroundColor: 'var(--color-success-hover, #059669)',
		transform: 'scale(1.1)'
	},

	// Reject button
	'.cm-critic-reject': {
		backgroundColor: 'var(--color-error)',
		color: 'white'
	},
	'.cm-critic-reject:hover': {
		backgroundColor: 'var(--color-error-hover, #dc2626)',
		transform: 'scale(1.1)'
	}
});

/**
 * Find the CriticMarkup match at a given position
 */
function findMatchAtPosition(doc: string, pos: number): CriticMarkupMatch | null {
	const matches = findCriticMarkupMatches(doc);
	for (const match of matches) {
		if (pos >= match.from && pos <= match.to) {
			return match;
		}
	}
	return null;
}

/**
 * Create keyboard shortcuts for CriticMarkup
 * - Cmd+Enter: Accept the change under cursor
 * - Cmd+Backspace: Reject the change under cursor
 */
function createCriticMarkupKeymap(config: CriticMarkupConfig) {
	return keymap.of([
		{
			key: 'Mod-Enter',
			run: (view) => {
				const doc = view.state.doc.toString();
				const pos = view.state.selection.main.head;
				const match = findMatchAtPosition(doc, pos);

				if (match) {
					// Accept: keep addition content, remove deletion
					const replacement = match.type === 'addition' ? match.content : '';
					view.dispatch({
						changes: { from: match.from, to: match.to, insert: replacement }
					});
					config.onAccept?.(match.type, match.content, match.from, match.to);
					return true;
				}
				return false;
			}
		},
		{
			key: 'Mod-Backspace',
			run: (view) => {
				const doc = view.state.doc.toString();
				const pos = view.state.selection.main.head;
				const match = findMatchAtPosition(doc, pos);

				if (match) {
					// Reject: remove addition, keep deletion content
					const replacement = match.type === 'deletion' ? match.content : '';
					view.dispatch({
						changes: { from: match.from, to: match.to, insert: replacement }
					});
					config.onReject?.(match.type, match.content, match.from, match.to);
					return true;
				}
				return false;
			}
		}
	]);
}

/**
 * Create the CriticMarkup extension
 *
 * @param config - Optional configuration with callbacks
 * @returns CodeMirror extension array
 */
export function criticMarkupExtension(config: CriticMarkupConfig = {}) {
	return [
		createCriticMarkupPlugin(config),
		createCriticMarkupKeymap(config),
		criticMarkupTheme
	];
}

/**
 * Check if document has any CriticMarkup
 */
export function hasCriticMarkup(doc: string): boolean {
	return ADDITION_PATTERN.test(doc) || DELETION_PATTERN.test(doc);
}

/**
 * Accept all CriticMarkup changes in a document
 * Returns the cleaned document with all additions kept and deletions removed
 */
export function acceptAllChanges(doc: string): string {
	// Keep additions: {++text++} → text
	let result = doc.replace(ADDITION_PATTERN, '$1');
	// Remove deletions: {--text--} → ""
	result = result.replace(DELETION_PATTERN, '');
	return result;
}

/**
 * Reject all CriticMarkup changes in a document
 * Returns the cleaned document with all additions removed and deletions kept
 */
export function rejectAllChanges(doc: string): string {
	// Remove additions: {++text++} → ""
	let result = doc.replace(ADDITION_PATTERN, '');
	// Keep deletions: {--text--} → text
	result = result.replace(DELETION_PATTERN, '$1');
	return result;
}
