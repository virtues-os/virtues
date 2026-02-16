/**
 * Link Decorations — URL-aware rendering
 *
 * Renders [label](url) markdown links with different widgets based on URL type:
 * - Entity links (/person/, /page/, etc.) → pill chip with type-specific icon
 * - External links (https://...) → favicon + text with underline on hover
 * - Internal links (other / paths) → simple colored link
 *
 * Right-click context menu on all link types: Go to, Copy, Turn into embed, Edit, Remove.
 *
 * Click behavior:
 * - Internal links dispatch a page-navigate custom event
 * - External links open in a new tab
 */

import type { Extension, Range } from '@codemirror/state';
import { Decoration, type DecorationSet, type EditorView, ViewPlugin, type ViewUpdate, WidgetType } from '@codemirror/view';
import { contextMenu } from '$lib/stores/contextMenu.svelte';

// =============================================================================
// URL Classification
// =============================================================================

const ENTITY_PREFIXES = [
	'/person/', '/page/', '/org/', '/place/',
	'/day/', '/year/', '/source/', '/chat/', '/drive/',
] as const;

const ENTITY_ICON_MAP: Record<string, string> = {
	'/person/': 'ri:user-line',
	'/page/': 'ri:file-text-line',
	'/org/': 'ri:building-line',
	'/place/': 'ri:map-pin-line',
	'/day/': 'ri:calendar-line',
	'/year/': 'ri:calendar-2-line',
	'/source/': 'ri:database-2-line',
	'/chat/': 'ri:chat-3-line',
	'/drive/': 'ri:file-line',
};

function getEntityIcon(url: string): string {
	for (const prefix of ENTITY_PREFIXES) {
		if (url.startsWith(prefix)) return ENTITY_ICON_MAP[prefix];
	}
	return 'ri:links-line';
}

function isEntityUrl(url: string): boolean {
	return ENTITY_PREFIXES.some(p => url.startsWith(p));
}

function isExternalUrl(url: string): boolean {
	return url.startsWith('http://') || url.startsWith('https://');
}

function getDomain(url: string): string {
	try { return new URL(url).hostname; }
	catch { return ''; }
}

function createGlobeSvg(): SVGElement {
	const ns = 'http://www.w3.org/2000/svg';
	const svg = document.createElementNS(ns, 'svg');
	svg.setAttribute('viewBox', '0 0 24 24');
	svg.setAttribute('fill', 'none');
	svg.setAttribute('stroke', 'currentColor');
	svg.setAttribute('stroke-width', '1.8');
	svg.setAttribute('stroke-linecap', 'round');
	svg.setAttribute('stroke-linejoin', 'round');

	const circle = document.createElementNS(ns, 'circle');
	circle.setAttribute('cx', '12');
	circle.setAttribute('cy', '12');
	circle.setAttribute('r', '10');

	const meridian = document.createElementNS(ns, 'path');
	meridian.setAttribute('d', 'M2 12h20');

	const longitudes = document.createElementNS(ns, 'path');
	longitudes.setAttribute('d', 'M12 2c3 3 3 17 0 20M12 2c-3 3-3 17 0 20');

	svg.appendChild(circle);
	svg.appendChild(meridian);
	svg.appendChild(longitudes);
	return svg;
}

// =============================================================================
// Context Menu
// =============================================================================

