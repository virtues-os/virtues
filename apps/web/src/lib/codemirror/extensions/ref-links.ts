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

import { type Extension, type Range } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate, WidgetType } from '@codemirror/view';
import { mount, unmount } from 'svelte';
import { contextMenu } from '$lib/stores/contextMenu.svelte';
import { linkEditor } from '$lib/stores/linkEditor.svelte';

import { selectionTouches } from './inline-marks';
import { dragJustEnded, isMouseSelecting } from './mouse-freeze';
import { getEntityTypeFromRoute } from '$lib/utils/refRoutes';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import RefPreview from '$lib/components/RefPreview.svelte';

// =============================================================================
// URL Classification
// =============================================================================

const ENTITY_PREFIXES = [
	'/person/', '/page/', '/org/', '/place/',
	'/day/', '/year/', '/source/', '/chat/', '/drive/', '/space/',
] as const;

function isEntityUrl(url: string): boolean {
	return ENTITY_PREFIXES.some(p => url.startsWith(p));
}

function isExternalUrl(url: string): boolean {
	return url.startsWith('http://') || url.startsWith('https://');
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
				// This used to drop the caret on the line so the raw
				// `[label](url)` would reveal itself for editing. Links no longer
				// reveal, so there is nothing to drop the caret into — the label
				// and href get their own panel instead.
				const current = /^!?\[([^\]]*)\]\(([^)]*)\)$/.exec(
					view.state.sliceDoc(from, to),
				);
				linkEditor.show(
					{ label: current?.[1] ?? '', href: current?.[2] ?? href },
					({ label, href: newHref }) => {
						view.dispatch({
							changes: { from, to, insert: `[${label}](${newHref})` },
						});
						view.focus();
					},
					{ x: e.clientX, y: e.clientY, width: 0, height: 0 },
				);
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
 * Inline reference link — the ONE inline density. Every `[label](url)` (entity,
 * file, internal path, or external URL) renders as a plain underlined link that
 * belongs to the prose (Wikipedia-style): no pill, no chip, no favicon. The `@`
 * marker, if any, is stripped for display. Target/type is surfaced on hover (see
 * refHoverPlugin) and in the block embed — never in inline chrome.
 *
 * Click model: ⌘/Ctrl-click acts (external → new tab; entity → open beside;
 * other internal → page-navigate event). Plain click falls through to CM and
 * places the caret in the line — the text no longer changes when it does.
 * Editing the label or the URL is right-click → Edit, which opens a panel; the
 * raw `[label](url)` is not shown in the document at any point.
 */
class RefLinkWidget extends WidgetType {
	constructor(
		private label: string,
		private href: string,
		private from: number,
		private to: number,
	) {
		super();
	}

	private displayText() {
		return this.label.replace(/^@/, '');
	}

	private activate() {
		if (isExternalUrl(this.href)) {
			window.open(this.href, '_blank', 'noopener,noreferrer');
		} else if (isEntityUrl(this.href)) {
			windowShellStore.openRouteBeside(this.href, this.displayText());
		} else {
			// Non-entity internal path — let the app route it.
			document.dispatchEvent(
				new CustomEvent('page-navigate', { bubbles: true, detail: { href: this.href } }),
			);
		}
	}

	toDOM(view: EditorView) {
		const link = document.createElement('a');
		link.className = 'cm-ref-link';
		link.href = this.href;
		link.textContent = this.displayText();

		// Data for the hover-preview plugin (delegated on the editor DOM).
		link.dataset.refHref = this.href;
		link.dataset.refLabel = this.displayText();

		link.addEventListener('click', (e) => {
			// Always stop the <a> from navigating; the caret is placed on mousedown,
			// so a plain click still drops into the line — it just no longer
			// changes what the line says.
			e.preventDefault();
			if (!(e.metaKey || e.ctrlKey)) return;
			e.stopPropagation();
			this.activate();
		});

		link.addEventListener('contextmenu', (e) => {
			showLinkContextMenu(e, view, this.from, this.to, this.href, isExternalUrl(this.href));
		});

		return link;
	}

	eq(other: RefLinkWidget) {
		return other.label === this.label && other.href === this.href;
	}

	ignoreEvent() { return false; }
}

// =============================================================================
// Decoration Builder
// =============================================================================

// Regex to find markdown links: [label](url). A leading `!` means media
// (![alt](url)) — left to the media widgets — UNLESS the target is an entity,
// in which case it's a legacy block embed that now renders as a plain inline
// link (entities have no card; they are always inline links + hover).
const LINK_REGEX = /\[([^\]]+)\]\(([^)]+)\)/g;

