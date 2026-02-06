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
import { RangeSetBuilder, StateField, StateEffect, type EditorState } from "@codemirror/state";
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
 * Get icon name based on URL pattern (everything is a URL)
 */
function getEntityIconFromUrl(url: string): string {
	if (url.startsWith("/person/")) return "ri:user-line";
	if (url.startsWith("/place/")) return "ri:map-pin-line";
	if (url.startsWith("/org/")) return "ri:building-line";
	if (url.startsWith("/thing/")) return "ri:box-3-line";
	if (url.startsWith("/page/")) return "ri:file-text-line";
	if (url.startsWith("/day/")) return "ri:calendar-line";
	if (url.startsWith("/year/")) return "ri:calendar-2-line";
	if (url.startsWith("/source/")) return "ri:database-2-line";
	if (url.startsWith("/chat/")) return "ri:chat-3-line";
	if (url.startsWith("/drive/")) return "ri:file-line";
	return "ri:links-line";
}

/**
 * Check if URL is an entity link (internal app route)
 */
function isEntityUrl(url: string): boolean {
	const entityPrefixes = [
		"/person/", "/place/", "/thing/", "/org/", "/page/",
		"/day/", "/year/", "/source/", "/chat/", "/drive/"
	];
	return entityPrefixes.some(prefix => url.startsWith(prefix));
}

/**
 * Get file extension from filename/path
 */
function getFileExtension(name: string): string {
	const ext = name.split('.').pop()?.toLowerCase() || '';
	return ext;
}

/**
 * Detect media type from filename extension
 */
function getMediaType(name: string): 'image' | 'audio' | 'video' | null {
	const ext = getFileExtension(name);

	const imageExts = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico'];
	const audioExts = ['mp3', 'wav', 'm4a', 'ogg', 'flac', 'aac', 'wma'];
	const videoExts = ['mp4', 'mov', 'webm', 'avi', 'mkv', 'm4v', 'wmv'];

	if (imageExts.includes(ext)) return 'image';
	if (audioExts.includes(ext)) return 'audio';
	if (videoExts.includes(ext)) return 'video';

	return null;
}

// =============================================================================
// WIDGET CLASSES
// =============================================================================

/**
 * Entity link widget: [Display Name](/type/id)
 * Renders as clickable chip with icon, dispatches custom event for SvelteKit navigation
 * Used for internal entity URLs like /person/xxx, /page/xxx, /drive/xxx
 */
class EntityLinkWidget extends WidgetType {
	constructor(
		readonly displayName: string,
		readonly url: string
	) {
		super();
	}

	toDOM() {
		const link = document.createElement("a");
		link.className = "cm-entity-link";

		// Create icon element
		const iconSpan = document.createElement("span");
		iconSpan.className = "cm-entity-icon";

		// Use iconify-icon for the icon (determined by URL pattern)
		const icon = document.createElement("iconify-icon");
		icon.setAttribute("icon", getEntityIconFromUrl(this.url));
		icon.setAttribute("width", "14");
		iconSpan.appendChild(icon);
		link.appendChild(iconSpan);

		// Create text element
		const text = document.createElement("span");
		text.className = "cm-entity-text";
		text.textContent = this.displayName;
		link.appendChild(text);

		link.href = this.url;
		link.addEventListener("click", (e) => {
			e.preventDefault();
			e.stopPropagation();
			link.dispatchEvent(
				new CustomEvent("page-navigate", {
					bubbles: true,
					detail: { href: this.url },
				})
			);
		});
		return link;
	}