function showLinkContextMenu(
	e: MouseEvent,
	view: EditorView,
	from: number,
	to: number,
	href: string,
	isExternal: boolean,
) {
	e.preventDefault();
	e.stopPropagation();

	contextMenu.show({ x: e.clientX, y: e.clientY }, [
		{
			id: 'go-to',
			label: 'Go to',
			icon: 'ri:arrow-right-up-line',
			action: () => {
				if (isExternal) {
					window.open(href, '_blank', 'noopener');
				} else {
					view.dom.dispatchEvent(
						new CustomEvent('page-navigate', {
							bubbles: true,
							detail: { href },
						})
					);
				}
			},
		},
		{
			id: 'open-new-tab',
			label: 'Open in New Tab',
			icon: 'ri:external-link-line',
			action: () => {
				window.open(href, '_blank', 'noopener');
			},
		},
		{
			id: 'copy-link',
			label: 'Copy link',
			icon: 'ri:file-copy-line',
			action: () => {
				const fullUrl = isExternal ? href : `${window.location.origin}${href}`;
				navigator.clipboard.writeText(fullUrl);
			},
		},
		{
			id: 'turn-into-embed',
			label: 'Turn into embed',
			icon: 'ri:image-line',
			dividerBefore: true,
			action: () => {
				view.dispatch({ changes: { from, to: from, insert: '!' } });
			},
		},
		{
			id: 'edit',
			label: 'Edit',
			icon: 'ri:edit-line',
			action: () => {
				view.dispatch({ selection: { anchor: from } });
				view.focus();
			},
		},
		{
			id: 'remove',
			label: 'Remove',
			icon: 'ri:delete-bin-line',
			variant: 'destructive' as const,
			action: () => {
				view.dispatch({ changes: { from, to, insert: '' } });
			},
		},
	]);
}

// =============================================================================
// Widget Classes
// =============================================================================

/**
 * Entity link pill: [Label](/person/id), [@Label](/page/id), etc.
 * Rendered as pill chip with type-specific icon.
 */
class EntityLinkWidget extends WidgetType {
	constructor(
		private label: string,
		private href: string,
		private from: number,
		private to: number,
	) {
		super();
	}

	toDOM(view: EditorView) {
		const chip = document.createElement('a');
		chip.className = 'cm-entity-link';
		chip.setAttribute('href', this.href);

		const iconSpan = document.createElement('span');
		iconSpan.className = 'cm-entity-icon';
		const icon = document.createElement('iconify-icon');
		icon.setAttribute('icon', getEntityIcon(this.href));
		icon.setAttribute('width', '14');
		iconSpan.appendChild(icon);
		chip.appendChild(iconSpan);

		const text = document.createElement('span');
		text.className = 'cm-entity-text';
		text.textContent = this.label;
		chip.appendChild(text);

		chip.addEventListener('click', (e) => {
			e.preventDefault();
			e.stopPropagation();
			chip.dispatchEvent(
				new CustomEvent('page-navigate', {
					bubbles: true,
					detail: { href: this.href },
				})
			);
		});

		chip.addEventListener('contextmenu', (e) => {
			showLinkContextMenu(e, view, this.from, this.to, this.href, false);
		});

		return chip;
	}

	eq(other: EntityLinkWidget) {
		return other.label === this.label && other.href === this.href;
	}

	ignoreEvent() { return false; }
}

/**
 * External link: [text](https://example.com)
 * Rendered with Google favicon (globe SVG fallback) + text.
 */
class ExternalLinkWidget extends WidgetType {
	constructor(
		private label: string,
		private href: string,
		private from: number,
		private to: number,
	) {
		super();
	}

	toDOM(view: EditorView) {
		const link = document.createElement('a');
		link.className = 'cm-external-link';
		link.href = this.href;
		link.target = '_blank';
		link.rel = 'noopener noreferrer';

		const iconSpan = document.createElement('span');
		iconSpan.className = 'cm-external-icon';
		iconSpan.appendChild(createGlobeSvg());

		const domain = getDomain(this.href);
		if (domain) {
			const favicon = document.createElement('img');
			favicon.className = 'cm-external-favicon';
			favicon.src = `https://www.google.com/s2/favicons?domain=${domain}&sz=16`;
			favicon.width = 12;
			favicon.height = 12;
			favicon.alt = '';
			favicon.loading = 'lazy';
			favicon.referrerPolicy = 'no-referrer';
			favicon.decoding = 'async';
			favicon.onload = () => {
				try { iconSpan.replaceChildren(favicon); } catch { /* ignore */ }
			};
		}

		link.appendChild(iconSpan);

		const text = document.createElement('span');
		text.className = 'cm-link-text';
		text.textContent = this.label;
		link.appendChild(text);

		link.addEventListener('contextmenu', (e) => {
			showLinkContextMenu(e, view, this.from, this.to, this.href, true);
		});

		return link;
	}

