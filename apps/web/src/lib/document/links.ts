/**
 * Links in a block page.
 *
 * - A link to something on the box (a route, `/kind/id`) reads as a ref:
 *   the `doc-ref-link` class, and ⌘ or Ctrl-click opens it beside the page.
 *   A link elsewhere (`doc-web-link`) opens in a new tab on ⌘ or Ctrl-click.
 *   Off a link, ⌘ or Ctrl-click is a plain click, never ProseMirror's
 *   selection of the whole block.
 *   Both preview on hover (the host's `onHover`): a ref as its record, a
 *   link elsewhere as its address (`previewType`), as the CodeMirror editor
 *   previews them. In an editable page a plain click places the caret, so
 *   a link's text can be edited like any other; in a page that is read
 *   only, a plain click opens it.
 * - The link menu (right-click, or a long press): Open, Open beside, Copy
 *   link, and on an editable page Edit, Turn into embed, Remove link. Turn
 *   into embed makes a route a mention and a media or Drive address a media
 *   block.
 * - ⌘K links the selected text or edits the link at the caret
 *   (`editLinkAtSelection`). A selected widget is no text to link: ⌘K
 *   leaves it, and the page's ⌘K (Ask or search) has it then
 *   (`canEditLinkAtSelection`).
 * - The page stays live while the menu or the link panel is open: another
 *   device or the assistant can edit it meanwhile. What they act on is held
 *   by anchors (`anchor.ts`) and checked when they act: a link or a
 *   selection someone changed meanwhile is left alone, and the person told.
 */

import { Extension, getMarkRange, type Editor } from '@tiptap/core';
import type { Mark, Node as PmNode } from '@tiptap/pm/model';
import { NodeSelection, Plugin, PluginKey, TextSelection, type EditorState } from '@tiptap/pm/state';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';
import { toast } from 'svelte-sonner';
import { contextMenu, type ContextMenuItem } from '$lib/stores/contextMenu.svelte';
import { linkEditor } from '$lib/stores/linkEditor.svelte';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { getEntityTypeFromRoute, parseRef } from '$lib/utils/refRoutes';
import { isRoute, linkAllowed } from './schema';
import { driveFileId, mediaNode } from './media';
import { insertBlockAt } from './place';
import { kindOfLink } from './media-kind';
import type { ContextHandler } from './gesture';
import { anchorAt, anchorRange, resolveRange } from './anchor';

const HOVER_SHOW_MS = 350;
const HOVER_HIDE_MS = 160;

export interface LinkAt {
	from: number;
	to: number;
	href: string;
	text: string;
	mark: Mark;
}

/** Whether `href` is a ref: a route to a record on the box, which previews and opens beside. */
export function isRef(href: string): boolean {
	return parseRef(href) !== null;
}

/** What a link's hover preview shows it as: its record for a ref, its address for anything else. */
export function previewType(href: string): string | null {
	return isRef(href) ? getEntityTypeFromRoute(href) : 'link';
}

/** The link around `pos`: its whole run, however many marks split it. */
export function linkAt(state: EditorState, pos: number): LinkAt | null {
	const type = state.schema.marks.link;
	if (!type) return null;
	const $pos = state.doc.resolve(pos);
	const range = getMarkRange($pos, type) ?? (pos > 0 ? getMarkRange(state.doc.resolve(pos - 1), type) : undefined);
	if (!range) return null;
	const node = state.doc.nodeAt(range.from);
	const mark = node?.marks.find((m) => m.type === type);
	if (!mark) return null;
	return {
		from: range.from,
		to: range.to,
		href: String(mark.attrs.href),
		text: state.doc.textBetween(range.from, range.to, ' ', ''),
		mark,
	};
}

/**
 * Open a link: a ref in this pane, anything else in a new tab. An address
 * the contract refuses (`javascript:`), which reached the page past the
 * server's check, opens nothing.
 */
export function openLink(href: string, label: string): void {
	if (isRef(href)) windowShellStore.navigate(href, { label });
	else if (linkAllowed(href)) window.open(href, '_blank', 'noopener,noreferrer');
}