	eq(other: EntityLinkWidget) {
		return other.displayName === this.displayName && other.url === this.url;
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
 * Audio player widget: ![audio.mp3](/drive/file_id)
 * Renders inline audio player with controls
 */
class AudioPlayerWidget extends WidgetType {
	constructor(
		readonly src: string,
		readonly name: string
	) {
		super();
	}

	toDOM() {
		const wrapper = document.createElement("div");
		wrapper.className = "cm-audio-wrapper";

		// Create header with icon and name
		const header = document.createElement("div");
		header.className = "cm-audio-header";

		const icon = document.createElement("iconify-icon");
		icon.setAttribute("icon", "ri:music-2-line");
		icon.setAttribute("width", "16");
		header.appendChild(icon);

		const nameSpan = document.createElement("span");
		nameSpan.className = "cm-audio-name";
		nameSpan.textContent = this.name;
		header.appendChild(nameSpan);

		wrapper.appendChild(header);

		// Create audio element
		const audio = document.createElement("audio");
		audio.className = "cm-audio-player";
		audio.src = this.src;
		audio.controls = true;
		audio.preload = "metadata";

		wrapper.appendChild(audio);
		return wrapper;
	}

	eq(other: AudioPlayerWidget) {
		return other.src === this.src && other.name === this.name;
	}

	ignoreEvent() {
		return true;
	}
}

/**
 * Video player widget: ![video.mp4](/drive/file_id)
 * Renders inline video player with controls
 */
class VideoPlayerWidget extends WidgetType {
	constructor(
		readonly src: string,
		readonly name: string
	) {
		super();
	}

	toDOM() {
		const wrapper = document.createElement("div");
		wrapper.className = "cm-video-wrapper";

		// Create video element
		const video = document.createElement("video");
		video.className = "cm-video-player";
		video.src = this.src;
		video.controls = true;
		video.preload = "metadata";

		wrapper.appendChild(video);
		return wrapper;
	}

	eq(other: VideoPlayerWidget) {
		return other.src === this.src && other.name === this.name;
	}

	ignoreEvent() {
		return true;
	}
}

/**
 * File card widget: [document.pdf](/drive/file_id)
 * Renders as a styled file card for non-media files
 */
class FileCardWidget extends WidgetType {
	constructor(
		readonly url: string,
		readonly name: string
	) {
		super();
	}

	toDOM() {
		const wrapper = document.createElement("a");
		wrapper.className = "cm-file-card";
		wrapper.href = this.url;

		// Get icon based on extension
		const ext = getFileExtension(this.name);
		let iconName = "ri:file-line";
		if (['pdf'].includes(ext)) iconName = "ri:file-pdf-line";
		else if (['doc', 'docx'].includes(ext)) iconName = "ri:file-word-line";
		else if (['xls', 'xlsx'].includes(ext)) iconName = "ri:file-excel-line";
		else if (['ppt', 'pptx'].includes(ext)) iconName = "ri:file-ppt-line";
		else if (['zip', 'rar', 'tar', 'gz', '7z'].includes(ext)) iconName = "ri:file-zip-line";
		else if (['txt', 'md'].includes(ext)) iconName = "ri:file-text-line";
		else if (['js', 'ts', 'py', 'rs', 'go', 'java', 'c', 'cpp', 'h'].includes(ext)) iconName = "ri:file-code-line";

		const icon = document.createElement("iconify-icon");
		icon.setAttribute("icon", iconName);
		icon.setAttribute("width", "20");
		wrapper.appendChild(icon);

		const nameSpan = document.createElement("span");
		nameSpan.className = "cm-file-card-name";
		nameSpan.textContent = this.name;
		wrapper.appendChild(nameSpan);

		wrapper.addEventListener("click", (e) => {
			e.preventDefault();
			e.stopPropagation();
			wrapper.dispatchEvent(
				new CustomEvent("page-navigate", {
					bubbles: true,
					detail: { href: this.url },
				})
			);
		});

		return wrapper;
	}

	eq(other: FileCardWidget) {
		return other.url === this.url && other.name === this.name;
	}

	ignoreEvent() {
		return true;
	}
}

/**
 * Bullet widget: renders - as dash, * as •
 */
class BulletWidget extends WidgetType {
	constructor(readonly marker: string) {
		super();
	}

	toDOM() {
		const span = document.createElement("span");
		span.className = "cm-list-bullet-rendered";
		// Convert * and + to bullet dot, keep - as dash
		span.textContent = this.marker === "-" ? "–" : "•";
		return span;
	}

	eq(other: BulletWidget) {
		return other.marker === this.marker;
	}

	ignoreEvent() {
		return false;
	}
}

/**
 * Checkbox widget: renders [ ] as unchecked, [x] as checked
 */
class CheckboxWidget extends WidgetType {
	constructor(
		readonly checked: boolean,
		readonly pos: number
	) {
		super();
	}

	toDOM(view: EditorView) {
		const checkbox = document.createElement("input");
		checkbox.type = "checkbox";
		checkbox.className = "cm-checkbox";
		checkbox.checked = this.checked;

		// Toggle checkbox on click
		checkbox.addEventListener("mousedown", (e) => {
			e.preventDefault();
			e.stopPropagation();
			const newState = !this.checked;
			const newText = newState ? "[x]" : "[ ]";
			view.dispatch({
				changes: { from: this.pos, to: this.pos + 3, insert: newText },
			});
		});

		return checkbox;
	}

	eq(other: CheckboxWidget) {
		return other.checked === this.checked && other.pos === this.pos;
	}

	ignoreEvent(e: Event) {
		// Handle mouse events on the checkbox, let others through
		return e.type === "mousedown" || e.type === "click";
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

/**
 * Table widget: renders markdown tables as HTML tables
 * Parses pipe-delimited table syntax and renders with styling matching chat markdown
 */
class TableWidget extends WidgetType {
	constructor(
		readonly tableText: string,
		readonly from: number
	) {
		super();
	}

	toDOM(view: EditorView) {
		const wrapper = document.createElement("div");
		wrapper.className = "cm-table-wrapper";

		// Click handler to enter edit mode
		wrapper.addEventListener("mousedown", (e) => {
			e.preventDefault();
			// Position cursor at the start of the table to reveal raw markdown
			view.dispatch({
				selection: { anchor: this.from },
			});
			view.focus();
		});

		const table = document.createElement("table");
		table.className = "cm-table";

		const lines = this.tableText.split("\n").filter((line) => line.trim());
		if (lines.length < 2) {
			// Not a valid table (need header + separator at minimum)
			wrapper.textContent = this.tableText;
			return wrapper;
		}

		// Parse header row
		const headerCells = this.parseRow(lines[0]);

		// Check if second line is separator (contains only |, -, :, and spaces)
		const isSeparator = /^[\s|:\-]+$/.test(lines[1]);
		if (!isSeparator) {
			wrapper.textContent = this.tableText;
			return wrapper;
		}

		// Parse alignment from separator row
		const alignments = this.parseAlignments(lines[1]);

		// Create thead
		const thead = document.createElement("thead");
		const headerRow = document.createElement("tr");
		headerCells.forEach((cell, i) => {
			const th = document.createElement("th");
			th.textContent = cell.trim();
			if (alignments[i]) {
				th.style.textAlign = alignments[i];
			}
			headerRow.appendChild(th);
		});
		thead.appendChild(headerRow);
		table.appendChild(thead);

		// Create tbody with remaining rows
		if (lines.length > 2) {
			const tbody = document.createElement("tbody");
			for (let i = 2; i < lines.length; i++) {
				const cells = this.parseRow(lines[i]);
				const row = document.createElement("tr");
				cells.forEach((cell, j) => {
					const td = document.createElement("td");
					td.textContent = cell.trim();
					if (alignments[j]) {
						td.style.textAlign = alignments[j];
					}
					row.appendChild(td);
				});
				tbody.appendChild(row);
			}
			table.appendChild(tbody);
		}

		wrapper.appendChild(table);
		return wrapper;
	}

	parseRow(line: string): string[] {
		// Remove leading/trailing pipes and split
		const trimmed = line.replace(/^\|/, "").replace(/\|$/, "");
		return trimmed.split("|");
	}

	parseAlignments(separator: string): string[] {
		const cells = this.parseRow(separator);
		return cells.map((cell) => {
			const trimmed = cell.trim();
			const leftColon = trimmed.startsWith(":");
			const rightColon = trimmed.endsWith(":");
			if (leftColon && rightColon) return "center";
			if (rightColon) return "right";
			if (leftColon) return "left";
			return "";
		});
	}

	eq(other: TableWidget) {
		return other.tableText === this.tableText && other.from === this.from;
	}

	ignoreEvent() {
		return false;
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
	"& .cm-selectionLayer .cm-selectionBackground": {
		backgroundColor: "var(--color-highlight) !important",
	},
	"&.cm-focused .cm-selectionLayer .cm-selectionBackground": {
		backgroundColor: "var(--color-highlight) !important",
	},
	".cm-content ::selection": {
		backgroundColor: "var(--color-highlight) !important",
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
	".cm-list-line": {
		paddingLeft: "8px !important",
	},
	".cm-list-bullet": {
		color: "var(--color-foreground-muted)",
		paddingRight: "4px",
	},
	".cm-list-bullet-rendered": {
		color: "var(--color-foreground-muted)",
		paddingRight: "4px",
		display: "inline-block",
		width: "1ch",
		textAlign: "center",
	},
	".cm-list-number": {
		color: "var(--color-foreground-muted)",
		paddingRight: "4px",
		fontVariantNumeric: "tabular-nums",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// TODO CHECKBOXES
	// ─────────────────────────────────────────────────────────────────────────
	".cm-todo-line": {
		paddingLeft: "8px !important",
	},
	".cm-todo-checked": {
		color: "var(--color-foreground-muted)",
		textDecoration: "line-through",
		textDecorationColor: "var(--color-border)",
	},
	".cm-checkbox": {
		appearance: "none",
		width: "14px",
		height: "14px",
		border: "1.5px solid var(--color-border)",
		borderRadius: "3px",
		backgroundColor: "transparent",
		cursor: "pointer",
		verticalAlign: "middle",
		marginRight: "6px",
		position: "relative",
		transition: "all 0.15s ease",
	},
	".cm-checkbox:hover": {
		borderColor: "var(--color-primary)",
	},
	".cm-checkbox:checked": {
		backgroundColor: "var(--color-primary)",
		borderColor: "var(--color-primary)",
	},
	".cm-checkbox:checked::after": {
		content: '""',
		position: "absolute",
		left: "4px",
		top: "1px",
		width: "4px",
		height: "8px",
		border: "solid white",
		borderWidth: "0 2px 2px 0",
		transform: "rotate(45deg)",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// AUDIO PLAYER
	// ─────────────────────────────────────────────────────────────────────────
	".cm-audio-wrapper": {
		display: "block",
		margin: "0.5rem 0",
		padding: "0.75rem",
		backgroundColor: "var(--color-surface)",
		borderRadius: "0.5rem",
		border: "1px solid var(--color-border)",
	},
	".cm-audio-header": {
		display: "flex",
		alignItems: "center",
		gap: "0.5rem",
		marginBottom: "0.5rem",
		color: "var(--color-foreground-muted)",
	},
	".cm-audio-name": {
		fontSize: "0.875rem",
		fontWeight: "500",
		color: "var(--color-foreground)",
	},
	".cm-audio-player": {
		width: "100%",
		height: "32px",
		outline: "none",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// VIDEO PLAYER
	// ─────────────────────────────────────────────────────────────────────────
	".cm-video-wrapper": {
		display: "block",
		margin: "0.5rem 0",
	},
	".cm-video-player": {
		width: "100%",
		maxWidth: "100%",
		height: "auto",
		borderRadius: "0.5rem",
		backgroundColor: "var(--color-surface)",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// FILE CARD
	// ─────────────────────────────────────────────────────────────────────────
	".cm-file-card": {
		display: "inline-flex",
		alignItems: "center",
		gap: "0.5rem",
		padding: "0.5rem 0.75rem",
		backgroundColor: "var(--color-surface)",
		border: "1px solid var(--color-border)",
		borderRadius: "0.5rem",
		color: "var(--color-foreground)",
		textDecoration: "none",
		cursor: "pointer",
		transition: "background-color 0.15s ease, border-color 0.15s ease",
	},
	".cm-file-card:hover": {
		backgroundColor: "var(--color-surface-elevated)",
		borderColor: "var(--color-primary)",
	},
	".cm-file-card-name": {
		fontSize: "0.875rem",
		fontWeight: "500",
	},

	// ─────────────────────────────────────────────────────────────────────────
	// TABLES
	// ─────────────────────────────────────────────────────────────────────────
	".cm-table-wrapper": {
		display: "block",
		margin: "0.5rem 0",
		overflowX: "auto",
	},
	".cm-table": {
		width: "100%",
		fontSize: "0.875rem",
		borderCollapse: "separate",
		borderSpacing: "0",
	},
	".cm-table thead": {
		backgroundColor: "var(--color-surface-elevated)",
	},
	".cm-table th": {
		padding: "0.5rem 1rem",
		textAlign: "left",
		fontSize: "0.875rem",
		fontWeight: "400",
		fontFamily: "var(--font-serif, Georgia, serif)",
		borderRight: "1px solid var(--color-border-subtle)",
		borderBottom: "1px solid var(--color-border-subtle)",
	},
	".cm-table th:last-child": {
		borderRight: "none",
	},
	".cm-table td": {
		padding: "0.5rem 1rem",
		fontSize: "0.875rem",
		borderRight: "1px solid var(--color-border-subtle)",
		borderBottom: "1px solid var(--color-border-subtle)",
	},
	".cm-table td:last-child": {
		borderRight: "none",
	},
	".cm-table tr:last-child td": {
		borderBottom: "none",
	},
	".cm-table tr:hover": {
		backgroundColor: "var(--color-background)",
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
 * Decorate ATX headings (## Heading) and Setext headings (underlined)
 * Uses regex-based detection to work even without blank lines above headings
 */
function decorateHeadings(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const decoratedLines = new Set<number>();

	// Regex-based ATX heading detection (works regardless of blank lines)
	for (let i = 1; i <= doc.lines; i++) {
		const line = doc.line(i);
		// Match # followed by space (standard) or # followed by text (lenient)
		const match = line.text.match(/^(#{1,6})(\s+|\s*$)/);

		if (match) {
			const level = match[1].length;
			const isActive = activeLines.has(i);
			decoratedLines.add(i);

			// Always add line styling
			decorations.push({
				from: line.from,
				to: line.from,
				deco: Decoration.line({ class: `cm-heading-line cm-heading-${level}` }),
			});

			// Hide the # markers when not on this line
			if (!isActive && match[0].length > 0) {
				decorations.push({
					from: line.from,
					to: line.from + match[0].length,
					deco: Decoration.replace({}),
				});
			}
		}
	}

	// Also check syntax tree for Setext headings (underlined style)
	const tree = syntaxTree(view.state);
	tree.iterate({
		enter(node) {
			if (node.name === "SetextHeading1" || node.name === "SetextHeading2") {
				const level = node.name === "SetextHeading1" ? 1 : 2;
				const startLine = doc.lineAt(node.from);
				const endLine = doc.lineAt(node.to);

				// Skip if already decorated by regex
				if (decoratedLines.has(startLine.number)) return;

				const isHeadingLineActive = activeLines.has(startLine.number);
				const isUnderlineActive = activeLines.has(endLine.number);

				decorations.push({
					from: startLine.from,
					to: startLine.from,
					deco: Decoration.line({ class: `cm-heading-line cm-heading-${level}` }),
				});

				if (!isHeadingLineActive && !isUnderlineActive && endLine.number !== startLine.number) {
					decorations.push({
						from: endLine.from,
						to: endLine.to,
						deco: Decoration.replace({}),
					});
				} else if (endLine.number !== startLine.number) {
					decorations.push({
						from: endLine.from,
						to: endLine.to,
						deco: Decoration.mark({ class: "cm-syntax-dim" }),
					});
				}
			}
		},
	});
}

/**
 * Decorate list items (- item, * item, 1. item, - [ ] todo)
 */
function decorateLists(
	view: EditorView,
	activeLines: Set<number>,
	decorations: DecorationEntry[]
) {
	const doc = view.state.doc;
	const tree = syntaxTree(view.state);

	tree.iterate({
		enter(node) {
			// BulletList contains ListItem nodes
			if (node.name === "ListItem") {
				const line = doc.lineAt(node.from);
				const isActive = activeLines.has(line.number);

				// Check for todo item: - [ ] or - [x] or * [ ] etc
				const todoMatch = line.text.match(/^(\s*)([-*+])\s\[([ xX])\]\s/);
				if (todoMatch) {
					const isChecked = todoMatch[3].toLowerCase() === "x";
					const markerStart = line.from + todoMatch[1].length;
					const checkboxStart = markerStart + 2; // after "- "
					const checkboxEnd = checkboxStart + 3; // "[ ]" or "[x]"

					// Add line decoration for todo item
					decorations.push({
						from: line.from,
						to: line.from,
						deco: Decoration.line({
							class: `cm-list-line cm-todo-line${isChecked ? " cm-todo-checked" : ""}`,
						}),
					});

					if (!isActive) {
						// Hide the bullet marker
						decorations.push({
							from: markerStart,
							to: markerStart + 2, // "- " or "* "
							deco: Decoration.replace({}),
						});
						// Replace checkbox with interactive widget
						decorations.push({
							from: checkboxStart,
							to: checkboxEnd,
							deco: Decoration.replace({
								widget: new CheckboxWidget(isChecked, checkboxStart),
							}),
						});
					} else {
						// Dim the syntax when active
						decorations.push({
							from: markerStart,
							to: markerStart + 1,
							deco: Decoration.mark({ class: "cm-list-bullet" }),
						});
						decorations.push({
							from: checkboxStart,
							to: checkboxEnd,
							deco: Decoration.mark({ class: "cm-syntax-dim" }),
						});
					}
					return; // Don't process as regular bullet
				}

				// Add line decoration for list item padding
				decorations.push({
					from: line.from,
					to: line.from,
					deco: Decoration.line({ class: "cm-list-line" }),
				});

				// Check for bullet marker (-, *, +)
				const bulletMatch = line.text.match(/^(\s*)([-*+])\s/);
				if (bulletMatch) {
					const markerStart = line.from + bulletMatch[1].length;
					const markerEnd = markerStart + 1;
					const marker = bulletMatch[2];

					if (!isActive) {
						// Replace with styled bullet widget
						decorations.push({
							from: markerStart,
							to: markerEnd,
							deco: Decoration.replace({ widget: new BulletWidget(marker) }),
						});
					} else {
						// Just mark it when active
						decorations.push({
							from: markerStart,
							to: markerEnd,
							deco: Decoration.mark({ class: "cm-list-bullet" }),
						});
					}
				}

				// Check for ordered list marker (1., 2., etc)
				const orderedMatch = line.text.match(/^(\s*)(\d+\.)\s/);
				if (orderedMatch) {
					const markerStart = line.from + orderedMatch[1].length;
					const markerEnd = markerStart + orderedMatch[2].length;
					decorations.push({
						from: markerStart,
						to: markerEnd,
						deco: Decoration.mark({ class: "cm-list-number" }),
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
 * - External URLs (http/https) → ExternalLinkWidget
 * - Drive file URLs (/drive/xxx) → FileCardWidget (file card with icon)
 * - Other entity URLs (/person/, /page/, etc.) → EntityLinkWidget (styled chip)
 * - Other internal URLs → InternalLinkWidget
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
						// External link (http/https)
						decorations.push({
							from: node.from,
							to: node.to,
							deco: Decoration.replace({ widget: new ExternalLinkWidget(linkText, url) }),
						});
					} else if (url.startsWith("/drive/")) {
						// Drive file link - render as file card
						decorations.push({
							from: node.from,
							to: node.to,
							deco: Decoration.replace({ widget: new FileCardWidget(url, linkText) }),
						});
					} else if (isEntityUrl(url)) {
						// Entity link (/person/, /page/, etc.) - render as styled chip
						decorations.push({
							from: node.from,
							to: node.to,
							deco: Decoration.replace({ widget: new EntityLinkWidget(linkText, url) }),
						});
					} else {
						// Other internal link
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
 * Decorate images/media ![alt](url)
 * Detects media type by extension in the alt text (display name):
 * - .jpg/.png/etc → ImageWidget
 * - .mp3/.wav/etc → AudioPlayerWidget
 * - .mp4/.mov/etc → VideoPlayerWidget
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
					// Detect media type from the alt text (which contains filename with extension)
					const mediaType = getMediaType(alt);

					if (mediaType === 'audio') {
						decorations.push({
							from: node.from,
							to: node.to,
							deco: Decoration.replace({ widget: new AudioPlayerWidget(src, alt) }),
						});
					} else if (mediaType === 'video') {
						decorations.push({
							from: node.from,
							to: node.to,
							deco: Decoration.replace({ widget: new VideoPlayerWidget(src, alt) }),
						});
					} else {
						// Default to image (including when no extension detected)
						decorations.push({
							from: node.from,
							to: node.to,
							deco: Decoration.replace({ widget: new ImageWidget(src, alt) }),
						});
					}
				}
			}
		},
	});
}

// =============================================================================
// MAIN DECORATION BUILDER
// =============================================================================

function buildDecorations(view: EditorView): DecorationSet {
	const decorations: DecorationEntry[] = [];
	const activeLines = getActiveLines(view);

	// Run all decoration builders
	decorateHeadings(view, activeLines, decorations);
	decorateLists(view, activeLines, decorations);
	decorateEmphasis(view, activeLines, decorations);
	decorateInlineCode(view, activeLines, decorations);
	decorateBlockquotes(view, activeLines, decorations);
	decorateCodeBlocks(view, activeLines, decorations);
	decorateHorizontalRules(view, activeLines, decorations);
	decorateLinks(view, activeLines, decorations);
	decorateImages(view, activeLines, decorations);
	// Note: Tables are handled by a separate StateField (tableDecorationField)
	// because multi-line replacements cannot be done via ViewPlugin

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

// =============================================================================
// TABLE DECORATION STATE FIELD
// =============================================================================
// Tables must use a StateField instead of ViewPlugin because they span
// multiple lines, and CodeMirror doesn't allow ViewPlugin decorations
// to replace content that crosses line boundaries.

/**
 * Effect to update focus state for table decorations
 */
const setTableFocus = StateEffect.define<boolean>();

/**
 * Get active lines for table decoration (similar to getActiveLines but for state)
 */
function getActiveLinesFromState(state: EditorState, hasFocus: boolean): Set<number> {
	if (!hasFocus) {
		return new Set<number>();
	}

	const lines = new Set<number>();
	for (const range of state.selection.ranges) {
		const startLine = state.doc.lineAt(range.from).number;
		const endLine = state.doc.lineAt(range.to).number;
		for (let i = startLine; i <= endLine; i++) {
			lines.add(i);
		}
	}
	return lines;
}

/**
 * Build table decorations from state
 */
function buildTableDecorations(state: EditorState, hasFocus: boolean): DecorationSet {
	const builder = new RangeSetBuilder<Decoration>();
	const doc = state.doc;
	const tree = syntaxTree(state);
	const activeLines = getActiveLinesFromState(state, hasFocus);

	const tables: { from: number; to: number; text: string }[] = [];

	tree.iterate({
		enter(node) {
			if (node.name === "Table") {
				const startLine = doc.lineAt(node.from);
				const endLine = doc.lineAt(node.to);

				// Check if any line in the table is active
				let hasActiveLine = false;
				for (let lineNum = startLine.number; lineNum <= endLine.number; lineNum++) {
					if (activeLines.has(lineNum)) {
						hasActiveLine = true;
						break;
					}
				}

				// Only add decoration if no lines are being edited
				if (!hasActiveLine) {
					const tableText = state.sliceDoc(node.from, node.to);
					tables.push({ from: node.from, to: node.to, text: tableText });
				}
			}
		},
	});

	// Sort by position and add to builder
	tables.sort((a, b) => a.from - b.from);
	for (const { from, to, text } of tables) {
		builder.add(from, to, Decoration.replace({ widget: new TableWidget(text, from) }));
	}

	return builder.finish();
}

/**
 * StateField for table decorations
 * Required because multi-line replacements cannot be done via ViewPlugin
 */
export const tableDecorationField = StateField.define<{ decorations: DecorationSet; hasFocus: boolean }>({
	create(state) {
		// Initially assume no focus (decorations will show)
		return { decorations: buildTableDecorations(state, false), hasFocus: false };
	},
	update(value, tr) {
		// Check if focus changed via effect
		let hasFocus = value.hasFocus;
		for (const effect of tr.effects) {
			if (effect.is(setTableFocus)) {
				hasFocus = effect.value;
			}
		}

		// Rebuild on doc change, selection change, or focus change
		if (tr.docChanged || tr.selection || hasFocus !== value.hasFocus) {
			return {
				decorations: buildTableDecorations(tr.state, hasFocus),
				hasFocus
			};
		}

		// Map existing decorations through changes
		return {
			decorations: value.decorations.map(tr.changes),
			hasFocus
		};
	},
	provide(field) {
		return EditorView.decorations.from(field, (value) => value.decorations);
	},
});

/**
 * Facet to dispatch focus effect when editor focus changes
 * This is the CodeMirror-recommended way to handle focus changes with StateField
 */
const tableFocusEffect = EditorView.focusChangeEffect.of((_state, focusing) => {
	return setTableFocus.of(focusing);
});

export const livePreviewExtension = [livePreviewPlugin, tableDecorationField, tableFocusEffect];
