/**
 * CodeMirror Wiki Editor Theme & Live Preview
 *
 * Obsidian-style live preview with:
 * - Sans-serif body, serif headings
 * - Syntax hidden when line is inactive, revealed on focus
 * - Clickable wiki links, external links, citations
 * - Inline image previews
 */

import {
	EditorView,
	Decoration,
	type DecorationSet,
	WidgetType,
	ViewPlugin,
	type ViewUpdate,
} from "@codemirror/view";
import { syntaxTree } from "@codemirror/language";
import { RangeSetBuilder } from "@codemirror/state";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags } from "@lezer/highlight";

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

/**
 * Get all active line numbers (handles multi-cursor and selections)
 * Returns empty set if editor is not focused (so all syntax gets hidden)
 */
function getActiveLines(view: EditorView): Set<number> {
	// If editor is not focused, return empty set so all syntax is hidden
	if (!view.hasFocus) {
		return new Set<number>();
	}

	const lines = new Set<number>();
	for (const range of view.state.selection.ranges) {
		const startLine = view.state.doc.lineAt(range.from).number;
		const endLine = view.state.doc.lineAt(range.to).number;
		for (let i = startLine; i <= endLine; i++) {
			lines.add(i);
		}
	}
	return lines;
}

/**
 * Extract domain from URL for favicon
 */
function getDomain(url: string): string {
	try {
		return new URL(url).hostname;
	} catch {
		return "";
	}
}

function createGlobeSvg(): SVGElement {
	const ns = "http://www.w3.org/2000/svg";
	const svg = document.createElementNS(ns, "svg");
	svg.setAttribute("viewBox", "0 0 24 24");
	svg.setAttribute("fill", "none");
	svg.setAttribute("stroke", "currentColor");
	svg.setAttribute("stroke-width", "1.8");
	svg.setAttribute("stroke-linecap", "round");
	svg.setAttribute("stroke-linejoin", "round");

	const circle = document.createElementNS(ns, "circle");
	circle.setAttribute("cx", "12");
	circle.setAttribute("cy", "12");
	circle.setAttribute("r", "10");

	const meridian = document.createElementNS(ns, "path");
	meridian.setAttribute("d", "M2 12h20");

	const longitudes = document.createElementNS(ns, "path");
	longitudes.setAttribute("d", "M12 2c3 3 3 17 0 20M12 2c-3 3-3 17 0 20");

	svg.appendChild(circle);
	svg.appendChild(meridian);
	svg.appendChild(longitudes);
	return svg;
}

/**
 * Check if URL is external (not a wiki link)
 */
function isExternalUrl(url: string): boolean {
	return url.startsWith("http://") || url.startsWith("https://");
}

// =============================================================================
// WIDGET CLASSES
// =============================================================================

/**
 * Wiki link widget: [[Page Name]] or [[Page Name|Display Text]]
 * Renders as clickable link, dispatches custom event for SvelteKit navigation
 */
class WikiLinkWidget extends WidgetType {
	constructor(
		readonly text: string,
		readonly slug: string
	) {
		super();
	}

	toDOM() {
		const link = document.createElement("a");
		link.className = "cm-wikilink";
		const text = document.createElement("span");
		text.className = "cm-link-text";
		text.textContent = this.text;
		link.appendChild(text);
		link.href = `/wiki/${this.slug}`;
		link.addEventListener("click", (e) => {
			e.preventDefault();
			e.stopPropagation();
			link.dispatchEvent(
				new CustomEvent("wiki-navigate", {
					bubbles: true,
					detail: { href: `/wiki/${this.slug}` },
				})
			);
		});
		return link;
	}

	eq(other: WikiLinkWidget) {
		return other.text === this.text && other.slug === this.slug;
	}

	ignoreEvent() {
		// When the widget is shown (inactive line), clicking should navigate instead of
		// putting the editor into "edit this line" mode.
		return true;
	}
}