/** Open a link beside the page: a ref in the other pane, anything else in a new tab (`openLink`). */
export function openLinkBeside(href: string, label: string): void {
	if (isRef(href)) windowShellStore.openRouteBeside(href, label);
	else if (linkAllowed(href)) window.open(href, '_blank', 'noopener,noreferrer');
}

/** The address to copy: a ref as a full link to this box. */
export function shareableHref(href: string): string {
	return href.startsWith('/') ? `${window.location.origin}${href}` : href;
}

const refLinksKey = new PluginKey<DecorationSet>('refLinks');

/** The decoration over each run of a link: `doc-ref-link` to a route, `doc-web-link` to anywhere else. */
function refDecorations(doc: PmNode): DecorationSet {
	const decos: Decoration[] = [];
	doc.descendants((node, pos) => {
		if (!node.isText) return true;
		const link = node.marks.find((m) => m.type.name === 'link');
		const href = link ? String(link.attrs.href) : '';
		if (link && href) {
			const kind = isRef(href) ? 'doc-ref-link' : 'doc-web-link';
			decos.push(Decoration.inline(pos, pos + node.nodeSize, { class: kind, 'data-ref-href': href }));
		}
		return false;
	});
	return DecorationSet.create(doc, decos);
}

/** A link in the page's DOM that previews. */
const LINK_SELECTOR = '.doc-ref-link, .doc-web-link';

/**
 * A link as a menu or the link panel found it, followed to where it is when
 * they act: the same address over the same words, wherever edits made
 * meanwhile have moved it. Null once someone has changed either, or taken
 * the link out. Held by its last character, so typing just before the link
 * moves it and leaves it whole.
 */
export function followLink(editor: Editor, link: LinkAt): () => LinkAt | null {
	const end = anchorAt(editor.state, link.to);
	return () => {
		const at = end.resolve();
		if (at === null || at <= 0) return null;
		const now = linkAt(editor.state, at - 1);
		return now && now.to === at && now.href === link.href && now.text === link.text ? now : null;
	};
}

/** Tell the person a link changed under its menu or panel, so nothing was done to it. */
function linkChanged(where: 'menu' | 'panel'): void {
	toast.error("Couldn't change the link", {
		description:
			where === 'menu'
				? 'Someone changed it while its menu was open. Open the menu again to change it.'
				: 'Someone changed it while you were editing it. Open its menu again to edit it.',
	});
}

/**
 * Replace the link with what it points at, as the page shows it: a Drive
 * file or a media address as its block, a ref as a mention. `link` is where
 * the link is now.
 */
export function turnIntoEmbed(editor: Editor, link: LinkAt): boolean {
	const { schema } = editor.state;
	const tr = editor.state.tr;
	if (driveFileId(link.href) === null && isRef(link.href) && isRoute(link.href) && schema.nodes.mention) {
		const label = link.text.replace(/^@/, '').trim() || link.href;
		tr.replaceWith(link.from, link.to, schema.nodes.mention.create({ to: link.href, label }));
	} else {
		const node = mediaNode(schema, kindOfLink(link.href, link.text), link.href, link.text);
		if (!node) return false;
		tr.delete(link.from, link.to);
		insertBlockAt(tr, link.from, node);
	}
	editor.view.dispatch(tr);
	return true;
}

/** Edit a link's text and address in the link panel. */
export function editLink(editor: Editor, link: LinkAt, anchor?: { x: number; y: number }): void {
	const current = followLink(editor, link);
	linkEditor.show(
		{ label: link.text, href: link.href },
		({ label, href }) => {
			const now = current();
			if (!now) {
				linkChanged('panel');
				return;
			}
			const { schema } = editor.state;
			const mark = schema.marks.link.create({ href });
			const marks = editor.state.doc.resolve(now.from + 1).marks().filter((m) => m.type !== mark.type);
			editor.view.dispatch(
				editor.state.tr.replaceWith(now.from, now.to, schema.text(label, [...marks, mark])),
			);
			editor.commands.focus();
		},
		anchor ? { x: anchor.x, y: anchor.y, width: 0, height: 0 } : undefined,
	);
}