	eq(other: ExternalLinkWidget) {
		return other.label === this.label && other.href === this.href;
	}

	ignoreEvent() { return false; }
}

/**
 * Internal link: [text](/some/path)
 * Simple colored link that dispatches page-navigate on click.
 */
class InternalLinkWidget extends WidgetType {
	constructor(
		private label: string,
		private href: string,
		private from: number,
		private to: number,
	) {
		super();
	}

	toDOM(view: EditorView) {
		const link = document.createElement('a');
		link.className = 'cm-internal-link';
		link.href = this.href;

		const text = document.createElement('span');
		text.className = 'cm-link-text';
		text.textContent = this.label;
		link.appendChild(text);

		link.addEventListener('click', (e) => {
			e.preventDefault();
			e.stopPropagation();
			link.dispatchEvent(
				new CustomEvent('page-navigate', {
					bubbles: true,
					detail: { href: this.href },
				})
			);
		});

		link.addEventListener('contextmenu', (e) => {
			showLinkContextMenu(e, view, this.from, this.to, this.href, false);
		});

		return link;
	}

	eq(other: InternalLinkWidget) {
		return other.label === this.label && other.href === this.href;
	}

	ignoreEvent() { return false; }
}

// =============================================================================
// Decoration Builder
// =============================================================================

// Regex to find markdown links: [label](url) — but NOT images ![alt](url)
const LINK_REGEX = /\[([^\]]+)\]\(([^)]+)\)/g;

function buildLinkDecorations(view: EditorView): DecorationSet {
	const builder: Range<Decoration>[] = [];
	const doc = view.state.doc;
	const { from: vpFrom, to: vpTo } = view.viewport;

	// Active-line exclusion (Obsidian pattern): don't decorate the cursor's line
	const cursorLine = doc.lineAt(view.state.selection.main.head).number;

	// Scan visible lines for link patterns
	const startLine = doc.lineAt(vpFrom).number;
	const endLine = doc.lineAt(Math.min(vpTo, doc.length)).number;

	for (let lineNum = startLine; lineNum <= endLine; lineNum++) {
		// Skip the cursor's line — show raw markdown for editing
		if (lineNum === cursorLine) continue;

		const line = doc.line(lineNum);
		LINK_REGEX.lastIndex = 0;

		for (let match = LINK_REGEX.exec(line.text); match !== null; match = LINK_REGEX.exec(line.text)) {
			const label = match[1];
			const url = match[2];

			// Skip image links: ![alt](url)
			if (match.index > 0 && line.text[match.index - 1] === '!') continue;

			// Skip empty URLs
			if (!url.trim()) continue;

			const from = line.from + match.index;
			const to = from + match[0].length;

			// URL-aware widget selection
			let widget: WidgetType;
			if (isExternalUrl(url)) {
				widget = new ExternalLinkWidget(label, url, from, to);
			} else if (isEntityUrl(url)) {
				widget = new EntityLinkWidget(label, url, from, to);
			} else {
				widget = new InternalLinkWidget(label, url, from, to);
			}

			builder.push(
				Decoration.replace({
					widget,
					inclusive: false,
				}).range(from, to)
			);
		}
	}

	builder.sort((a, b) => a.from - b.from);
	return Decoration.set(builder);
}

// =============================================================================
// Plugin
// =============================================================================

const linkPillsPlugin = ViewPlugin.fromClass(
	class {
		decorations: DecorationSet;

		constructor(view: EditorView) {
			this.decorations = buildLinkDecorations(view);
		}

		update(update: ViewUpdate) {
			if (update.docChanged || update.viewportChanged || update.selectionSet) {
				this.decorations = buildLinkDecorations(update.view);
			}
		}
	},
	{
		decorations: (v) => v.decorations,
	}
);

export const entityLinks: Extension = linkPillsPlugin;