/**
 * External link widget: [text](https://...)
 * Shows link text with favicon and external icon
 */
class ExternalLinkWidget extends WidgetType {
	constructor(
		readonly text: string,
		readonly url: string
	) {
		super();
	}

	toDOM() {
		const link = document.createElement("a");
		link.className = "cm-external-link";
		link.href = this.url;
		link.target = "_blank";
		link.rel = "noopener noreferrer";

		// Icon slot: show a globe immediately, then swap in favicon if it loads.
		// This avoids "nothing shows" cases where remote images are blocked/slow.
		const icon = document.createElement("span");
		icon.className = "cm-external-icon";
		icon.appendChild(createGlobeSvg());

		const domain = getDomain(this.url);
		if (domain) {
			const favicon = document.createElement("img");
			favicon.className = "cm-external-favicon";
			favicon.src = `https://www.google.com/s2/favicons?domain=${domain}&sz=16`;
			// Slightly smaller so it aligns better with text baseline
			favicon.width = 12;
			favicon.height = 12;
			favicon.alt = "";
			favicon.loading = "lazy";
			// Some environments block remote images without firing a reliable error,
			// so we only swap on a successful load and otherwise keep the globe.
			favicon.referrerPolicy = "no-referrer";
			favicon.decoding = "async";
			favicon.onload = () => {
				try {
					icon.replaceChildren(favicon);
				} catch {
					// ignore
				}
			};
		}

		link.appendChild(icon);

		// Link text
		const text = document.createElement("span");
		text.className = "cm-link-text";
		text.textContent = this.text;
		link.appendChild(text);

		return link;
	}

	eq(other: ExternalLinkWidget) {
		return other.text === this.text && other.url === this.url;
	}

	ignoreEvent() {
		return true;
	}
}

/**
 * Internal link widget: [text](/path)
 * Shows link text, navigates via SvelteKit
 */
class InternalLinkWidget extends WidgetType {
	constructor(
		readonly text: string,
		readonly href: string
	) {
		super();
	}

	toDOM() {
		const link = document.createElement("a");
		link.className = "cm-internal-link";
		const text = document.createElement("span");
		text.className = "cm-link-text";
		text.textContent = this.text;
		link.appendChild(text);
		link.href = this.href;
		link.addEventListener("click", (e) => {
			e.preventDefault();
			e.stopPropagation();
			link.dispatchEvent(
				new CustomEvent("wiki-navigate", {
					bubbles: true,
					detail: { href: this.href },
				})
			);
		});
		return link;
	}

	eq(other: InternalLinkWidget) {
		return other.text === this.text && other.href === this.href;
	}

	ignoreEvent() {
		return true;
	}
}

/**
 * Citation widget: [1], [2], etc.
 * Renders as superscript, scrolls to citation on click
 */
class CitationWidget extends WidgetType {
	constructor(readonly index: string) {
		super();
	}

	toDOM() {
		// Use a plain inline element instead of <sup> to avoid overly aggressive
		// superscript baseline shifting (which can collide with the line above).
		const el = document.createElement("span");
		el.className = "cm-citation cm-citation-widget";
		el.textContent = `[${this.index}]`;
		el.title = `Citation ${this.index} - click to scroll`;
		el.addEventListener("click", (e) => {
			e.preventDefault();
			e.stopPropagation();
			const target = document.getElementById(`citation-${this.index}`);
			if (target) {
				target.scrollIntoView({ behavior: "smooth", block: "center" });
				// Brief highlight effect
				target.classList.add("cm-citation-highlight");
				setTimeout(() => target.classList.remove("cm-citation-highlight"), 1500);
			}
		});
		return el;
	}

	eq(other: CitationWidget) {
		return other.index === this.index;
	}

	ignoreEvent() {
		// When rendered (inactive line), clicking should trigger the widget action
		// instead of entering "edit this line" mode.
		return true;
	}
}

/**
 * Image widget: ![alt](url)
 * Renders inline image preview
 */
