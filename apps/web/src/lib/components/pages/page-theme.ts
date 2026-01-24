/**
 * CodeMirror Pages Editor Theme & Live Preview
 *
 * Based on wiki-theme.ts but adapted for Pages with entity linking.
 * Entity links use the format: [Display Name](entity:prefix_hash)
 *
 * Features:
 * - Sans-serif body, serif headings
 * - Syntax hidden when line is inactive, revealed on focus
 * - Clickable entity links with prefix-aware icons
 * - External links, images, inline code
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
 * Check if URL is external (not an internal link)
 */
function isExternalUrl(url: string): boolean {
	return url.startsWith("http://") || url.startsWith("https://");
}

/**
 * Get icon name based on entity ID prefix
 */
function getEntityIcon(id: string): string {
	if (id.startsWith("person_")) return "ri:user-line";
	if (id.startsWith("place_")) return "ri:map-pin-line";
	if (id.startsWith("org_")) return "ri:building-line";
	if (id.startsWith("file_")) return "ri:file-line";
	if (id.startsWith("page_")) return "ri:file-text-line";
	if (id.startsWith("thing_")) return "ri:box-3-line";
	return "ri:links-line";
}

/**
 * Get route path based on entity ID prefix
 */
function getEntityRoute(id: string): string {
	if (id.startsWith("person_")) return `/wiki/${id}`;
	if (id.startsWith("place_")) return `/wiki/${id}`;
	if (id.startsWith("org_")) return `/wiki/${id}`;
	if (id.startsWith("thing_")) return `/wiki/${id}`;
	if (id.startsWith("file_")) return `/data/drive?file=${id}`;
	if (id.startsWith("page_")) return `/pages/${id}`;
	// Default to wiki slug lookup
	return `/wiki/${id}`;
}

// =============================================================================
// WIDGET CLASSES
// =============================================================================

/**
 * Entity link widget: [Display Name](entity:prefix_hash)
 * Renders as clickable chip with icon, dispatches custom event for SvelteKit navigation
 */
class EntityLinkWidget extends WidgetType {
	constructor(
		readonly displayName: string,
		readonly entityId: string
	) {
		super();
	}

	toDOM() {
		const link = document.createElement("a");
		link.className = "cm-entity-link";
		
		// Create icon element
		const iconSpan = document.createElement("span");
		iconSpan.className = "cm-entity-icon";
		
		// Use iconify-icon for the icon
		const icon = document.createElement("iconify-icon");
		icon.setAttribute("icon", getEntityIcon(this.entityId));
		icon.setAttribute("width", "14");
		iconSpan.appendChild(icon);
		link.appendChild(iconSpan);
		
		// Create text element
		const text = document.createElement("span");
		text.className = "cm-entity-text";
		text.textContent = this.displayName;
		link.appendChild(text);
		
		link.href = getEntityRoute(this.entityId);
		link.addEventListener("click", (e) => {
			e.preventDefault();
			e.stopPropagation();
			link.dispatchEvent(
				new CustomEvent("page-navigate", {
					bubbles: true,
					detail: { href: getEntityRoute(this.entityId), entityId: this.entityId },
				})
			);
		});
		return link;
	}

	eq(other: EntityLinkWidget) {
		return other.displayName === this.displayName && other.entityId === this.entityId;
	}