/**
 * Whether ⌘K has something to do here: a link at the caret, or selected text
 * to link. A selected widget (an image, an applet, a mention), or a range
 * holding one, is not text: its words are not the panel's to rewrite.
 */
export function canEditLinkAtSelection(state: EditorState): boolean {
	const sel = state.selection;
	if (sel instanceof NodeSelection) return false;
	if (linkAt(state, sel.from)) return true;
	if (sel.empty) return false;
	let widget = false;
	state.doc.nodesBetween(sel.from, sel.to, (node) => {
		if (node.isInline && !node.isText) widget = true;
		return !widget;
	});
	return !widget;
}

/**
 * ⌘K: edit the link at the selection, or link the selected text. The text
 * is linked only if it still reads as it did when the panel opened; kept as
 * it read, it is linked where it stands, its blocks and marks as they are.
 */
export function editLinkAtSelection(editor: Editor, anchor?: { x: number; y: number }): boolean {
	if (!canEditLinkAtSelection(editor.state)) return false;
	const { from, to } = editor.state.selection;
	const link = linkAt(editor.state, from);
	if (link) {
		editLink(editor, link, anchor);
		return true;
	}
	const text = editor.state.doc.textBetween(from, to, ' ', '');
	const range = anchorRange(editor.state, from, to);
	linkEditor.show(
		{ label: text, href: '' },
		({ label, href }) => {
			const now = resolveRange(range);
			if (!now || editor.state.doc.textBetween(now.from, now.to, ' ', '') !== text) {
				toast.error("Couldn't link that text", {
					description: 'Someone changed it while the link panel was open. Select it again to link it.',
				});
				return;
			}
			const { schema } = editor.state;
			const mark = schema.marks.link.create({ href });
			let tr = editor.state.tr;
			let end: number;
			if (label === text) {
				tr = tr.addMark(now.from, now.to, mark);
				end = now.to;
			} else {
				const marks = editor.state.doc.resolve(now.from + 1).marks().filter((m) => m.type !== schema.marks.link);
				tr = tr.replaceWith(now.from, now.to, schema.text(label, [...marks, mark]));
				end = now.from + label.length;
			}
			editor.view.dispatch(tr.setSelection(TextSelection.create(tr.doc, end)));
			editor.commands.focus();
		},
		anchor ? { x: anchor.x, y: anchor.y, width: 0, height: 0 } : undefined,
	);
	return true;
}

/**
 * The link menu. Opening a link reads its address as the menu found it;
 * every item that changes the page acts on the link where it is then. A page
 * that is read only offers only what leaves it as it is.
 */
export function linkMenu(editor: Editor, link: LinkAt, at: { x: number; y: number }): ContextMenuItem[] {
	const label = link.text || link.href;
	const current = followLink(editor, link);
	const change = (act: (now: LinkAt) => void) => () => {
		const now = current();
		if (now) act(now);
		else linkChanged('menu');
	};
	const reading: ContextMenuItem[] = [
		{ id: 'open', label: 'Open', icon: 'ri:arrow-right-up-line', action: () => openLink(link.href, label) },
		{ id: 'open-beside', label: 'Open beside', icon: 'ri:layout-column-line', action: () => openLinkBeside(link.href, label) },
		{
			id: 'copy-link',
			label: 'Copy link',
			icon: 'ri:file-copy-line',
			action: () => void navigator.clipboard?.writeText(shareableHref(link.href)),
		},
	];
	if (!editor.isEditable) return reading;
	return [
		...reading,
		{ id: 'edit', label: 'Edit', icon: 'ri:edit-line', dividerBefore: true, action: change((now) => editLink(editor, now, at)) },
		{ id: 'embed', label: 'Turn into embed', icon: 'ri:image-line', action: change((now) => void turnIntoEmbed(editor, now)) },
		{
			id: 'remove',
			label: 'Remove link',
			icon: 'ri:link-unlink',
			variant: 'destructive',
			action: change((now) => editor.view.dispatch(editor.state.tr.removeMark(now.from, now.to, now.mark.type))),
		},
	];
}