class ImageWidget extends WidgetType {
	constructor(
		readonly src: string,
		readonly alt: string
	) {
		super();
	}

	toDOM() {
		const wrapper = document.createElement("span");
		wrapper.className = "cm-image-wrapper";

		const img = document.createElement("img");
		img.className = "cm-image";
		img.src = this.src;
		img.alt = this.alt;
		img.loading = "lazy";

		// Handle load errors gracefully
		img.onerror = () => {
			wrapper.classList.add("cm-image-error");
			img.style.display = "none";
			const placeholder = document.createElement("span");
			placeholder.className = "cm-image-placeholder";
			placeholder.textContent = `[Image: ${this.alt || "failed to load"}]`;
			wrapper.appendChild(placeholder);
		};

		wrapper.appendChild(img);
		return wrapper;
	}

	eq(other: ImageWidget) {
		return other.src === this.src && other.alt === this.alt;
	}

	ignoreEvent() {
		return false;
	}
}

/**
 * Horizontal rule widget: ---
 * Renders as styled <hr>
 */
class HorizontalRuleWidget extends WidgetType {
	toDOM() {
		const hr = document.createElement("hr");
		hr.className = "cm-hr";
		return hr;
	}

	eq() {
		return true;
	}

	ignoreEvent() {
		return true;
	}
}

// =============================================================================
// THEME STYLES
// =============================================================================