	ignoreEvent() {
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

		const icon = document.createElement("span");
		icon.className = "cm-external-icon";
		icon.appendChild(createGlobeSvg());

		const domain = getDomain(this.url);
		if (domain) {
			const favicon = document.createElement("img");
			favicon.className = "cm-external-favicon";
			favicon.src = `https://www.google.com/s2/favicons?domain=${domain}&sz=16`;
			favicon.width = 12;
			favicon.height = 12;
			favicon.alt = "";
			favicon.loading = "lazy";
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
				new CustomEvent("page-navigate", {
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

export const pageEditorTheme = EditorView.theme({
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

	// Dimmed syntax markers
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
	// ENTITY LINKS - [Display Name](entity:prefix_hash)
	// ─────────────────────────────────────────────────────────────────────────
	".cm-entity-link": {
		display: "inline-flex",
		alignItems: "center",
		gap: "4px",
		padding: "1px 8px 1px 6px",
		borderRadius: "4px",
		backgroundColor: "color-mix(in srgb, var(--color-primary) 10%, transparent)",
		color: "var(--color-primary)",
		textDecoration: "none",
		cursor: "pointer",
		fontSize: "0.9em",
		verticalAlign: "baseline",
		transition: "background-color 0.15s ease",
	},
	".cm-entity-link:hover": {
		backgroundColor: "color-mix(in srgb, var(--color-primary) 20%, transparent)",
	},
	".cm-entity-icon": {
		display: "flex",
		alignItems: "center",
		opacity: "0.8",
	},
	".cm-entity-text": {
		fontWeight: "500",
	},
	".cm-entity-link-raw": {
		color: "var(--color-primary)",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// EXTERNAL LINKS
	// ─────────────────────────────────────────────────────────────────────────
	".cm-external-link": {
		color: "var(--color-primary)",
		textDecoration: "none",
		cursor: "pointer",
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
	".cm-external-link:hover .cm-link-text": {
		backgroundSize: "100% 100%",
	},
	".cm-internal-link:hover .cm-link-text": {
		backgroundSize: "100% 100%",
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

export const pageHighlightStyle = HighlightStyle.define([
	// Headings
	{ tag: tags.heading1, fontFamily: "var(--font-serif, Georgia, serif)", fontSize: "1.75rem" },
	{ tag: tags.heading2, fontFamily: "var(--font-serif, Georgia, serif)", fontSize: "1.375rem" },
	{ tag: tags.heading3, fontFamily: "var(--font-serif, Georgia, serif)", fontSize: "1.125rem", fontWeight: "500" },
	{ tag: tags.heading4, fontFamily: "var(--font-serif, Georgia, serif)", fontSize: "1rem", fontWeight: "500" },

	// Emphasis
	{ tag: tags.emphasis, fontStyle: "italic" },
	{ tag: tags.strong, fontWeight: "500" },
	{ tag: tags.strikethrough, textDecoration: "line-through", color: "var(--color-foreground-subtle)" },

	// Code
	{ tag: tags.monospace, fontFamily: "var(--font-mono, monospace)", fontSize: "0.875em" },

	// Links
	{ tag: tags.link, color: "var(--color-primary)" },
	{ tag: tags.url, color: "var(--color-primary)", opacity: "0.8" },

	// Quotes
	{ tag: tags.quote, color: "var(--color-foreground-muted)", fontStyle: "italic" },

	// Meta/syntax markers
	{ tag: tags.processingInstruction, color: "var(--color-foreground-muted)", opacity: "0.6" },
	{ tag: tags.meta, color: "var(--color-foreground-muted)", opacity: "0.6" },
]);

export const pageSyntaxHighlighting = syntaxHighlighting(pageHighlightStyle);

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

				decorations.push({
					from: line.from,
					to: line.from,
					deco: Decoration.line({ class: `cm-heading-line cm-heading-${level}` }),
				});

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

			if (node.name === "StrongEmphasis") {
				const text = view.state.sliceDoc(node.from, node.to);
				const marker = text.startsWith("**") ? "**" : "__";
				if (text.startsWith(marker) && text.endsWith(marker) && !isActive) {
					decorations.push({ from: node.from, to: node.from + 2, deco: Decoration.replace({}) });
					decorations.push({ from: node.to - 2, to: node.to, deco: Decoration.replace({}) });
				}
			}

			if (node.name === "Emphasis") {
				const text = view.state.sliceDoc(node.from, node.to);
				if ((text.startsWith("*") || text.startsWith("_")) && !isActive) {
					decorations.push({ from: node.from, to: node.from + 1, deco: Decoration.replace({}) });
					decorations.push({ from: node.to - 1, to: node.to, deco: Decoration.replace({}) });
				}
			}

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

				if (node.to - node.from > 2) {
					decorations.push({
						from: node.from + 1,
						to: node.to - 1,
						deco: Decoration.mark({ class: "cm-code-text" }),
					});
				}

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
 * Decorate entity links: [Display Name](entity:prefix_hash)
 * This is the new syntax for linking to entities in pages
 */
function decorateEntityLinks(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const docText = doc.toString();

	// Match [Display Name](entity:entity_id) pattern
	const entityLinkRegex = /\[([^\]]+)\]\(entity:([^)]+)\)/g;
	let match: RegExpExecArray | null = entityLinkRegex.exec(docText);

	while (match !== null) {
		const start = match.index;
		const end = start + match[0].length;
		const line = doc.lineAt(start);
		const isActive = activeLines.has(line.number);

		const displayName = match[1];
		const entityId = match[2];

		if (!isActive) {
			decorations.push({
				from: start,
				to: end,
				deco: Decoration.replace({ widget: new EntityLinkWidget(displayName, entityId) }),
			});
		} else {
			// When active, show the full syntax with dim markers
			decorations.push({
				from: start,
				to: start + 1, // [
				deco: Decoration.mark({ class: "cm-syntax-dim" }),
			});
			decorations.push({
				from: start + 1,
				to: start + 1 + displayName.length, // Display Name
				deco: Decoration.mark({ class: "cm-entity-link-raw" }),
			});
			decorations.push({
				from: start + 1 + displayName.length,
				to: start + 1 + displayName.length + 9, // ](entity:
				deco: Decoration.mark({ class: "cm-syntax-dim" }),
			});
			decorations.push({
				from: start + 1 + displayName.length + 9,
				to: end - 1, // entity_id
				deco: Decoration.mark({ class: "cm-syntax-dim" }),
			});
			decorations.push({
				from: end - 1,
				to: end, // )
				deco: Decoration.mark({ class: "cm-syntax-dim" }),
			});
		}
		match = entityLinkRegex.exec(docText);
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
	decorateEntityLinks(view, activeLines, decorations);

	// Sort decorations by position
	decorations.sort((a, b) => {
		if (a.from !== b.from) return a.from - b.from;
		if (a.from === a.to && b.from !== b.to) return -1;
		if (b.from === b.to && a.from !== a.to) return 1;
		return a.to - b.to;
	});

	// Build decoration set
	const builder = new RangeSetBuilder<Decoration>();
	let lastTo = 0;
	let lastLineNumber = -1;

	for (const { from, to, deco } of decorations) {
		if (from < 0 || to > view.state.doc.length) continue;
		if (from > to) continue;

		const fromLine = view.state.doc.lineAt(from);
		const currentLineNumber = fromLine.number;
		
		if (currentLineNumber !== lastLineNumber) {
			lastTo = fromLine.from;
			lastLineNumber = currentLineNumber;
		}

		if (from < lastTo && from !== to && from !== lastTo && from !== fromLine.from) {
			continue;
		}

		if (from !== to) {
			const lineEnd = fromLine.to;
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