/** The context gesture on a link opens the link menu. */
export function linkContext(editor: () => Editor | null): ContextHandler {
	return (view, at) => {
		const ed = editor();
		if (!ed || !at.target.closest('a')) return false;
		const link = linkAt(view.state, at.pos);
		if (!link) return false;
		contextMenu.show({ x: at.x, y: at.y }, linkMenu(ed, link, at));
		return true;
	};
}

export interface LinkHover {
	/** Show the preview for the link under the pointer (`previewType` says as what). */
	onHover: (anchor: HTMLElement, href: string, label: string) => void;
	/** Hide it, unless the pointer moved onto the preview (the host keeps it then). */
	onLeave: () => void;
}

export function links(hover?: LinkHover): Extension {
	return Extension.create({
		name: 'pageLinks',
		addProseMirrorPlugins() {
			return [
				new Plugin<DecorationSet>({
					key: refLinksKey,
					state: {
						init: (_config, state) => refDecorations(state.doc),
						apply: (tr, prev) => (tr.docChanged ? refDecorations(tr.doc) : prev),
					},
					props: {
						decorations: (state) => refLinksKey.getState(state),
						handleClick(view, pos, event) {
							if (!(event.metaKey || event.ctrlKey)) return false;
							const link = linkAt(view.state, pos);
							if (link) {
								openLinkBeside(link.href, link.text || link.href);
								return true;
							}
							// Off a link, the click is a plain one. ⌘ (Ctrl elsewhere) is also
							// ProseMirror's key for selecting the whole block clicked in, a
							// selection drawn as nothing, which the next key replaced: a click
							// that just missed a link took a paragraph. A widget is selected,
							// as a plain click selects it.
							const inside = view.posAtCoords({ left: event.clientX, top: event.clientY })?.inside ?? -1;
							const widget = inside >= 0 ? view.state.doc.nodeAt(inside) : null;
							const selection =
								widget?.isAtom && NodeSelection.isSelectable(widget)
									? NodeSelection.create(view.state.doc, inside)
									: TextSelection.near(view.state.doc.resolve(pos));
							view.dispatch(view.state.tr.setSelection(selection).setMeta('pointer', true));
							return true;
						},
						handleDOMEvents: {
							// A link's own navigation never runs inside the editor. In a
							// page that is read only there is no caret to place: a plain
							// click opens the link. A widget's own link (a file's card)
							// is the widget's, and opens what it points at.
							click(view, event) {
								const a = (event.target as HTMLElement | null)?.closest?.('a[href]');
								if (!a || a.closest('.doc-widget, [data-widget-control]')) return false;
								event.preventDefault();
								if (view.editable || event.metaKey || event.ctrlKey || event.button !== 0) return false;
								const link = linkAt(view.state, view.posAtDOM(a, 0));
								if (!link) return false;
								openLink(link.href, link.text || link.href);
								return true;
							},
						},
					},
					view(view: EditorView) {
						if (!hover) return {};
						let showTimer: ReturnType<typeof setTimeout> | null = null;
						let hideTimer: ReturnType<typeof setTimeout> | null = null;
						let anchor: HTMLElement | null = null;
						const clear = () => {
							if (showTimer) clearTimeout(showTimer);
							if (hideTimer) clearTimeout(hideTimer);
							showTimer = hideTimer = null;
						};
						const over = (e: MouseEvent) => {
							const el = (e.target as HTMLElement | null)?.closest?.(LINK_SELECTOR) as HTMLElement | null;
							if (!el || el === anchor) return;
							clear();
							anchor = el;
							showTimer = setTimeout(() => {
								const href = el.dataset.refHref ?? '';
								if (href) hover.onHover(el, href, el.textContent ?? '');
							}, HOVER_SHOW_MS);
						};
						const out = (e: MouseEvent) => {
							if (!(e.target as HTMLElement | null)?.closest?.(LINK_SELECTOR)) return;
							clear();
							hideTimer = setTimeout(() => {
								anchor = null;
								hover.onLeave();
							}, HOVER_HIDE_MS);
						};
						view.dom.addEventListener('mouseover', over);
						view.dom.addEventListener('mouseout', out);
						return {
							destroy() {
								clear();
								view.dom.removeEventListener('mouseover', over);
								view.dom.removeEventListener('mouseout', out);
							},
						};
					},
				}),
			];
		},
	});
}