function buildLinkDecorations(view: EditorView): DecorationSet {
	const builder: Range<Decoration>[] = [];
	const doc = view.state.doc;
	const { from: vpFrom, to: vpTo } = view.viewport;

	// Reveal-on-touch, per CONSTRUCT — not the old per-line rule, where the
	// caret touching a line burst every link on it back into `[label](url)`
	// and rewrapped the whole paragraph. Only the one link the selection
	// touches shows its source; its neighbors stay rendered. The Edit popover
	// remains the deliberate path for fixing a URL without entering the text.
	const startLine = doc.lineAt(vpFrom).number;
	const endLine = doc.lineAt(Math.min(vpTo, doc.length)).number;

	for (let lineNum = startLine; lineNum <= endLine; lineNum++) {
		const line = doc.line(lineNum);
		LINK_REGEX.lastIndex = 0;

		for (let match = LINK_REGEX.exec(line.text); match !== null; match = LINK_REGEX.exec(line.text)) {
			const label = match[1];
			const url = match[2];

			// Skip empty URLs
			if (!url.trim()) continue;

			// A leading `!` is media (image/audio/video/file) → let the media
			// widgets render it. But a `!` in front of an ENTITY link is a legacy
			// block embed — render it inline and swallow the `!` too.
			const bang = match.index > 0 && line.text[match.index - 1] === '!';
			if (bang && !isEntityUrl(url)) continue;

			const from = line.from + match.index - (bang ? 1 : 0);
			const to = line.from + match.index + match[0].length;

			// Touched → leave the raw markdown in place for direct editing.
			if (selectionTouches(view.state, { openFrom: from, closeTo: to })) continue;

			// One inline density for every target — a plain underlined link.
			builder.push(
				Decoration.replace({
					widget: new RefLinkWidget(label, url, from, to),
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
			// Selection matters again (reveal-on-touch), but rebuilds are held
			// mid-drag — see mouse-freeze.ts.
			const rebuild =
				update.docChanged ||
				update.viewportChanged ||
				(update.selectionSet && !isMouseSelecting(update.state)) ||
				dragJustEnded(update);
			if (rebuild) {
				this.decorations = buildLinkDecorations(update.view);
			}
		}
	},
	{
		decorations: (v) => v.decorations,
	}
);

// =============================================================================
// Hover preview
// =============================================================================

// Inline links are plain, so the type/summary lives in a floating RefPreview
// shown on dwell. Delegated on the editor DOM (one plugin, not one listener per
// link) and mounts the same Svelte RefPreview the rendered views use. Show/hide
// dwell matches refHover.svelte so the pointer can travel link → card.
const HOVER_SHOW_DELAY = 350;
const HOVER_HIDE_DELAY = 160;

const refHoverPlugin = ViewPlugin.fromClass(
	class {
		private showTimer: ReturnType<typeof setTimeout> | null = null;
		private hideTimer: ReturnType<typeof setTimeout> | null = null;
		// biome-ignore lint/suspicious/noExplicitAny: Svelte mount() instance handle
		private instance: any = null;
		private container: HTMLElement | null = null;
		private anchor: HTMLElement | null = null;

		private onOver = (e: MouseEvent) => {
			const el = (e.target as HTMLElement)?.closest?.('.cm-ref-link') as HTMLElement | null;
			if (!el || el === this.anchor) return;
			this.clearHide();
			this.clearShow();
			this.anchor = el;
			this.showTimer = setTimeout(() => this.show(el), HOVER_SHOW_DELAY);
		};

		private onOut = (e: MouseEvent) => {
			if (!(e.target as HTMLElement)?.closest?.('.cm-ref-link')) return;
			this.clearShow();
			this.scheduleHide();
		};

		constructor(private view: EditorView) {
			view.dom.addEventListener('mouseover', this.onOver);
			view.dom.addEventListener('mouseout', this.onOut);
		}

		private show(el: HTMLElement) {
			this.destroyCard();
			const href = el.dataset.refHref || el.getAttribute('href') || '';
			const label = el.dataset.refLabel || el.textContent || '';
			// External URLs report as 'link' so RefCard shows the domain; entities
			// resolve their real type from the route.
			const type = isExternalUrl(href) ? 'link' : getEntityTypeFromRoute(href);
			const open = () => {
				if (isExternalUrl(href)) window.open(href, '_blank', 'noopener,noreferrer');
				else windowShellStore.openRouteBeside(href, label);
			};

			this.container = document.createElement('div');
			this.instance = mount(RefPreview, {
				target: this.container,
				props: {
					anchor: el,
					type,
					label,
					url: href,
					onOpen: open,
					oncardenter: () => this.clearHide(),
					oncardleave: () => this.scheduleHide(),
				},
			});
			this.anchor = el;
		}

		private scheduleHide() {
			this.clearHide();
			this.hideTimer = setTimeout(() => {
				this.destroyCard();
				this.anchor = null;
			}, HOVER_HIDE_DELAY);
		}
		private clearShow() {
			if (this.showTimer) { clearTimeout(this.showTimer); this.showTimer = null; }
		}
		private clearHide() {
			if (this.hideTimer) { clearTimeout(this.hideTimer); this.hideTimer = null; }
		}
		private destroyCard() {
			if (this.instance) { void unmount(this.instance); this.instance = null; }
			// RefPreview portals its card to <body>; the mount container is empty
			// but remove it too.
			if (this.container) { this.container.remove(); this.container = null; }
		}

		destroy() {
			this.clearShow();
			this.clearHide();
			this.destroyCard();
			this.view.dom.removeEventListener('mouseover', this.onOver);
			this.view.dom.removeEventListener('mouseout', this.onOut);
		}
	},
);

export const entityLinks: Extension = [linkPillsPlugin, refHoverPlugin];