export const wikiEditorTheme = EditorView.theme({
	// Base editor styles
	"&": {
		fontSize: "1rem",
		lineHeight: "1.75",
	},
	".cm-scroller": {
		fontFamily: "var(--font-sans, ui-sans-serif, system-ui, -apple-system, sans-serif)",
	},
	".cm-content": {
		padding: "0",
		caretColor: "var(--color-primary)",
	},
	".cm-line": {
		padding: "2px 0",
	},

	// Cursor and selection
	"&.cm-focused .cm-cursor": {
		borderLeftColor: "var(--color-primary)",
		borderLeftWidth: "2px",
	},
	"&.cm-focused .cm-selectionBackground, ::selection": {
		backgroundColor: "color-mix(in srgb, var(--color-primary) 20%, transparent)",
	},
	".cm-selectionBackground": {
		backgroundColor: "color-mix(in srgb, var(--color-primary) 15%, transparent)",
	},
	".cm-activeLine": {
		backgroundColor: "color-mix(in srgb, var(--color-foreground) 3%, transparent)",
	},

	// Hide gutters
	".cm-gutters": {
		display: "none",
	},

	// Placeholder
	".cm-placeholder": {
		color: "var(--color-foreground-subtle)",
		fontStyle: "italic",
	},

	// Dimmed syntax markers (visible on active line)
	".cm-syntax-dim": {
		color: "var(--color-foreground-muted)",
		opacity: "0.6",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// HEADINGS
	// ─────────────────────────────────────────────────────────────────────────
	".cm-heading-line": {
		fontFamily: "var(--font-serif, Georgia, 'Times New Roman', serif)",
		fontWeight: "400",
		color: "var(--color-foreground)",
	},
	".cm-heading-1": { fontSize: "1.75rem", lineHeight: "1.3" },
	".cm-heading-2": { fontSize: "1.375rem", lineHeight: "1.35" },
	".cm-heading-3": { fontSize: "1.125rem", lineHeight: "1.4", fontWeight: "500" },
	".cm-heading-4": { fontSize: "1rem", lineHeight: "1.5", fontWeight: "500" },
	".cm-heading-5": { fontSize: "0.9375rem", lineHeight: "1.5", fontWeight: "600" },
	".cm-heading-6": { fontSize: "0.875rem", lineHeight: "1.5", fontWeight: "600" },

	// ─────────────────────────────────────────────────────────────────────────
	// WIKI LINKS
	// ─────────────────────────────────────────────────────────────────────────
	".cm-wikilink": {
		color: "var(--color-primary)",
		textDecoration: "none",
		cursor: "pointer",
	},
	".cm-link-text": {
		display: "inline",
		position: "relative",
		backgroundImage:
			"linear-gradient(to top, color-mix(in srgb, var(--color-primary) 15%, transparent), color-mix(in srgb, var(--color-primary) 15%, transparent))",
		backgroundRepeat: "no-repeat",
		backgroundSize: "100% 0%",
		backgroundPosition: "0 100%",
		transition: "background-size 0.2s ease",
	},
	".cm-wikilink:hover .cm-link-text": {
		backgroundSize: "100% 100%",
	},
	".cm-wikilink-text": {
		color: "var(--color-primary)",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// EXTERNAL LINKS
	// ─────────────────────────────────────────────────────────────────────────
	".cm-external-link": {
		color: "var(--color-primary)",
		textDecoration: "none",
		cursor: "pointer",
		// Avoid inline-flex baseline quirks (can make the whole link sit lower).
		// Keep it as a normal inline element and align the icon separately.
		display: "inline",
	},
	".cm-external-favicon": {
		width: "12px",
		height: "12px",
		flexShrink: "0",
		opacity: "0.8",
		verticalAlign: "middle",
	},
	".cm-external-icon": {
		width: "12px",
		height: "12px",
		display: "inline-block",
		marginRight: "0.25rem",
		// Nudge upward a touch so the icon aligns with the text baseline
		verticalAlign: "-0.125em",
		opacity: "0.8",
		color: "var(--color-foreground-muted)",
	},
	".cm-external-icon svg": {
		width: "12px",
		height: "12px",
		display: "block",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// INTERNAL LINKS
	// ─────────────────────────────────────────────────────────────────────────
	".cm-internal-link": {
		color: "var(--color-primary)",
		textDecoration: "none",
		cursor: "pointer",
	},
	".cm-external-link:hover .cm-link-text": {
		backgroundSize: "100% 100%",
	},
	".cm-internal-link:hover .cm-link-text": {
		backgroundSize: "100% 100%",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// CITATIONS
	// ─────────────────────────────────────────────────────────────────────────
	".cm-citation": {
		color: "var(--color-primary)",
		fontSize: "0.75em",
		fontWeight: "400",
		cursor: "pointer",
		// Match widget styling to prevent line height jump when switching modes
		verticalAlign: "0.35em",
		lineHeight: "1",
	},
	".cm-citation-widget": {
		verticalAlign: "0.35em",
		lineHeight: "1",
		display: "inline-block",
	},
	".cm-citation:hover": {
		textDecoration: "underline",
	},
	".cm-citation-highlight": {
		backgroundColor: "color-mix(in srgb, var(--color-primary) 20%, transparent)",
		transition: "background-color 0.3s ease",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// IMAGES
	// ─────────────────────────────────────────────────────────────────────────
	".cm-image-wrapper": {
		display: "block",
		margin: "0.5rem 0",
	},
	".cm-image": {
		maxWidth: "100%",
		height: "auto",
		borderRadius: "0.375rem",
	},
	".cm-image-error": {
		color: "var(--color-foreground-subtle)",
		fontStyle: "italic",
	},
	".cm-image-placeholder": {
		color: "var(--color-foreground-subtle)",
		fontSize: "0.875rem",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// HORIZONTAL RULES
	// ─────────────────────────────────────────────────────────────────────────
	".cm-hr": {
		display: "block",
		border: "none",
		borderTop: "1px solid var(--color-border)",
		margin: "0",
		height: "0",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// BLOCKQUOTES
	// ─────────────────────────────────────────────────────────────────────────
	".cm-blockquote-line": {
		borderLeft: "3px solid var(--color-border)",
		paddingLeft: "1rem !important",
		color: "var(--color-foreground-muted)",
		fontStyle: "italic",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// INLINE CODE
	// ─────────────────────────────────────────────────────────────────────────
	".cm-code-text": {
		fontFamily: "var(--font-mono, ui-monospace, monospace)",
		fontSize: "0.875em",
		backgroundColor: "var(--color-surface)",
		padding: "0.125rem 0.375rem",
		borderRadius: "0.25rem",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// FENCED CODE BLOCKS
	// ─────────────────────────────────────────────────────────────────────────
	".cm-codeblock-line": {
		fontFamily: "var(--font-mono, ui-monospace, monospace)",
		fontSize: "0.875em",
		backgroundColor: "var(--color-surface)",
	},
	".cm-codeblock-start": {
		borderTopLeftRadius: "0.375rem",
		borderTopRightRadius: "0.375rem",
		paddingTop: "0.5rem !important",
	},
	".cm-codeblock-end": {
		borderBottomLeftRadius: "0.375rem",
		borderBottomRightRadius: "0.375rem",
		paddingBottom: "0.5rem !important",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// LISTS
	// ─────────────────────────────────────────────────────────────────────────
	".cm-list-bullet": {
		color: "var(--color-foreground-muted)",
	},
});

// =============================================================================
// SYNTAX HIGHLIGHTING
// =============================================================================

export const wikiHighlightStyle = HighlightStyle.define([
	// Headings
	{ tag: tags.heading1, fontFamily: "var(--font-serif, Georgia, serif)", fontSize: "1.75rem" },
	{ tag: tags.heading2, fontFamily: "var(--font-serif, Georgia, serif)", fontSize: "1.375rem" },
	{ tag: tags.heading3, fontFamily: "var(--font-serif, Georgia, serif)", fontSize: "1.125rem", fontWeight: "500" },
	{ tag: tags.heading4, fontFamily: "var(--font-serif, Georgia, serif)", fontSize: "1rem", fontWeight: "500" },

	// Emphasis
	{ tag: tags.emphasis, fontStyle: "italic" },
	// Keep bold readable but not "full bold" (avoid 700+)
	{ tag: tags.strong, fontWeight: "500" },
	{ tag: tags.strikethrough, textDecoration: "line-through", color: "var(--color-foreground-subtle)" },

	// Code
	{ tag: tags.monospace, fontFamily: "var(--font-mono, monospace)", fontSize: "0.875em" },

	// Links
	{ tag: tags.link, color: "var(--color-primary)" },
	{ tag: tags.url, color: "var(--color-primary)", opacity: "0.8" },

	// Quotes
	{ tag: tags.quote, color: "var(--color-foreground-muted)", fontStyle: "italic" },

	// Meta/syntax markers - dim them but keep readable
	{ tag: tags.processingInstruction, color: "var(--color-foreground-muted)", opacity: "0.6" },
	{ tag: tags.meta, color: "var(--color-foreground-muted)", opacity: "0.6" },
]);

export const wikiSyntaxHighlighting = syntaxHighlighting(wikiHighlightStyle);

// =============================================================================
// DECORATION BUILDERS
// =============================================================================

interface DecorationEntry {
	from: number;
	to: number;
	deco: Decoration;
}

/**
 * Decorate ATX headings (## Heading)
 */
function decorateHeadings(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const tree = syntaxTree(view.state);

	tree.iterate({
		enter(node) {
			if (/^ATXHeading[1-6]$/.test(node.name)) {
				const level = parseInt(node.name.charAt(node.name.length - 1)) || 1;
				const line = doc.lineAt(node.from);
				const isActive = activeLines.has(line.number);
				const match = line.text.match(/^(#{1,6})\s*/);

				// Line decoration for heading styling
				decorations.push({
					from: line.from,
					to: line.from,
					deco: Decoration.line({ class: `cm-heading-line cm-heading-${level}` }),
				});

				// Hide hash markers when not active
				if (match && !isActive) {
					decorations.push({
						from: line.from,
						to: line.from + match[0].length,
						deco: Decoration.replace({}),
					});
				}
			}
		},
	});
}

/**
 * Decorate emphasis (bold, italic, strikethrough)
 */
function decorateEmphasis(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const tree = syntaxTree(view.state);

	tree.iterate({
		enter(node) {
			const line = doc.lineAt(node.from);
			const isActive = activeLines.has(line.number);

			// Bold **text** or __text__
			if (node.name === "StrongEmphasis") {
				const text = view.state.sliceDoc(node.from, node.to);
				const marker = text.startsWith("**") ? "**" : "__";
				if (text.startsWith(marker) && text.endsWith(marker) && !isActive) {
					decorations.push({ from: node.from, to: node.from + 2, deco: Decoration.replace({}) });
					decorations.push({ from: node.to - 2, to: node.to, deco: Decoration.replace({}) });
				}
			}

			// Italic *text* or _text_
			if (node.name === "Emphasis") {
				const text = view.state.sliceDoc(node.from, node.to);
				if ((text.startsWith("*") || text.startsWith("_")) && !isActive) {
					decorations.push({ from: node.from, to: node.from + 1, deco: Decoration.replace({}) });
					decorations.push({ from: node.to - 1, to: node.to, deco: Decoration.replace({}) });
				}
			}

			// Strikethrough ~~text~~
			if (node.name === "Strikethrough") {
				if (!isActive) {
					decorations.push({ from: node.from, to: node.from + 2, deco: Decoration.replace({}) });
					decorations.push({ from: node.to - 2, to: node.to, deco: Decoration.replace({}) });
				}
			}
		},
	});
}

/**
 * Decorate inline code `code`
 */
function decorateInlineCode(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const tree = syntaxTree(view.state);

	tree.iterate({
		enter(node) {
			if (node.name === "InlineCode") {
				const line = doc.lineAt(node.from);
				const isActive = activeLines.has(line.number);

				// Style the code content
				if (node.to - node.from > 2) {
					decorations.push({
						from: node.from + 1,
						to: node.to - 1,
						deco: Decoration.mark({ class: "cm-code-text" }),
					});
				}

				// Hide backticks when not active
				if (!isActive) {
					decorations.push({ from: node.from, to: node.from + 1, deco: Decoration.replace({}) });
					decorations.push({ from: node.to - 1, to: node.to, deco: Decoration.replace({}) });
				}
			}
		},
	});
}

/**
 * Decorate blockquotes
 */
function decorateBlockquotes(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const tree = syntaxTree(view.state);

	tree.iterate({
		enter(node) {
			if (node.name === "Blockquote") {
				// Apply to all lines in the blockquote
				const startLine = doc.lineAt(node.from);
				const endLine = doc.lineAt(node.to);

				for (let lineNum = startLine.number; lineNum <= endLine.number; lineNum++) {
					const line = doc.line(lineNum);
					const isActive = activeLines.has(lineNum);

					decorations.push({
						from: line.from,
						to: line.from,
						deco: Decoration.line({ class: "cm-blockquote-line" }),
					});

					// Hide > marker when not active
					const match = line.text.match(/^>\s*/);
					if (match && !isActive) {
						decorations.push({
							from: line.from,
							to: line.from + match[0].length,
							deco: Decoration.replace({}),
						});
					}
				}
			}
		},
	});
}

/**
 * Decorate fenced code blocks
 */
function decorateCodeBlocks(
	view: EditorView,
	_activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const tree = syntaxTree(view.state);

	tree.iterate({
		enter(node) {
			if (node.name === "FencedCode") {
				const startLine = doc.lineAt(node.from);
				const endLine = doc.lineAt(node.to);

				for (let lineNum = startLine.number; lineNum <= endLine.number; lineNum++) {
					const line = doc.line(lineNum);
					let classes = "cm-codeblock-line";

					if (lineNum === startLine.number) classes += " cm-codeblock-start";
					if (lineNum === endLine.number) classes += " cm-codeblock-end";

					decorations.push({
						from: line.from,
						to: line.from,
						deco: Decoration.line({ class: classes }),
					});
				}
			}
		},
	});
}

/**
 * Decorate horizontal rules
 */
function decorateHorizontalRules(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const tree = syntaxTree(view.state);

	tree.iterate({
		enter(node) {
			if (node.name === "HorizontalRule") {
				const line = doc.lineAt(node.from);
				const isActive = activeLines.has(line.number);

				if (!isActive) {
					// Ensure we don't replace the newline - only replace up to end of line
					const lineEnd = line.to;
					const replaceEnd = Math.min(node.to, lineEnd);
					
					decorations.push({
						from: node.from,
						to: replaceEnd,
						deco: Decoration.replace({ widget: new HorizontalRuleWidget() }),
					});
				}
			}
		},
	});
}

/**
 * Decorate markdown links [text](url)
 */
function decorateLinks(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const tree = syntaxTree(view.state);

	tree.iterate({
		enter(node) {
			if (node.name === "Link") {
				const line = doc.lineAt(node.from);
				const isActive = activeLines.has(line.number);
				const fullText = view.state.sliceDoc(node.from, node.to);

				// Parse [text](url)
				const match = fullText.match(/^\[([^\]]*)\]\(([^)]*)\)$/);
				if (!match) return;

				const [, linkText, url] = match;

				if (!isActive) {
					if (isExternalUrl(url)) {
						decorations.push({
							from: node.from,
							to: node.to,
							deco: Decoration.replace({ widget: new ExternalLinkWidget(linkText, url) }),
						});
					} else {
						decorations.push({
							from: node.from,
							to: node.to,
							deco: Decoration.replace({ widget: new InternalLinkWidget(linkText, url) }),
						});
					}
				}
			}
		},
	});
}

/**
 * Decorate images ![alt](url)
 */
function decorateImages(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const tree = syntaxTree(view.state);

	tree.iterate({
		enter(node) {
			if (node.name === "Image") {
				const line = doc.lineAt(node.from);
				const isActive = activeLines.has(line.number);
				const fullText = view.state.sliceDoc(node.from, node.to);

				// Parse ![alt](url)
				const match = fullText.match(/^!\[([^\]]*)\]\(([^)]*)\)$/);
				if (!match) return;

				const [, alt, src] = match;

				if (!isActive) {
					decorations.push({
						from: node.from,
						to: node.to,
						deco: Decoration.replace({ widget: new ImageWidget(src, alt) }),
					});
				}
			}
		},
	});
}

/**
 * Decorate wiki links [[Page Name]] or [[Page Name|Display]]
 */
function decorateWikiLinks(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const docText = doc.toString();

	const wikiLinkRegex = /\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g;
	let match: RegExpExecArray | null = wikiLinkRegex.exec(docText);

	while (match !== null) {
		const start = match.index;
		const end = start + match[0].length;
		const line = doc.lineAt(start);
		const isActive = activeLines.has(line.number);

		const linkTarget = match[1];
		const linkText = match[2] || match[1];
		const slug = linkTarget.toLowerCase().replace(/\s+/g, "-");

		if (!isActive) {
			decorations.push({
				from: start,
				to: end,
				deco: Decoration.replace({ widget: new WikiLinkWidget(linkText, slug) }),
			});
		} else {
			// When active, style the text but show brackets dimmed
			decorations.push({
				from: start,
				to: start + 2,
				deco: Decoration.mark({ class: "cm-syntax-dim" }),
			});
			decorations.push({
				from: start + 2,
				to: end - 2,
				deco: Decoration.mark({ class: "cm-wikilink-text" }),
			});
			decorations.push({
				from: end - 2,
				to: end,
				deco: Decoration.mark({ class: "cm-syntax-dim" }),
			});
		}
		match = wikiLinkRegex.exec(docText);
	}
}

/**
 * Decorate citations [1], [2], etc.
 */
function decorateCitations(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const docText = doc.toString();

	// Match [1], [2], etc. but not markdown links [text](url)
	const citationRegex = /(?<!\])\[(\d+)\](?!\()/g;
	let match: RegExpExecArray | null = citationRegex.exec(docText);

	while (match !== null) {
		const start = match.index;
		const end = start + match[0].length;
		const line = doc.lineAt(start);
		const isActive = activeLines.has(line.number);
		const citationNum = match[1];

		if (!isActive) {
			decorations.push({
				from: start,
				to: end,
				deco: Decoration.replace({ widget: new CitationWidget(citationNum) }),
			});
		} else {
			decorations.push({
				from: start,
				to: end,
				deco: Decoration.mark({ class: "cm-citation" }),
			});
		}
		match = citationRegex.exec(docText);
	}
}

// =============================================================================
// MAIN DECORATION BUILDER
// =============================================================================

function buildDecorations(view: EditorView): DecorationSet {
	const decorations: DecorationEntry[] = [];
	const activeLines = getActiveLines(view);

	// Run all decoration builders
	decorateHeadings(view, activeLines, decorations);
	decorateEmphasis(view, activeLines, decorations);
	decorateInlineCode(view, activeLines, decorations);
	decorateBlockquotes(view, activeLines, decorations);
	decorateCodeBlocks(view, activeLines, decorations);
	decorateHorizontalRules(view, activeLines, decorations);
	decorateLinks(view, activeLines, decorations);
	decorateImages(view, activeLines, decorations);
	decorateWikiLinks(view, activeLines, decorations);
	decorateCitations(view, activeLines, decorations);

	// Sort decorations by position
	decorations.sort((a, b) => {
		if (a.from !== b.from) return a.from - b.from;
		// Line decorations (from === to) should come first
		if (a.from === a.to && b.from !== b.to) return -1;
		if (b.from === b.to && a.from !== a.to) return 1;
		return a.to - b.to;
	});

	// Build decoration set, handling overlaps
	const builder = new RangeSetBuilder<Decoration>();
	let lastTo = 0;
	let lastLineNumber = -1;

	for (const { from, to, deco } of decorations) {
		// Validate range
		if (from < 0 || to > view.state.doc.length) continue;
		if (from > to) continue;

		// Get line number for this decoration
		const fromLine = view.state.doc.lineAt(from);
		const currentLineNumber = fromLine.number;
		
		// Reset lastTo when we move to a new line (allows decorations at start of line)
		if (currentLineNumber !== lastLineNumber) {
			lastTo = fromLine.from; // Start of line
			lastLineNumber = currentLineNumber;
		}

		// Skip if overlapping with previous decoration
		// Allow: line decorations (from === to), adjacent decorations (from === lastTo or from === fromLine.from)
		if (from < lastTo && from !== to && from !== lastTo && from !== fromLine.from) {
			continue;
		}

		// For replace decorations, ensure we don't replace newlines
		// Check if range includes a newline character
		if (from !== to) {
			const lineEnd = fromLine.to;
			
			// If decoration extends beyond line end, it includes a newline - skip it
			// But allow decorations that end exactly at line end (they don't include the newline)
			if (to > lineEnd) {
				continue;
			}
		}

		try {
			builder.add(from, to, deco);
			if (to > from) lastTo = to;
		} catch {
			// Skip invalid decorations
		}
	}

	return builder.finish();
}

// =============================================================================
// LIVE PREVIEW PLUGIN
// =============================================================================

export const livePreviewPlugin = ViewPlugin.fromClass(
	class {
		decorations: DecorationSet;

		constructor(view: EditorView) {
			this.decorations = buildDecorations(view);
		}

		update(update: ViewUpdate) {
			if (
				update.docChanged ||
				update.selectionSet ||
				update.viewportChanged ||
				update.focusChanged
			) {
				this.decorations = buildDecorations(update.view);
			}
		}
	},
	{
		decorations: (v) => v.decorations,
	}
);

export const livePreviewExtension = [livePreviewPlugin];
