// @vitest-environment happy-dom
/**
 * The block editor by keyboard, by touch, read only, to assistive
 * technology and on paper: what a pointer on a desktop would never show.
 * Fictional people only.
 */

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.hoisted(() => {
	(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
});

vi.mock('svelte', async (orig) => ({
	...(await orig<typeof import('svelte')>()),
	mount: () => ({}),
	unmount: () => undefined,
}));
// The node views' components cannot be compiled here (no Svelte plugin in vitest.config.ts).
vi.mock('$lib/components/pages/nodes/MentionNode.svelte', () => ({ default: () => {} }));
vi.mock('$lib/components/pages/nodes/MediaNode.svelte', () => ({ default: () => {} }));
vi.mock('$lib/components/pages/nodes/AppletNode.svelte', () => ({ default: () => {} }));
vi.mock('$lib/components/pages/nodes/CalloutNode.svelte', () => ({ default: () => {} }));
vi.mock('$lib/components/pages/nodes/CodeBlockNode.svelte', () => ({ default: () => {} }));
vi.mock('$lib/stores/contextMenu.svelte', () => ({ contextMenu: { show: vi.fn(), reachFirstRow: vi.fn() } }));
vi.mock('$lib/stores/linkEditor.svelte', () => ({ linkEditor: { show: vi.fn() } }));
vi.mock('$lib/stores/window-shell.svelte', () => ({
	windowShellStore: { openRouteBeside: vi.fn(), navigate: vi.fn() },
}));
vi.mock('svelte-sonner', () => ({ toast: Object.assign(vi.fn(), { error: vi.fn() }) }));
vi.mock('$lib/api/client', async (orig) => ({
	...(await orig<typeof import('$lib/api/client')>()),
	getApplet: vi.fn(async (id: string) => ({ id, name: 'Sleep this week' })),
}));

import { computePosition } from '@floating-ui/dom';
import { Extension, type Editor } from '@tiptap/core';
import { NodeSelection, TextSelection } from '@tiptap/pm/state';
import { CellSelection } from '@tiptap/pm/tables';
import { contextMenu } from '$lib/stores/contextMenu.svelte';
import { linkEditor } from '$lib/stores/linkEditor.svelte';
import { windowShellStore } from '$lib/stores/window-shell.svelte';
import { shortcuts } from '$lib/shortcuts/registry.svelte';
import { isAppleKeyboard } from '$lib/utils/platform';
import { inComposition } from '$lib/utils/ime';
import { listenForMenuKeys, type MenuKeyTarget } from '$lib/components/contextMenu/keys';
import { holdsActiveDescendant, pointFocusAt } from '$lib/components/contextMenu/activeDescendant';
import { floatingMiddleware } from '$lib/floating/core/middleware';
import { onContextGesture } from '$lib/codemirror/extensions/long-press';
import { filterCommands } from '$lib/components/menuCommand';
import { codeAt, createPageEditor, editorAttributes, setPageEditorState } from './editor';
import { contract, contractExtensions } from './schema';
import { contextGesture, openContextFromKeyboard } from './gesture';
import { canEditLinkAtSelection, editLinkAtSelection, linkContext, links, previewType } from './links';
import { find, findState, replaceAll, replaceCurrent, setFindQuery } from './find';
import { cellAt, moveColumn, moveRow, selectionForCellMenu, tableActions, tableBarAnchor, tableMenuItems } from './tables';
import { proposalIds, proposalKeys, proposalSaid } from './suggestions';
import { indentItem, outdentItem, treeCommands, type TreeCommandHost } from './commands';
import { LANGUAGE_CHOICES, OTHER_LANGUAGE, typedLanguage } from './languages';
import {
	KEYBOARD_SHOW_DELAY_MS,
	SELECTION_HANDLE_ROOM,
	TOOLBAR_CLEARANCE,
	TOOLBAR_OFFSET,
	TOUCH_TOOLBAR_HEIGHT,
	selectionToolbarAt,
	toolbarTiming,
} from './toolbar';
import { dragToResize, sizeByKey } from './resize';
import { markForPrint, printAlone, printThis } from './print';
import { renderCaret } from './presence';
import { absolutePositionToRelativePosition, ySyncPluginKey } from '@tiptap/y-tiptap';
import * as Y from 'yjs';
import { applyAwarenessUpdate, encodeAwarenessUpdate } from 'y-protocols/awareness';
import { toast } from 'svelte-sonner';
import { fakeProvider, htmlEditor, pageDoc, posIn, settle } from './test-utils';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const read = (rel: string) => fs.readFileSync(path.resolve(HERE, rel), 'utf8');

const editors: Editor[] = [];
const cleanups: (() => void)[] = [];

function track<T extends Editor>(editor: T): T {
	editors.push(editor);
	return editor;
}

afterEach(() => {
	for (const e of editors.splice(0)) if (!e.isDestroyed) e.destroy();
	for (const c of cleanups.splice(0)) c();
	document.body.replaceChildren();
	vi.mocked(contextMenu.show).mockClear();
	vi.mocked(contextMenu.reachFirstRow).mockClear();
	vi.mocked(linkEditor.show).mockClear();
	vi.mocked(windowShellStore.navigate).mockClear();
	vi.mocked(windowShellStore.openRouteBeside).mockClear();
});

/** An editor on a shared page, as the app mounts one. */
function bound(html: string, plugins: Parameters<typeof createPageEditor>[0]['plugins'] = [], editable = true) {
	const { doc } = pageDoc(html);
	const provider = fakeProvider(doc);
	cleanups.push(provider.destroy);
	const element = document.createElement('div');
	document.body.append(element);
	return track(
		createPageEditor({
			element,
			doc: { ydoc: doc, provider },
			editable,
			label: 'Trip to Lisbon',
			placeholder: 'Write',
			user: { name: 'Computer', tint: 'orange' },
			plugins,
		}),
	);
}

/** A key pressed in the editor, as the browser delivers it. */
function press(editor: Editor, key: string, mods: { shift?: boolean; alt?: boolean; mod?: boolean } = {}): KeyboardEvent {
	const event = new KeyboardEvent('keydown', {
		key,
		code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
		shiftKey: !!mods.shift,
		altKey: !!mods.alt,
		metaKey: !!mods.mod && isAppleKeyboard,
		ctrlKey: !!mods.mod && !isAppleKeyboard,
		bubbles: true,
		cancelable: true,
	});
	editor.view.dom.dispatchEvent(event);
	return event;
}

/** A key with modifiers: `mod` is ⌘ or Ctrl as `press` has it; `ctrl` is Ctrl itself; `key` names the key when it is not the plain one. */
type KeyAt = { key?: string; mod?: boolean; ctrl?: boolean; alt?: boolean; shift?: boolean };

/** Press `k`, the key being `plain` unless `k` names another, as the browser delivers it. */
function keyAt(editor: Editor, k: KeyAt, plain: string): KeyboardEvent {
	const key = k.key ?? plain;
	const event = new KeyboardEvent('keydown', {
		key,
		code: key.length === 1 ? `Key${key.toUpperCase()}` : key,
		shiftKey: !!k.shift,
		altKey: !!k.alt,
		metaKey: !!k.mod && isAppleKeyboard,
		ctrlKey: !!k.ctrl || (!!k.mod && !isAppleKeyboard),
		bubbles: true,
		cancelable: true,
	});
	editor.view.dom.dispatchEvent(event);
	return event;
}

describe('assistive technology', () => {
	it('names the editor a many-line text box for its page, read only when it is', () => {
		const editor = bound('<p>Hello</p>');
		const dom = editor.view.dom;
		expect(dom.getAttribute('role')).toBe('textbox');
		expect(dom.getAttribute('aria-multiline')).toBe('true');
		expect(dom.getAttribute('aria-label')).toBe('Trip to Lisbon');
		expect(dom.hasAttribute('aria-readonly')).toBe(false);

		setPageEditorState(editor, { editable: false, spellcheck: true, label: 'Trip to Lisbon' });
		expect(dom.getAttribute('contenteditable')).toBe('false');
		expect(dom.getAttribute('aria-readonly')).toBe('true');
		expect(dom.getAttribute('aria-multiline')).toBe('true');
		setPageEditorState(editor, { editable: true, spellcheck: false, label: 'Trip to Lisbon' });
		expect(dom.hasAttribute('aria-readonly')).toBe(false);
		expect(dom.getAttribute('spellcheck')).toBe('false');

		const readOnly = bound('<p>Hello</p>', [], false);
		expect(readOnly.view.dom.getAttribute('aria-readonly')).toBe('true');
		expect(editorAttributes({ spellcheck: true, editable: true })['aria-label']).toBe('Page');
	});

	// A menu opens while the editor keeps the focus (keys.ts), and a screen
	// reader follows the focus: the editor points at the row the arrows reach.
	it('a menu open while the editor keeps the focus is read row by row, from the editor', () => {
		const editor = bound('<p>Hello</p>');
		const dom = editor.view.dom;
		dom.setAttribute('aria-controls', 'outline');
		const told = pointFocusAt(dom, 'context-menu');
		expect(dom.getAttribute('aria-controls')).toBe('context-menu');
		// A text box does not expand; a button that opened a menu does.
		expect(dom.hasAttribute('aria-expanded')).toBe(false);
		told.highlight('context-menu-row-2');
		expect(dom.getAttribute('aria-activedescendant')).toBe('context-menu-row-2');
		told.highlight(null);
		expect(dom.hasAttribute('aria-activedescendant')).toBe(false);
		told.release();
		expect(dom.getAttribute('aria-controls')).toBe('outline');
		const more = document.createElement('button');
		const fromButton = pointFocusAt(more, 'context-menu');
		expect(more.getAttribute('aria-expanded')).toBe('true');
		fromButton.release();
		expect(more.hasAttribute('aria-expanded')).toBe(false);

		const provider = read('../components/contextMenu/ContextMenuProvider.svelte');
		expect(provider).toMatch(/id=\{MENU_ID\}/);
		expect(provider).toMatch(/rowId=\{rowId\(index\)\}/);
		expect(provider).toMatch(/const owner = focusOwner\(\);\s+if \(owner\) pointed = pointFocusAt\(owner, MENU_ID\);/);
		expect(provider).toMatch(/pointed\?\.highlight\(index >= 0 \? rowId\(index\) : null\);/);
		expect(read('../components/contextMenu/ContextMenuItem.svelte')).toMatch(/id=\{rowId\}/);
		expect(read('../components/MenuItem.svelte')).toMatch(/\{role\}\s+\{id\}/);
		const slash = read('../components/SlashMenu.svelte');
		expect(slash).toMatch(/id=\{LIST_ID\} role="listbox"/);
		expect(slash).toMatch(/id=\{optionId\(index\)\}\s+role="option"\s+aria-selected=\{index === selectedIndex\}/);
		expect(slash).toMatch(/pointed\?\.highlight\(flatCommands\.length \? optionId\(selectedIndex\) : null\);/);
	});

	// A button, a link or a slider cannot say which row is reached
	// (`aria-activedescendant` is not theirs), and a menu opened from one
	// (the table's More, a callout's kind, Shift-F10 on a link or a size
	// handle) was silent: the arrows reached rows a screen reader never read.
	it('a menu opened from a button, a link or a size handle takes the focus to say each row, and gives it back', () => {
		const editor = bound('<p>Hello</p>');
		expect(holdsActiveDescendant(editor.view.dom)).toBe(true);
		const link = document.createElement('a');
		link.href = '/page/page_1';
		const slider = document.createElement('div');
		slider.setAttribute('role', 'slider');
		slider.tabIndex = 0;
		const more = document.createElement('button');
		for (const owner of [more, link, slider]) {
			expect(holdsActiveDescendant(owner), owner.tagName).toBe(false);
			const menu = document.createElement('div');
			menu.id = 'context-menu';
			menu.setAttribute('role', 'menu');
			document.body.append(owner, menu);
			owner.focus();
			const told = pointFocusAt(owner, 'context-menu');
			told.highlight('context-menu-row-0');
			expect(document.activeElement, owner.tagName).toBe(menu);
			expect(menu.getAttribute('aria-activedescendant')).toBe('context-menu-row-0');
			expect(owner.hasAttribute('aria-activedescendant')).toBe(false);
			told.highlight('context-menu-row-1');
			expect(menu.getAttribute('aria-activedescendant')).toBe('context-menu-row-1');
			menu.remove();
			told.release();
			expect(document.activeElement, owner.tagName).toBe(owner);
			owner.remove();
		}
		// What the menu ran put the focus elsewhere: it stays there.
		const elsewhere = document.createElement('input');
		const menu = document.createElement('div');
		menu.id = 'context-menu';
		document.body.append(more, menu, elsewhere);
		more.focus();
		const told = pointFocusAt(more, 'context-menu');
		told.highlight(null);
		expect(document.activeElement).toBe(menu);
		elsewhere.focus();
		told.release();
		expect(document.activeElement).toBe(elsewhere);
		// The table's bar stays while More's menu has the keys.
		expect(read('../components/pages/TableMenu.svelte')).toMatch(
			/if \(to\?\.closest\?\.\('\[role="menu"\]'\)\) keepWhileMenuHasKeys\(\);\s+else onFocusChange\?\.\(false\);/,
		);
	});

	// The @ picker took Enter from the window and picked the highlighted row,
	// which a row Tab reached never was: Tab to a result, Enter, and the
	// first was inserted. Its rows were no list, and the highlight the
	// arrows moved was never read.
	it('the @ picker’s Enter picks the row Tab reached, and its input points at the row the arrows reach', () => {
		const picker = read('../components/RefPicker.svelte');
		const row = picker.match(/<button\s+class="result-item"[\s\S]*?type="button"\s*>/)?.[0] ?? '';
		expect(row).toMatch(/onfocus=\{\(\) => \(selectedIndex = globalIndex\)\}/);
		expect(row).toMatch(/id=\{optionId\(globalIndex\)\}\s+role="option"\s+aria-selected=/);
		expect(picker).toMatch(/if \(e\.key === 'Enter'\) \{\s+e\.preventDefault\(\);\s+const item = flatResults\[selectedIndex\];/);
		expect(picker).toMatch(/id=\{LIST_ID\}\s+role="listbox"/);
		const input = picker.match(/<input[\s\S]*?\/>/)?.[0] ?? '';
		expect(input).toMatch(/role="combobox"/);
		expect(input).toMatch(/aria-label=\{placeholder\}/);
		expect(input).toMatch(/aria-controls=\{LIST_ID\}/);
		expect(input).toMatch(/aria-activedescendant=\{flatResults\.length \? optionId\(selectedIndex\) : undefined\}/);
	});

	// Proposals were told apart by colour and strikethrough alone, and the bar
	// that settles one appeared without a word.
	it('a proposal reads as a suggested deletion and insertion, and the caret coming into one is said', () => {
		const editor = bound('<p>The <virtues-del proposal="p1">old</virtues-del><virtues-ins proposal="p1">new <strong>bold</strong></virtues-ins> plan.</p>');
		const del = [...editor.view.dom.querySelectorAll('[role="deletion"]')];
		const ins = [...editor.view.dom.querySelectorAll('[role="insertion"]')];
		expect(del.map((el) => [el.textContent, el.getAttribute('aria-roledescription')])).toEqual([['old', 'suggested deletion']]);
		expect(ins.map((el) => [el.textContent, el.getAttribute('aria-roledescription')])).toEqual([['new bold', 'suggested insertion']]);
		// The page's HTML holds none of it.
		expect(editor.getHTML()).not.toMatch(/role=|aria-/);
		editor.commands.setTextSelection(posIn(editor, 'new', 1));
		editor.commands.insertContent('x');
		expect(editor.view.dom.querySelector('[role="insertion"]')?.textContent).toBe('nxew bold');

		const keys = { accept: '⌘⌥ENTER', reject: '⌘⌥BACKSPACE' };
		expect(proposalSaid({ id: 'p1', deletes: true, inserts: true }, keys)).toBe('Suggested edit, a rewrite. ⌘⌥ENTER accepts it, ⌘⌥BACKSPACE rejects it.');
		expect(proposalSaid({ id: 'p2', deletes: false, inserts: true }, keys)).toMatch(/^Suggested edit, an insertion\./);
		const view = read('../components/pages/DocumentEditor.svelte');
		expect(view).toMatch(/<div class="sr-only" aria-live="polite">\{suggestionSaid\}<\/div>/);
		expect(view).toMatch(/if \(suggestion\?\.id !== p\.id\) \{\s+suggestionSaid = proposalSaid\(p, \{/);
	});

	// Another device's caret sits in the text, its name in a label: a screen
	// reader read "The planPhone is good".
	it('another device’s caret and its name are out of what a screen reader reads, as the assistant’s are', async () => {
		const { doc } = pageDoc('<p>The plan is good</p>');
		const replica = new Y.Doc();
		Y.applyUpdate(replica, Y.encodeStateAsUpdate(doc));
		const editors = [doc, replica].map((ydoc, i) => {
			const provider = fakeProvider(ydoc);
			cleanups.push(provider.destroy);
			const element = document.createElement('div');
			document.body.append(element);
			const editor = track(
				createPageEditor({
					element,
					doc: { ydoc, provider },
					editable: true,
					placeholder: 'Write',
					user: i === 0 ? { name: 'Computer', tint: 'orange' } : { name: 'Phone', tint: 'cyan' },
				}),
			);
			return { editor, provider, ydoc };
		});
		const [computer, phone] = editors;
		const ys = ySyncPluginKey.getState(phone.editor.state) as { type: Y.XmlFragment; binding: { mapping: Map<unknown, unknown> } };
		const at = absolutePositionToRelativePosition(posIn(phone.editor, 'The plan', 'The plan'.length), ys.type, ys.binding.mapping as never);
		phone.provider.awareness.setLocalStateField('cursor', { anchor: Y.relativePositionToJSON(at), head: Y.relativePositionToJSON(at) });
		applyAwarenessUpdate(computer.provider.awareness, encodeAwarenessUpdate(phone.provider.awareness, [phone.ydoc.clientID]), 'remote');
		await settle(50);
		const caret = computer.editor.view.dom.querySelector('.collaboration-carets__caret');
		expect(caret?.textContent).toBe('Phone');
		expect(caret?.getAttribute('aria-hidden')).toBe('true');
		expect(renderCaret({ name: 'Tablet' }).getAttribute('aria-hidden')).toBe('true');
	});

	it('a menu of choices (a column’s alignment, a callout’s kind) says which is selected, not “you are here”', () => {
		const editor = bound('<table><tbody><tr><th><p>Day</p></th></tr><tr><td><p>Fri</p></td></tr></tbody></table>');
		editor.commands.setTextSelection(posIn(editor, 'Fri'));
		const rows = tableMenuItems(tableActions(editor));
		expect(rows.filter((r) => r.role === 'menuitemradio').map((r) => r.id)).toEqual(['align-left', 'align-center', 'align-right']);
		expect(rows.find((r) => r.id === 'delete-row')?.role).toBeUndefined();
		expect(read('../components/contextMenu/ContextMenuItem.svelte')).toMatch(/role=\{item\.role\}/);
		expect(read('../components/pages/nodes/CalloutNode.svelte')).toMatch(/checked: t\.tone === current\.tone,\s+role: "menuitemradio" as const,/);
		for (const file of ['../components/pages/DocumentEditor.svelte', '../components/pages/TableMenu.svelte']) {
			expect(read(file), file).toMatch(/contextMenu\.show\(\{ x: [^}]+\}, tableMenuItems\(/);
		}
	});
});

describe("the platform's own undo", () => {
	// Shake to undo, three fingers, the iPad's undo key and Edit > Undo
	// arrive as input, not as ⌘Z.
	it('takes historyUndo and historyRedo to the editor’s own undo', async () => {
		const editor = bound('<p>Hello</p>');
		editor.commands.setTextSelection(posIn(editor, 'Hello', 5));
		editor.commands.insertContent(' world');
		await settle(600);
		expect(editor.state.doc.textContent).toBe('Hello world');
		const undo = new InputEvent('beforeinput', { inputType: 'historyUndo', bubbles: true, cancelable: true });
		editor.view.dom.dispatchEvent(undo);
		expect(undo.defaultPrevented).toBe(true);
		expect(editor.state.doc.textContent).toBe('Hello');
		editor.view.dom.dispatchEvent(new InputEvent('beforeinput', { inputType: 'historyRedo', bubbles: true, cancelable: true }));
		expect(editor.state.doc.textContent).toBe('Hello world');
	});
});

describe('⌘K', () => {
	function withSearch() {
		const search = vi.fn();
		cleanups.push(shortcuts.register({ id: 'search.toggle', keys: 'mod+k', label: 'Ask or search', run: search }));
		return search;
	}

	// The app's ⌘K (Ask or search) listens first, on the window: the editor
	// claims it only while it has something to link.
	it('links the selection in a page, and is the app’s search everywhere else', () => {
		const search = withSearch();
		const linked = vi.fn(() => true);
		const linkKey = Extension.create({ name: 'linkKey', addKeyboardShortcuts: () => ({ 'Mod-k': linked }) });
		const editor = bound('<p>See the docs today.</p>', [linkKey]);
		cleanups.push(shortcuts.claim(editor.view.dom, 'mod+k', () => canEditLinkAtSelection(editor.state)));
		const from = posIn(editor, 'docs');
		editor.commands.setTextSelection({ from, to: from + 4 });
		press(editor, 'k', { mod: true });
		expect(linked).toHaveBeenCalledOnce();
		expect(search).not.toHaveBeenCalled();

		editor.commands.setTextSelection(from);
		press(editor, 'k', { mod: true });
		expect(search).toHaveBeenCalledOnce();
	});

	it('never rewrites a selected widget, or a range holding one, as text', () => {
		const editor = track(
			htmlEditor('<p>Before.</p><img src="/api/media/harbour.png" alt="Harbour"><p>Lunch with <virtues-mention to="/person/person_1" label="Nick"></virtues-mention> today.</p>'),
		);
		editor.view.dispatch(editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, posIn(editor, 'Before.') + 8)));
		expect(editor.state.selection instanceof NodeSelection).toBe(true);
		expect(canEditLinkAtSelection(editor.state)).toBe(false);
		expect(editLinkAtSelection(editor)).toBe(false);

		const mention = posIn(editor, 'Lunch with ') + 'Lunch with '.length;
		editor.view.dispatch(editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, mention)));
		expect(editLinkAtSelection(editor)).toBe(false);
		editor.commands.setTextSelection({ from: mention - 5, to: mention + 4 });
		expect(editLinkAtSelection(editor)).toBe(false);
		expect(linkEditor.show).not.toHaveBeenCalled();
		expect(editor.getHTML()).toContain('<virtues-mention to="/person/person_1" label="Nick"></virtues-mention>');
	});

	it('links text kept as it read where it stands, across blocks too', () => {
		const editor = track(htmlEditor('<p>One <strong>two</strong></p><p>three</p>'));
		editor.commands.setTextSelection({ from: 1, to: posIn(editor, 'three', 5) });
		expect(editLinkAtSelection(editor)).toBe(true);
		const [values, save] = vi.mocked(linkEditor.show).mock.calls[0] as unknown as [{ label: string }, (v: { label: string; href: string }) => void];
		save({ label: values.label, href: 'https://example.com' });
		expect(editor.getHTML()).toBe(
			'<p><a href="https://example.com">One <strong>two</strong></a></p><p><a href="https://example.com">three</a></p>',
		);
	});
});

describe('the link panel', () => {
	// Its field holds the keyboard and is removed with it: closing it left a
	// keyboard user on the page's body, out of the page.
	it('closing it, by Escape, Cancel or a save that does not focus, puts the keyboard back in the editor', async () => {
		const { linkEditor: panel } = await vi.importActual<typeof import('$lib/stores/linkEditor.svelte')>('$lib/stores/linkEditor.svelte');
		const editor = bound('<p>See docs</p>');
		const field = document.createElement('input');
		document.body.append(field);
		for (const close of [() => panel.hide(), () => panel.save()]) {
			editor.view.dom.focus();
			expect(document.activeElement).toBe(editor.view.dom);
			panel.show({ label: 'docs', href: 'https://example.com' }, () => {});
			field.focus();
			close();
			expect(document.activeElement).toBe(editor.view.dom);
		}
	});
});

describe('links', () => {
	// The CodeMirror editor previews every link, a link elsewhere as its address.
	it('previews a link elsewhere as its address, and a ref as its record', async () => {
		const hovered = vi.fn();
		const editor = track(
			htmlEditor('<p><a href="https://example.com/docs">docs</a> and <a href="/person/person_1">Nick</a></p>', [
				links({ onHover: hovered, onLeave: () => {} }),
			]),
		);
		const web = editor.view.dom.querySelector('.doc-web-link') as HTMLElement;
		expect(web?.dataset.refHref).toBe('https://example.com/docs');
		expect(editor.view.dom.querySelector('.doc-ref-link')?.getAttribute('data-ref-href')).toBe('/person/person_1');
		web.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }));
		await settle(400);
		expect(hovered).toHaveBeenCalledWith(web, 'https://example.com/docs', 'docs');
		expect(previewType('https://example.com/docs')).toBe('link');
		expect(previewType('/person/person_1')).toBe('person');
	});

	it('on a page that is read only, a tap opens a link and the menu still opens, with nothing that edits', () => {
		const editor: Editor = bound('<p>See <a href="/person/person_1">Nick</a> today.</p>', [
			links(),
			contextGesture([linkContext((): Editor => editor)]),
		], false);
		const a = editor.view.dom.querySelector('a')!;
		const click = new MouseEvent('click', { bubbles: true, cancelable: true, button: 0 });
		a.dispatchEvent(click);
		expect(click.defaultPrevented).toBe(true);
		expect(windowShellStore.navigate).toHaveBeenCalledWith('/person/person_1', { label: 'Nick' });

		editor.view.posAtCoords = () => ({ pos: posIn(editor, 'Nick', 1), inside: -1 });
		const menu = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 5, clientY: 5 });
		a.dispatchEvent(menu);
		expect(menu.defaultPrevented).toBe(true);
		const items = vi.mocked(contextMenu.show).mock.calls[0][1].map((i) => i.id);
		expect(items).toEqual(['open', 'open-beside', 'copy-link']);
	});
});

describe('a click with ⌘ or Ctrl', () => {
	/** A click at `pos`, `inside` the node there, with ⌘ and Ctrl held, as the browser delivers it. */
	function modClick(editor: Editor, target: Element, pos: number, inside: number) {
		editor.view.posAtCoords = () => ({ pos, inside });
		// At the top: no layout here, and a click below the last block's (zero) bottom is a click below the page.
		const init = { bubbles: true, cancelable: true, button: 0, ctrlKey: true, metaKey: true, clientX: 0, clientY: 0 };
		target.dispatchEvent(new MouseEvent('mousedown', init));
		target.dispatchEvent(new MouseEvent('mouseup', init));
	}

	// ⌘ (Ctrl elsewhere) is ProseMirror's key for selecting the block
	// clicked in, drawn as nothing: a click that just missed a link took the
	// whole paragraph, and the next key replaced it.
	it('off a link places the caret, and the next key replaces nothing', () => {
		const editor = bound('<p>First paragraph, kept.</p><p>Second line with a <a href="https://example.com">link</a> inside.</p>', [links()]);
		const second = editor.view.dom.querySelectorAll('p')[1];
		const at = posIn(editor, 'Second line', 'Second'.length);
		modClick(editor, second, at, editor.state.doc.child(0).nodeSize);
		expect(editor.state.selection).toBeInstanceOf(TextSelection);
		expect(editor.state.selection.from).toBe(at);
		editor.view.someProp('handleTextInput', (f) => f(editor.view, at, at, 'a', () => editor.state.tr.insertText('a', at, at)))
			|| editor.view.dispatch(editor.state.tr.insertText('a', at, at));
		expect(editor.state.doc.child(1).textContent).toBe('Seconda line with a link inside.');
		expect(editor.state.doc.child(0).textContent).toBe('First paragraph, kept.');
	});

	it('on a widget selects it, as a plain click does', () => {
		const editor = bound('<p>Above</p><img src="/api/media/harbour.png" alt="Harbour"><p>After</p>', [links()]);
		const image = editor.state.doc.child(0).nodeSize;
		modClick(editor, editor.view.dom, image, image);
		expect(editor.state.selection).toBeInstanceOf(NodeSelection);
		expect((editor.state.selection as NodeSelection).node.type.name).toBe('image');
	});
});

describe('find and replace', () => {
	const findIn = (html: string) => track(htmlEditor(html, [find({ onOpen: () => {}, onClose: () => {} })]));

	it('matches case, whole words and patterns when asked', () => {
		const editor = findIn('<p>Plan the plan; planning.</p>');
		setFindQuery(editor.view, 'plan');
		expect(findState(editor.state).matches).toHaveLength(3);
		setFindQuery(editor.view, 'plan', { caseSensitive: true, regexp: false, wholeWord: false });
		expect(findState(editor.state).matches).toHaveLength(2);
		setFindQuery(editor.view, 'plan', { caseSensitive: false, regexp: false, wholeWord: true });
		expect(findState(editor.state).matches).toHaveLength(2);
		setFindQuery(editor.view, 'plan\\w+', { caseSensitive: false, regexp: true, wholeWord: false });
		expect(findState(editor.state).matches.map((m) => editor.state.doc.textBetween(m.from, m.to))).toEqual(['planning']);
		setFindQuery(editor.view, 'plan(', { caseSensitive: false, regexp: true, wholeWord: false });
		expect(findState(editor.state)).toMatchObject({ invalid: true, matches: [] });
	});

	it('replaces the current match, keeping its marks, then all the rest as one step', () => {
		const editor = findIn('<p>Lunch on <strong>Friday</strong>, then Friday and friday.</p>');
		setFindQuery(editor.view, 'friday');
		expect(findState(editor.state).current).toBe(0);
		expect(replaceCurrent(editor.view, 'Saturday')).toBe(true);
		expect(editor.getHTML()).toBe('<p>Lunch on <strong>Saturday</strong>, then Friday and friday.</p>');
		expect(findState(editor.state).matches).toHaveLength(2);
		const steps = editor.state.doc;
		expect(replaceAll(editor.view, 'Sunday')).toBe(2);
		expect(editor.getHTML()).toBe('<p>Lunch on <strong>Saturday</strong>, then Sunday and Sunday.</p>');
		expect(editor.state.doc).not.toBe(steps);

		const pattern = findIn('<p>2026-10-08 and 2026-11-01</p>');
		setFindQuery(pattern.view, '(\\d+)-(\\d+)-(\\d+)', { caseSensitive: false, regexp: true, wholeWord: false });
		replaceAll(pattern.view, '$3/$2/$1');
		expect(pattern.getHTML()).toBe('<p>08/10/2026 and 01/11/2026</p>');
	});

	it('never replaces a match holding a widget', () => {
		const editor = findIn('<p>Lunch with <virtues-mention to="/person/person_1" label="Nick"></virtues-mention> today</p>');
		setFindQuery(editor.view, 'with ￼', { caseSensitive: false, regexp: false, wholeWord: false });
		expect(findState(editor.state).matches).toHaveLength(1);
		expect(replaceAll(editor.view, 'alone')).toBe(0);
		expect(editor.getHTML()).toContain('virtues-mention');
	});
});

describe('tables', () => {
	const TABLE =
		'<table><tbody><tr><th><p>Day</p></th><th><p>Cost</p></th><th><p>Note</p></th></tr>'
		+ '<tr><td><p>Fri</p></td><td><p>40</p></td><td><p>Taxi</p></td></tr>'
		+ '<tr><td><p>Sat</p></td><td><p>12</p></td><td><p>Tram</p></td></tr></tbody></table>';
	const rows = (editor: Editor) => {
		const out: string[] = [];
		editor.state.doc.firstChild!.forEach((row) => {
			const cells: string[] = [];
			row.forEach((c) => cells.push(c.textContent));
			out.push(cells.join('|'));
		});
		return out;
	};

	// The CodeMirror editor's drag handles reorder rows and columns.
	it('moves a row and a column, ids and all, the header row staying first', () => {
		const editor = track(htmlEditor(TABLE));
		editor.commands.setTextSelection(posIn(editor, 'Sat'));
		expect(tableActions(editor).map((a) => a.id)).toEqual(expect.arrayContaining(['move-row-up', 'move-col-right']));
		expect(tableActions(editor).map((a) => a.id)).not.toContain('move-row-down');
		expect(moveRow(editor, -1)).toBe(true);
		expect(rows(editor)).toEqual(['Day|Cost|Note', 'Sat|12|Tram', 'Fri|40|Taxi']);
		editor.commands.setTextSelection(posIn(editor, 'Sat'));
		expect(moveRow(editor, -1)).toBe(false);
		expect(moveColumn(editor, 1)).toBe(true);
		expect(rows(editor)).toEqual(['Cost|Day|Note', '12|Sat|Tram', '40|Fri|Taxi']);
		expect(tableActions(editor).find((a) => a.id === 'move-row-up')).toBeUndefined();
	});

	// A row carries an id, as a list item does: the model edits a long
	// table by its rows, so a row the editor adds gets one too.
	it('gives a row the editor adds an id of its own', async () => {
		const editor = bound(TABLE);
		editor.commands.setTextSelection(posIn(editor, 'Sat'));
		editor.commands.addRowAfter();
		await settle();
		const rows: unknown[] = [];
		editor.state.doc.firstChild!.forEach((r) => rows.push(r.attrs.id));
		expect(rows).toHaveLength(4);
		expect(rows.every((id) => typeof id === 'string' && id.length === 8)).toBe(true);
		expect(new Set(rows).size).toBe(4);
	});
});

describe('the selection toolbar', () => {
	// The bar is how a phone formats: its state was only drawn, so a screen
	// reader read Bold the same whether the words were bold or not.
	it('each mark button says whether its mark is on, and names its key as the keyboard does', () => {
		const source = read('../components/SelectionToolbar.svelte');
		expect(source).toMatch(/aria-pressed=\{activeMarks\[btn\.mark\]\}/);
		expect(source).toMatch(/aria-label=\{btn\.label\}/);
		expect(source).toMatch(/title=\{btn\.shortcut \? `\$\{btn\.label\} \(\$\{shortcuts\.format\(btn\.shortcut\)\}\)` : btn\.label\}/);
		expect(source).not.toMatch(/Cmd\+/);
		expect(shortcuts.format('mod+shift+s')).toBe(isAppleKeyboard ? '⌘⇧S' : 'Ctrl+Shift+S');
	});

	it('waits out a drag and shows on release, wherever the pointer is let go', () => {
		const editor = track(htmlEditor('<p>Sweep these words</p>'));
		const show = vi.fn();
		const timing = toolbarTiming(editor.view, show);
		cleanups.push(timing.destroy);
		editor.view.dom.dispatchEvent(new PointerEvent('pointerdown', { button: 0, bubbles: true }));
		const tr = editor.state.tr.setSelection(TextSelection.create(editor.state.doc, 1, 6)).setMeta('pointer', true);
		editor.view.dispatch(tr);
		timing.note(tr);
		expect(timing.ready()).toBe(false);
		window.dispatchEvent(new PointerEvent('pointerup', { button: 0 }));
		expect(show).toHaveBeenCalledOnce();
		expect(timing.ready()).toBe(true);
	});

	it('shows for a keyboard selection once it has held still', async () => {
		const editor = track(htmlEditor('<p>Shift arrow over words</p>'));
		const show = vi.fn();
		const timing = toolbarTiming(editor.view, show, 30);
		cleanups.push(timing.destroy);
		for (const to of [2, 3, 4]) {
			const tr = editor.state.tr.setSelection(TextSelection.create(editor.state.doc, 1, to));
			editor.view.dispatch(tr);
			timing.note(tr);
			expect(timing.ready()).toBe(false);
		}
		await settle(60);
		expect(show).toHaveBeenCalledOnce();
		expect(timing.ready()).toBe(true);
		expect(KEYBOARD_SHOW_DELAY_MS).toBe(200);
	});

	// WKWebView paints the keyboard over the page without resizing it.
	it('on touch sits under the selection, unless the keyboard leaves no room there', () => {
		const start = { left: 10, top: 400 };
		const end = { left: 110, bottom: 420 };
		const under = 420 + TOOLBAR_CLEARANCE;
		expect(selectionToolbarAt(start, end, { coarse: false, bottom: 800 })).toEqual({ x: 60, y: 400 });
		expect(selectionToolbarAt(start, end, { coarse: true, bottom: 800 })).toEqual({ x: 60, y: under });
		expect(selectionToolbarAt(start, end, { coarse: true, bottom: under - 1 })).toEqual({ x: 60, y: 400 });
	});

	// The bar sits above its anchor: placed by its old 28px buttons' height,
	// the 44pt bar covered the selection's last line and its end handle.
	it('on touch the bar itself clears the selection and the handle under it', async () => {
		const bar = read('../components/SelectionToolbar.svelte');
		expect(bar).toMatch(/@media \(pointer: coarse\) \{\s+\.toolbar-btn \{\s+width: 44px;\s+height: 44px;/);
		expect(bar).toMatch(/\.selection-toolbar \{[^}]*padding: 3px;[^}]*border: 1px solid/);
		expect(bar).toMatch(/placement: 'top', offset: TOOLBAR_OFFSET/);
		expect(TOUCH_TOOLBAR_HEIGHT).toBeGreaterThanOrEqual(44 + 2 * 3 + 2 * 1);

		const end = { left: 110, bottom: 420 };
		const anchor = selectionToolbarAt({ left: 10, top: 400 }, end, { coarse: true, bottom: 800 });
		const platform = {
			getElementRects: async () => ({
				reference: { x: anchor.x, y: anchor.y, width: 0, height: 0 },
				floating: { x: 0, y: 0, width: 300, height: TOUCH_TOOLBAR_HEIGHT },
			}),
			getDimensions: async () => ({ width: 300, height: TOUCH_TOOLBAR_HEIGHT }),
			getClippingRect: async () => ({ x: 0, y: 0, width: 400, height: 800 }),
			convertOffsetParentRelativeRectToViewportRelativeRect: async ({ rect }: { rect: unknown }) => rect,
			getDocumentElement: () => document.documentElement,
			isElement: () => false,
			getOffsetParent: async () => window,
			getClientRects: () => [],
			isRTL: () => false,
			getScale: async () => ({ x: 1, y: 1 }),
		};
		const placed = await computePosition({ getBoundingClientRect: () => new DOMRect(anchor.x, anchor.y, 0, 0) } as never, document.createElement('div'), {
			placement: 'top',
			strategy: 'fixed',
			middleware: floatingMiddleware({ offset: TOOLBAR_OFFSET }, 0, { width: 400, height: 800 }),
			platform: platform as never,
		});
		expect(placed.placement).toBe('top');
		expect(placed.y).toBeGreaterThanOrEqual(end.bottom + SELECTION_HANDLE_ROOM);
		expect(SELECTION_HANDLE_ROOM).toBeGreaterThanOrEqual(16);
	});

	it('a floating bar keeps above the keyboard', async () => {
		const platform = {
			getElementRects: async () => ({
				reference: { x: 100, y: 480, width: 0, height: 0 },
				floating: { x: 0, y: 0, width: 200, height: 36 },
			}),
			getDimensions: async () => ({ width: 200, height: 36 }),
			getClippingRect: async ({ rootBoundary }: { rootBoundary: unknown }) =>
				typeof rootBoundary === 'object' && rootBoundary ? rootBoundary : { x: 0, y: 0, width: 400, height: 800 },
			convertOffsetParentRelativeRectToViewportRelativeRect: async ({ rect }: { rect: unknown }) => rect,
			getDocumentElement: () => document.documentElement,
			isElement: () => false,
			getOffsetParent: async () => window,
			getClientRects: () => [],
			isRTL: () => false,
			getScale: async () => ({ x: 1, y: 1 }),
		};
		const place = (inset: number) =>
			computePosition({ getBoundingClientRect: () => new DOMRect(100, 480, 0, 0) } as never, document.createElement('div'), {
				placement: 'bottom-start',
				strategy: 'fixed',
				middleware: floatingMiddleware({ offset: 8 }, inset, { width: 400, height: 800 }),
				platform: platform as never,
			});
		expect((await place(0)).y).toBe(488);
		const above = await place(320);
		expect(above.y + 36).toBeLessThanOrEqual(480);
	});
});

describe('keyboard paths', () => {
	it('Shift-F10 opens a selected widget’s own menu, and the menu of a link at the caret', () => {
		const opened = vi.fn();
		const editor: Editor = track(
			htmlEditor('<p>See <a href="https://example.com">docs</a></p><img src="/api/media/harbour.png" alt="Harbour">', [
				contextGesture([linkContext((): Editor => editor)]),
			]),
		);
		// A widget's node view opens its menu on the gesture, as MediaNode does.
		const image = editor.view.dom.querySelector('img')!;
		onContextGesture(image, opened);
		const at = posIn(editor, 'docs') + 'docs'.length + 1;
		editor.view.dispatch(editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, at)));
		press(editor, 'F10', { shift: true });
		expect(opened).toHaveBeenCalledOnce();
		// Opened from the keyboard, it starts on its first row.
		expect(contextMenu.reachFirstRow).toHaveBeenCalledOnce();

		editor.commands.setTextSelection(posIn(editor, 'docs', 2));
		expect(openContextFromKeyboard(editor.view, [linkContext(() => editor)])).toBe(true);
		expect(vi.mocked(contextMenu.show).mock.calls.at(-1)?.[1].map((i) => i.id)).toContain('edit');
		expect(contextMenu.reachFirstRow).toHaveBeenCalledTimes(2);
		// Nothing opened, nothing reached.
		editor.commands.setTextSelection(posIn(editor, 'See', 1));
		expect(openContextFromKeyboard(editor.view, [linkContext(() => editor)])).toBe(false);
		expect(contextMenu.reachFirstRow).toHaveBeenCalledTimes(2);
	});

	// ProseMirror runs key bindings only while the page is editable, and a
	// read-only page has no caret: the focused link's or widget's menu.
	it('on a page that is read only, Shift-F10 opens the focused link’s menu, or the focused widget’s', () => {
		const editor: Editor = bound(
			'<p>See <a href="https://example.com">docs</a> and <virtues-mention to="/person/person_1" label="Nick"></virtues-mention></p>',
			[contextGesture([linkContext((): Editor => editor)])],
			false,
		);
		const link = editor.view.dom.querySelector('a')!;
		link.focus();
		const key = new KeyboardEvent('keydown', { key: 'F10', shiftKey: true, bubbles: true, cancelable: true });
		link.dispatchEvent(key);
		expect(key.defaultPrevented).toBe(true);
		expect(vi.mocked(contextMenu.show).mock.calls.at(-1)?.[1].map((i) => i.id)).toEqual(expect.arrayContaining(['open', 'copy-link']));
		expect(vi.mocked(contextMenu.show).mock.calls.at(-1)?.[1].map((i) => i.id)).not.toContain('edit');

		const widget = editor.view.dom.querySelector('[data-type="mention"], virtues-mention, .doc-widget') as HTMLElement;
		const control = document.createElement('button');
		widget.append(control);
		const opened = vi.fn();
		onContextGesture(widget, opened);
		control.focus();
		control.dispatchEvent(new KeyboardEvent('keydown', { key: 'ContextMenu', bubbles: true, cancelable: true }));
		expect(opened).toHaveBeenCalledOnce();
	});

	it('⌘⌥Enter accepts the proposal at the caret and ⌘⌥Backspace rejects it', () => {
		const editor = track(
			htmlEditor(
				'<p>The <virtues-del proposal="p1">old</virtues-del><virtues-ins proposal="p1">new</virtues-ins> plan.</p><p><virtues-ins proposal="p2">Added.</virtues-ins></p>',
				[proposalKeys()],
			),
		);
		editor.commands.setTextSelection(posIn(editor, 'new', 1));
		press(editor, 'Enter', { mod: true, alt: true });
		expect(editor.getHTML()).toContain('<p>The new plan.</p>');
		editor.commands.setTextSelection(posIn(editor, 'Added', 1));
		press(editor, 'Backspace', { mod: true, alt: true });
		expect(proposalIds(editor.state)).toEqual([]);
		expect(editor.getHTML()).toBe('<p>The new plan.</p>');
	});

	it('a size handle moves by key, within its range', () => {
		const range = { min: 48, max: 600, step: 16, page: 160 };
		expect(sizeByKey('ArrowRight', 300, range)).toBe(316);
		expect(sizeByKey('ArrowLeft', 50, range)).toBe(48);
		expect(sizeByKey('PageUp', 500, range)).toBe(600);
		expect(sizeByKey('End', 300, range)).toBe(600);
		expect(sizeByKey('a', 300, range)).toBeNull();
		for (const [file, label] of [
			['../components/pages/nodes/MediaNode.svelte', 'Image width'],
			['../components/pages/nodes/AppletNode.svelte', 'Applet height'],
		]) {
			const source = read(file);
			expect(source, file).toMatch(/role="slider"\s+tabindex="0"/);
			expect(source, file).toContain(`aria-label="${label}"`);
			expect(source, file).toMatch(/aria-valuenow=/);
			expect(source, file).toMatch(/onkeydown=\{resizeByKey\}/);
		}
		// An applet's height has a way that is no drag at all.
		expect(read('../components/pages/nodes/AppletNode.svelte')).toMatch(/id: "height-short"/);
	});

	it('a drag on a size handle resizes along its axis only: a swipe across it, or one the browser took, writes nothing', () => {
		const handle = document.createElement('div');
		document.body.append(handle);
		const drag = (moves: [number, number][], end: 'pointerup' | 'pointercancel' = 'pointerup') => {
			const sizes: number[] = [];
			let wrote: number | null | undefined;
			const down = new PointerEvent('pointerdown', { clientX: 100, clientY: 100, pointerId: 1, bubbles: true, cancelable: true });
			dragToResize(handle, down, { axis: 'x', start: 300, min: 48, max: 600, onSize: (n) => sizes.push(n), onEnd: (n) => (wrote = n) });
			for (const [x, y] of moves) handle.dispatchEvent(new PointerEvent('pointermove', { clientX: x, clientY: y, pointerId: 1 }));
			handle.dispatchEvent(new PointerEvent(end, { pointerId: 1 }));
			return { sizes, wrote };
		};
		// Sideways: a resize, written once when the pointer lifts.
		expect(drag([[103, 101], [140, 104], [160, 106]])).toEqual({ sizes: [340, 360], wrote: 360 });
		// A scroll begun on the handle, straight down the page: nothing drawn, nothing written.
		expect(drag([[101, 90], [102, 40], [103, -100]])).toEqual({ sizes: [], wrote: null });
		// The browser took the gesture.
		expect(drag([[140, 100]], 'pointercancel')).toEqual({ sizes: [340], wrote: null });

		// On touch a handle takes the pointer only on a selected widget.
		for (const [file, selected] of [
			['../components/pages/nodes/MediaNode.svelte', '.doc-media.selected .doc-image-handle'],
			['../components/pages/nodes/AppletNode.svelte', '.doc-applet.selected .doc-applet-handle'],
		]) {
			const source = read(file);
			const coarse = source.slice(source.indexOf('@media (pointer: coarse)'));
			expect(coarse, file).toMatch(/-handle \{[^}]*pointer-events: none;\s+touch-action: auto;/);
			expect(coarse, file).toContain(`${selected} {\n\t\t\tpointer-events: auto;\n\t\t\ttouch-action: none;`);
			expect(source, file).toMatch(/dragToResize\(e\.currentTarget as HTMLElement, e, \{/);
		}
	});

	// Anchored at a tall table's top edge, the bar sat above the window once
	// that edge scrolled away, and Alt-F10 put the keyboard in it unseen.
	it('the table’s bar stays in view while any of the table is, however tall', async () => {
		const BAR = { width: 300, height: 44 + 2 * 4 + 2 * 1 };
		const view = { top: 0, width: 1280, height: 800 };
		const place = (anchor: { x: number; y: number; width: number }) => {
			const platform = {
				getElementRects: async () => ({ reference: { ...anchor, height: 0 }, floating: { x: 0, y: 0, ...BAR } }),
				getDimensions: async () => BAR,
				getClippingRect: async () => ({ x: 0, y: 0, width: view.width, height: view.height }),
				convertOffsetParentRelativeRectToViewportRelativeRect: async ({ rect }: { rect: unknown }) => rect,
				getDocumentElement: () => document.documentElement,
				isElement: () => false,
				getOffsetParent: async () => window,
				getClientRects: () => [],
				isRTL: () => false,
				getScale: async () => ({ x: 1, y: 1 }),
			};
			return computePosition({ getBoundingClientRect: () => new DOMRect(anchor.x, anchor.y, anchor.width, 0) } as never, document.createElement('div'), {
				placement: 'top-end',
				strategy: 'fixed',
				middleware: floatingMiddleware({ offset: 6, flip: false, shift: true, padding: 8 }, 0, view),
				platform: platform as never,
			});
		};
		// The table's top 800px above the window, the caret in a row further down.
		const tall = { left: 100, top: -800, bottom: 600, width: 600 };
		expect((await place(tableBarAnchor(tall, 0))).y).toBeGreaterThanOrEqual(0);
		// Under a header that ends 120px down, the bar stays below it.
		expect((await place(tableBarAnchor(tall, 120))).y).toBeGreaterThanOrEqual(120);
		// A table whose top is in view keeps the bar over its top edge.
		expect(tableBarAnchor({ left: 100, top: 300, bottom: 600, width: 600 }, 0).y).toBe(300);
		// Once its last row has gone too, the bar goes with it.
		expect(tableBarAnchor({ left: 100, top: -900, bottom: -20, width: 600 }, 0).y).toBe(-20);
		expect(read('../components/pages/DocumentEditor.svelte')).toMatch(/tableAnchor = box \? tableBarAnchor\(box, top\) : null;/);
	});

	it('the table’s bar is reached by Alt-F10 and stays while the keyboard is in it', () => {
		const editorSource = read('../components/pages/DocumentEditor.svelte');
		expect(editorSource).toMatch(/"Alt-F10": \(\{ editor: e \}\) => \{\s+if \(!cellAt\(e\.state\) \|\| !tableMenu\) return false;\s+tableMenu\.focus\(\);/);
		expect(editorSource).toMatch(/e\.view\.hasFocus\(\) \|\| tableMenuFocused/);
		const menu = read('../components/pages/TableMenu.svelte');
		expect(menu).toMatch(/export function focus\(\)/);
		expect(menu).toMatch(/onfocusin=\{\(\) => onFocusChange\?\.\(true\)\}/);
	});

	// The Enter that commits a Japanese or Chinese candidate, the arrows that
	// choose one: an open menu or a field that acted on them would insert a
	// mention, run a command or save before the word was written.
	it('menus and fields that act on Enter, the arrows or Escape leave an input method’s keys be', () => {
		const key = (init: KeyboardEventInit & { keyCode?: number }) => {
			const event = new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...init });
			if (init.keyCode !== undefined) Object.defineProperty(event, 'keyCode', { value: init.keyCode });
			return event;
		};
		expect(inComposition(key({ key: 'Enter', isComposing: true }))).toBe(true);
		// WebKit's commit: `isComposing` already false.
		expect(inComposition(key({ key: 'Enter', keyCode: 229 }))).toBe(true);
		expect(inComposition(key({ key: 'Enter', keyCode: 13 }))).toBe(false);
		for (const [file, handler] of [
			['../components/SlashMenu.svelte', 'handleKeydown'],
			['../components/RefPicker.svelte', 'handleKeydown'],
			['../components/pages/AiPromptPopover.svelte', 'handleKeydown'],
			['../components/pages/LinkEditorPopover.svelte', 'onKeydown'],
			['../components/pages/FindBar.svelte', 'onKeydown'],
			['../components/pages/FindBar.svelte', 'onReplaceKeydown'],
		]) {
			expect(read(file), `${file} ${handler}`).toMatch(new RegExp(`function ${handler}\\(e: KeyboardEvent\\) \\{\\s+if \\(inComposition\\(e\\)\\) return;`));
		}
		expect(read('../floating/hooks/useEscapeKey.svelte.ts')).toMatch(/event\.key === 'Escape' && !inComposition\(event\)/);

		const menu: MenuKeyTarget = {
			visible: true,
			openSubmenuId: null,
			focusedIndex: 0,
			items: [{ id: 'open' }],
			closeSubmenu: vi.fn(),
			hide: vi.fn(),
			focusNext: vi.fn(),
			focusPrevious: vi.fn(),
			focusAt: vi.fn(),
			openSubmenu: vi.fn(),
			activateFocused: vi.fn(),
		};
		cleanups.push(listenForMenuKeys(menu));
		window.dispatchEvent(key({ key: 'Enter', keyCode: 229 }));
		expect(menu.activateFocused).not.toHaveBeenCalled();
		window.dispatchEvent(key({ key: 'Enter', keyCode: 13 }));
		expect(menu.activateFocused).toHaveBeenCalledOnce();
	});

	// The editor keeps the focus while a menu is open: a key the menu left
	// reached it, and Tab nested the list item behind the menu, Backspace
	// joined two blocks, a letter typed.
	it('keys pressed on an open menu are its own and never edit the page behind it', () => {
		const editor = track(htmlEditor('<ul><li><p>One</p></li><li><p>Two</p></li></ul><h2>Head</h2>'));
		const html = editor.getHTML();
		const menu: MenuKeyTarget = {
			visible: true,
			openSubmenuId: null,
			focusedIndex: 0,
			items: [{ id: 'open', label: 'Open' }, { id: 'edit', label: 'Edit link', disabled: true }, { id: 'embed', label: 'Embed' }, { id: 'copy', label: 'Copy link' }],
			closeSubmenu: vi.fn(),
			hide: vi.fn(),
			focusNext: vi.fn(),
			focusPrevious: vi.fn(),
			focusAt: vi.fn(),
			openSubmenu: vi.fn(),
			activateFocused: vi.fn(),
		};
		cleanups.push(listenForMenuKeys(menu));
		editor.commands.setTextSelection(posIn(editor, 'Two'));
		expect(press(editor, 'Tab').defaultPrevented).toBe(true);
		press(editor, 'Tab', { shift: true });
		expect(menu.focusNext).toHaveBeenCalledOnce();
		expect(menu.focusPrevious).toHaveBeenCalledOnce();
		editor.commands.setTextSelection(posIn(editor, 'Head'));
		for (const key of ['Backspace', 'Delete', 'Home', 'End', 'PageUp', 'PageDown', 'x']) {
			expect(press(editor, key).defaultPrevented, key).toBe(true);
		}
		expect(editor.getHTML()).toBe(html);
		// Home and Page Up reach the first row, End and Page Down the last; a letter the next row it starts.
		expect(vi.mocked(menu.focusAt).mock.calls.map(([i]) => i)).toEqual([0, 3, 0, 3]);
		press(editor, 'e');
		expect(vi.mocked(menu.focusAt).mock.calls.at(-1)).toEqual([2]);
		// A shortcut is the app's.
		expect(press(editor, 'k', { mod: true }).defaultPrevented).toBe(false);
	});

	// A menu opens while the editor keeps the focus; Enter on its item must
	// not also split the line under the caret.
	it('Enter on an open menu chooses its item and never reaches the editor', () => {
		const editor = track(htmlEditor('<p>Hello world</p>'));
		editor.commands.setTextSelection(posIn(editor, 'Hello', 5));
		const menu: MenuKeyTarget = {
			visible: true,
			openSubmenuId: null,
			focusedIndex: 0,
			items: [{ id: 'open' }],
			closeSubmenu: vi.fn(),
			hide: vi.fn(),
			focusNext: vi.fn(),
			focusPrevious: vi.fn(),
			focusAt: vi.fn(),
			openSubmenu: vi.fn(),
			activateFocused: vi.fn(),
		};
		cleanups.push(listenForMenuKeys(menu));
		press(editor, 'ArrowDown');
		press(editor, 'Enter');
		expect(menu.focusNext).toHaveBeenCalledOnce();
		expect(menu.activateFocused).toHaveBeenCalledOnce();
		expect(editor.state.doc.childCount).toBe(1);
		(menu as { visible: boolean }).visible = false;
		press(editor, 'Enter');
		expect(editor.state.doc.childCount).toBe(2);
	});
});

describe('on a phone', () => {
	const host: TreeCommandHost = { pickFiles: () => {}, pickApplet: () => {}, mention: () => {}, askAi: () => {} };
	const LIST = '<ul><li><p>One</p></li><li><p>Two</p></li></ul>';
	/** Whether "Two" is nested under "One": the first item holds its text and a list. */
	const nested = (editor: Editor) => {
		const first = editor.state.doc.firstChild!.firstChild!;
		return editor.state.doc.firstChild!.childCount === 1 && first.childCount === 2 && first.lastChild!.textContent === 'Two';
	};

	/** Type `text` at the caret one character at a time, input rules running. */
	function type(editor: Editor, text: string) {
		for (const ch of text) {
			const { from, to } = editor.state.selection;
			const handled = editor.view.someProp('handleTextInput', (f) => f(editor.view, from, to, ch, () => editor.state.tr));
			if (!handled) editor.view.dispatch(editor.state.tr.insertText(ch, from, to));
		}
	}

	// A phone's keyboard has no Tab.
	it('two spaces at the start of an item nest it; Indent and Outdent are in the insert menu', () => {
		const typed = bound(LIST);
		typed.commands.setTextSelection(posIn(typed, 'Two'));
		type(typed, '  ');
		expect(nested(typed)).toBe(true);
		expect(typed.state.doc.textContent).toBe('OneTwo');

		const first = bound(LIST);
		first.commands.setTextSelection(posIn(first, 'One'));
		type(first, '  ');
		expect(first.state.doc.textContent).toBe('  OneTwo');

		const menu = bound(LIST);
		menu.commands.setTextSelection(posIn(menu, 'Two', 3));
		const indent = filterCommands(treeCommands(host), 'indent')[0];
		expect(indent.label).toBe('Indent');
		indent.run(menu, { from: menu.state.selection.from, to: menu.state.selection.from });
		expect(nested(menu)).toBe(true);
		expect(outdentItem(menu)).toBe(true);
		expect(nested(menu)).toBe(false);
		expect(indentItem(menu)).toBe(true);
	});

	it('controls are a thumb’s 44pt: IconButton, or a hit area that grows on touch', () => {
		for (const file of ['../components/pages/TableMenu.svelte', '../components/pages/nodes/CalloutNode.svelte', '../components/pages/nodes/CodeBlockNode.svelte']) {
			const source = read(file);
			expect(source, file).toMatch(/<IconButton /);
			expect(source, file).not.toMatch(/<button/);
		}
		const css = read('document.css');
		expect(css).toMatch(/@media \(pointer: coarse\) \{\s+\.doc-prose ul\[data-type='taskList'\] > li > label \{/);
		expect(css).toMatch(/width: 44px;\s+height: 44px;/);
		for (const file of ['../components/pages/nodes/MediaNode.svelte', '../components/pages/nodes/AppletNode.svelte']) {
			expect(read(file), file).toMatch(/@media \(pointer: coarse\) \{[^}]*handle[^}]*::after|@media \(pointer: coarse\) \{[\s\S]*?handle::after/);
		}
		expect(read('../components/pages/SuggestionBar.svelte')).toMatch(/@media \(pointer: coarse\) \{\s+\.sb-btn \{\s+height: 44px;/);
	});

	// A 28px button's grown hit area reaches 8px into a neighbour that touches
	// it, and the later one takes the overlap: the edge of Row above ran Row below.
	it('the table bar’s buttons and the selection toolbar’s are 44pt themselves on touch, never halos that overlap', () => {
		const bar = read('../components/pages/TableMenu.svelte');
		const buttons = bar.match(/<IconButton [^>]*>/g) ?? [];
		expect(buttons.length).toBeGreaterThan(0);
		for (const b of buttons) expect(b).toMatch(/size=\{touch \? "touch" : "md"\}/);
		expect(bar).toMatch(/const touch = typeof window !== "undefined" && !!window\.matchMedia\?\.\("\(pointer: coarse\)"\)\.matches;/);
		expect(read('../components/SelectionToolbar.svelte')).toMatch(
			/@media \(pointer: coarse\) \{\s+\.toolbar-btn \{\s+width: 44px;\s+height: 44px;/,
		);
	});

	// The selection toolbar's Ask AI and `/ai` open the prompt on a phone,
	// and Edit in a link's menu opens the link panel there.
	it('the Ask AI prompt’s actions and the link panel’s buttons are 44pt on touch', () => {
		const coarse = read('../components/pages/AiPromptPopover.svelte').match(/@media \(pointer: coarse\) \{([\s\S]*?)\n\t\}/g) ?? [];
		expect(coarse.some((block) => block.includes('.ai-prompt-action {') && /min-height: 44px;/.test(block))).toBe(true);
		const panel = read('../components/pages/LinkEditorPopover.svelte');
		const buttons = panel.match(/<Button [^>]*>/g) ?? [];
		expect(buttons).toHaveLength(2);
		for (const b of buttons) expect(b).toMatch(/size=\{touch \? "lg" : "sm"\}/);
		expect(panel).toMatch(/const touch = typeof window !== "undefined" && !!window\.matchMedia\?\.\("\(pointer: coarse\)"\)\.matches;/);
		expect(read('../components/Button.svelte')).toMatch(/\.v-btn\[data-size="lg"\] \{\s+height: 48px;/);
	});

	// A long press opens these menus on a phone; `/` and `@` open the others.
	it('menu rows, the code block’s language and the find bar’s buttons are 44pt on touch', () => {
		for (const [file, selector] of [
			['../components/MenuItem.svelte', '.v-menuitem'],
			['../components/SlashMenu.svelte', '.command-item'],
			['../components/RefPicker.svelte', '.result-item'],
			['../components/pages/nodes/CodeBlockNode.svelte', 'select.doc-code-language'],
		]) {
			const coarse = read(file).match(/@media \(pointer: coarse\) \{([\s\S]*?)\n\t\}/g) ?? [];
			expect(coarse.some((block) => block.includes(`${selector} {`) && /min-height: 44px;/.test(block)), file).toBe(true);
		}
		const find = read('../components/pages/FindBar.svelte');
		const buttons = find.match(/<IconButton [^>]*>/g) ?? [];
		expect(buttons.length).toBeGreaterThan(0);
		for (const b of buttons) expect(b).toMatch(/size=\{touch \? "touch" : "md"\}/);
		expect(find).toMatch(/const touch = typeof window !== "undefined" && !!window\.matchMedia\?\.\("\(pointer: coarse\)"\)\.matches;/);
	});

	// Tiptap's box focused the editor before writing the tick: on a phone the
	// keyboard rose over a page someone was only reading.
	it('ticking a to-do writes the tick and leaves the keyboard where it was', async () => {
		const editor = bound('<ul data-type="taskList"><li data-type="taskItem" data-checked="false"><p>Book seats</p></li></ul><p>Other</p>');
		const elsewhere = document.createElement('input');
		document.body.append(elsewhere);
		elsewhere.focus();
		const focus = vi.spyOn(editor.view, 'focus');
		const domFocus = vi.spyOn(editor.view.dom, 'focus');
		const box = editor.view.dom.querySelector('input[type="checkbox"]') as HTMLInputElement;
		box.checked = true;
		box.dispatchEvent(new Event('change', { bubbles: true }));
		await settle(50);
		expect(editor.state.doc.child(0).child(0).attrs.checked).toBe(true);
		expect(focus).not.toHaveBeenCalled();
		expect(domFocus).not.toHaveBeenCalled();
		expect(document.activeElement).toBe(elsewhere);
	});

	// A key on a focused box went to the editor, which acted where the
	// caret was: Enter split a paragraph elsewhere, Tab nested another item,
	// Space replaced a selected image.
	it('keys pressed on a focused to-do box are the box’s, never edits where the caret is', () => {
		const todos =
			'<ul data-type="taskList"><li data-type="taskItem" data-checked="false"><p>Book seats</p></li><li data-type="taskItem" data-checked="false"><p>Pack</p></li></ul>';
		const editor = bound(`<p>Hello world</p>${todos}<img src="/api/media/harbour.png" alt="Harbour">`);
		const boxes = [...editor.view.dom.querySelectorAll('input[type="checkbox"]')] as HTMLInputElement[];
		const on = (target: Element, type: string, key: string) => {
			const event = new KeyboardEvent(type, { key, code: key === ' ' ? 'Space' : key, bubbles: true, cancelable: true });
			target.dispatchEvent(event);
			return event;
		};
		const before = editor.state.doc;

		editor.commands.setTextSelection(posIn(editor, 'Hello', 5));
		boxes[0].focus();
		on(boxes[0], 'keydown', 'Enter');
		expect(editor.state.doc.eq(before), 'Enter').toBe(true);

		editor.commands.setTextSelection(posIn(editor, 'Pack', 2));
		on(boxes[0], 'keydown', 'Tab');
		expect(editor.state.doc.eq(before), 'Tab').toBe(true);

		editor.view.dispatch(editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, editor.state.doc.content.size - 1)));
		on(boxes[0], 'keydown', ' ');
		on(boxes[0], 'keypress', ' ');
		expect(editor.state.doc.eq(before), 'Space').toBe(true);
	});

	it('⌘Enter ticks the to-do the caret is in, and again unticks it', () => {
		const editor = bound(
			'<ul data-type="taskList"><li data-type="taskItem" data-checked="false"><p>Book seats</p></li></ul><p>Other</p>',
		);
		const checked = () => editor.state.doc.child(0).child(0).attrs.checked;
		editor.commands.setTextSelection(posIn(editor, 'Book', 2));
		expect(press(editor, 'Enter', { mod: true }).defaultPrevented).toBe(true);
		expect(checked()).toBe(true);
		expect(editor.state.doc.child(0).child(0).textContent).toBe('Book seats');
		press(editor, 'Enter', { mod: true });
		expect(checked()).toBe(false);
	});

	// A tick from the keyboard after one from a pointer took the focus back
	// to the editor, off the box the person was on.
	it('a tick by Space leaves the focus on the box, even after a pointer ticked it', async () => {
		const editor = bound('<ul data-type="taskList"><li data-type="taskItem" data-checked="false"><p>Book seats</p></li></ul><p>Other</p>');
		const box = editor.view.dom.querySelector('input[type="checkbox"]') as HTMLInputElement;
		vi.spyOn(editor.view, 'hasFocus').mockReturnValue(true);
		const focus = vi.spyOn(editor.view, 'focus');
		box.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
		box.checked = true;
		box.dispatchEvent(new Event('change', { bubbles: true }));
		expect(focus).toHaveBeenCalledOnce();
		focus.mockClear();
		box.focus();
		box.dispatchEvent(new KeyboardEvent('keydown', { key: ' ', bubbles: true }));
		box.checked = false;
		box.dispatchEvent(new Event('change', { bubbles: true }));
		await settle();
		expect(editor.state.doc.child(0).child(0).attrs.checked).toBe(false);
		expect(focus).not.toHaveBeenCalled();
	});

	const touch = (type: string, x = 20, y = 20) =>
		new PointerEvent(type, { pointerType: 'touch', pointerId: 7, clientX: x, clientY: y, bubbles: true, cancelable: true });

	// A thumb settles on a handle before it drags: the hold would open the
	// widget's menu, Remove in it, mid-resize.
	it('a still press on a size handle resizes, and never opens the widget’s menu', async () => {
		const figure = document.createElement('figure');
		const handle = document.createElement('span');
		handle.setAttribute('role', 'slider');
		figure.append(handle);
		document.body.append(figure);
		const opened = vi.fn();
		onContextGesture(figure, opened);
		handle.dispatchEvent(touch('pointerdown'));
		await settle(500);
		expect(opened).not.toHaveBeenCalled();
		figure.dispatchEvent(touch('pointerdown'));
		await settle(500);
		expect(opened).toHaveBeenCalledOnce();
		window.dispatchEvent(touch('pointerup'));
		window.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
	});

	// WebKit sends a click when the finger lifts, Android a `contextmenu` at
	// its own hold delay; either lands on the menu's backdrop and closes it.
	it('the lift after a hold opens a menu, and the platform’s own menu, never close it', async () => {
		const editor = bound('<p>See <a href="https://example.com">docs</a></p>', [contextGesture([linkContext((): Editor => editor)])]);
		const link = editor.view.dom.querySelector('a')!;
		const backdrop = vi.fn();
		document.addEventListener('click', backdrop);
		document.addEventListener('contextmenu', backdrop);
		cleanups.push(() => {
			document.removeEventListener('click', backdrop);
			document.removeEventListener('contextmenu', backdrop);
		});
		vi.spyOn(editor.view, 'posAtCoords').mockReturnValue({ pos: posIn(editor, 'docs', 1), inside: -1 });
		link.dispatchEvent(touch('pointerdown'));
		await settle(500);
		expect(contextMenu.show).toHaveBeenCalledOnce();
		// Android's own long press, at a lengthened touch-and-hold delay.
		const native = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
		document.body.dispatchEvent(native);
		expect(native.defaultPrevented).toBe(true);
		window.dispatchEvent(touch('pointerup'));
		const lift = new MouseEvent('click', { bubbles: true, cancelable: true });
		document.body.dispatchEvent(lift);
		expect(backdrop).not.toHaveBeenCalled();
		// The next tap is a tap.
		document.body.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
		expect(backdrop).toHaveBeenCalledOnce();
		for (const file of ['../components/contextMenu/ContextMenuProvider.svelte', '../codemirror/extensions/long-press.ts']) {
			expect(read(file), file).toMatch(/swallowLift\(\);/);
		}
	});

	it('a wide table scrolls inside its own box, not the page, title and text with it', () => {
		const editor = bound(`<table><tbody><tr>${'<td><p>Cell</p></td>'.repeat(8)}</tr></tbody></table>`);
		const table = editor.view.dom.querySelector('table')!;
		const wrapper = table.parentElement!;
		expect(wrapper.classList.contains('tableWrapper')).toBe(true);
		const css = read('document.css');
		expect(css).toMatch(/\.doc-prose \.tableWrapper \{\s+overflow-x: auto;\s+\}/);
		// The bar sits over the box the table scrolls in, not the table's full width.
		expect(read('../components/pages/DocumentEditor.svelte')).toMatch(/const dom = e\.view\.nodeDOM\(at\.tablePos\) as HTMLElement \| null;/);
		expect(editor.view.nodeDOM(0)).toBe(wrapper);
	});

	// A mention held on one line widened the page past a phone's column,
	// and the page's title and text scrolled sideways with it.
	it('a mention longer than the column wraps inside it, never widening the page', () => {
		const rule = read('../components/pages/nodes/MentionNode.svelte').match(/\.doc-mention \{[^}]*\}/)?.[0] ?? '';
		expect(rule).not.toMatch(/white-space/);
		expect(rule).toMatch(/overflow-wrap: anywhere;/);
	});

	it('a to-do’s hit area grows away from its text, so a tap at the text’s start places the caret', () => {
		const css = read('document.css');
		const after = css.match(/ul\[data-type='taskList'\] > li > label::after \{([^}]*)\}/)![1];
		// It ends at the 14px box's right edge; the text starts 8px past it.
		expect(after).toMatch(/right: 0;/);
		expect(after).not.toMatch(/left:/);
		expect(after).toMatch(/transform: translateY\(-50%\);/);
		expect(css).toMatch(/ul\[data-type='taskList'\] > li \{\s+display: flex;\s+align-items: flex-start;\s+gap: 8px;/);
	});
});

describe('Ask AI in a code block', () => {
	it('is offered in one code block, never across its edge', () => {
		const editor = track(htmlEditor('<p>Run this</p><pre data-language="bash"><code>ls -la</code></pre>'));
		const inCode = posIn(editor, 'ls -la');
		editor.commands.setTextSelection({ from: inCode, to: inCode + 2 });
		expect(codeAt(editor.state)).toBe('code');
		editor.commands.setTextSelection({ from: posIn(editor, 'Run this', 4), to: inCode + 2 });
		expect(codeAt(editor.state)).toBe('across');
		editor.commands.setTextSelection({ from: 1, to: 4 });
		expect(codeAt(editor.state)).toBe('text');
		const source = read('../components/pages/DocumentEditor.svelte');
		expect(source).toMatch(/if \(!editor \|\| !ai \|\| codeAt\(editor\.state\) === "across"\) return;/);
		expect(source).toMatch(/marks=\{!toolbarInCode\}/);
	});
});

describe('copying out', () => {
	// The chat composer and a markdown page read a paste's text, not its HTML.
	it('puts markdown on the clipboard as text, and plain words as themselves', () => {
		const editor = bound('<h2>Plan</h2><ul><li><p>see <a href="https://example.com">docs</a> and <strong>this</strong></p></li></ul><p>Cost: 5 * 3</p>');
		editor.commands.selectAll();
		const all = editor.view.serializeForClipboard(editor.state.selection.content());
		expect(all.text).toBe('## Plan\n\n- see [docs](https://example.com) and **this**\n\nCost: 5 \\* 3');
		const from = posIn(editor, 'Cost');
		editor.commands.setTextSelection({ from, to: from + 'Cost: 5 * 3'.length });
		expect(editor.view.serializeForClipboard(editor.state.selection.content()).text).toBe('Cost: 5 * 3');
		const link = posIn(editor, 'see');
		editor.commands.setTextSelection({ from: link, to: link + 'see docs'.length });
		expect(editor.view.serializeForClipboard(editor.state.selection.content()).text).toBe('see [docs](https://example.com)');
	});

	const copied = (editor: Editor) => editor.view.serializeForClipboard(editor.state.selection.content()).text;

	// A copy wholly inside one list item is that item's blocks; a list item
	// alone is nothing markdown can write.
	it('a copy inside one list item is its blocks as markdown, in any kind of list', () => {
		const nested = bound('<ul><li><p>alpha one</p><ul><li><p>beta two</p></li></ul></li></ul>');
		nested.commands.setTextSelection({ from: posIn(nested, 'alpha one', 2), to: posIn(nested, 'beta two', 4) });
		expect(copied(nested)).toBe('pha one\n\n- beta');

		for (const [list, item] of [
			['ul', '<li><p>first para</p><p>second para</p></li>'],
			['ol', '<li><p>first para</p><p>second para</p></li>'],
			['ul data-type="taskList"', '<li data-type="taskItem" data-checked="false"><p>first para</p><p>second para</p></li>'],
		]) {
			const editor = bound(`<${list}>${item}</${list.split(' ')[0]}><p>After</p>`);
			editor.commands.setTextSelection({ from: posIn(editor, 'first para', 6), to: posIn(editor, 'second para', 6) });
			expect(copied(editor), list).toBe('para\n\nsecond');
		}
	});

	// As a spreadsheet copies a range, and as the CodeMirror editor's table did.
	it('selected cells are their words, a tab between cells and a line between rows', () => {
		const editor = bound(
			'<table><tbody><tr><th><p>Day</p></th><th><p>Cost</p></th></tr><tr><td><p>Fri</p></td><td><p>40</p></td></tr><tr><td><p>Sat</p></td><td><p>55 <strong>each</strong></p></td></tr></tbody></table>',
		);
		const cell = (text: string) => editor.state.doc.resolve(posIn(editor, text) - 2);
		const select = (a: string, b: string) =>
			editor.view.dispatch(editor.state.tr.setSelection(new CellSelection(cell(a), cell(b))));
		select('Fri', '40');
		expect(copied(editor)).toBe('Fri\t40');
		select('Fri', '55 each');
		expect(copied(editor)).toBe('Fri\t40\nSat\t55 each');
		select('Cost', '55 each');
		expect(copied(editor)).toBe('Cost\n40\n55 each');
		select('Sat', 'Sat');
		expect(copied(editor)).toBe('Sat');
		select('Day', '55 each');
		expect(copied(editor)).toContain('| Day | Cost |');
	});
});

describe('printing', () => {
	it('the browser’s own Print prints the page alone, as the toolbar’s does', () => {
		document.body.innerHTML = '<nav id="side">Sidebar</nav><main><div id="tabs">Tabs</div><div id="scroller"><p>Page</p></div></main>';
		const scroller = document.getElementById('scroller')!;
		cleanups.push(printAlone(() => scroller));
		window.dispatchEvent(new Event('beforeprint'));
		expect(scroller.hasAttribute('data-print-root')).toBe(true);
		expect(document.querySelector('main')!.hasAttribute('data-print-chain')).toBe(true);
		expect(document.getElementById('tabs')!.hasAttribute('data-print-off')).toBe(true);
		expect(document.getElementById('side')!.hasAttribute('data-print-off')).toBe(true);
		window.dispatchEvent(new Event('afterprint'));
		expect(document.querySelectorAll('[data-print-root], [data-print-chain], [data-print-off]')).toHaveLength(0);
		markForPrint(scroller)();
		const page = read('../components/views/PageContent.svelte');
		expect(page).toMatch(/return printAlone\(\(\) => printRootEl \?\? null\);/);
		expect(page).toMatch(/if \(printRootEl\) printThis\(printRootEl\);/);
	});

	it('only the page in view prints: never one in a hidden tab, and of two side by side the one in use', () => {
		// Tabs as the shell keeps them: every one mounted, the hidden ones `display: none`.
		document.body.innerHTML =
			'<main><div id="slot-a" style="display: none"><div id="a">Page A</div></div>' +
			'<div id="slot-chat"><div id="chat">A chat</div></div></main>';
		const el = (id: string) => document.getElementById(id)!;
		const marked = () => [...document.querySelectorAll('[data-print-root], [data-print-chain], [data-print-off]')].map((e) => e.id);
		const release = printAlone(() => el('a'));
		window.dispatchEvent(new Event('beforeprint'));
		// The chat in view prints as it is; the page in the hidden tab does not.
		expect(marked()).toEqual([]);
		window.dispatchEvent(new Event('afterprint'));

		el('slot-chat').innerHTML = '<div id="b">Page B</div>';
		cleanups.push(release, printAlone(() => el('b')));
		window.dispatchEvent(new Event('beforeprint'));
		expect(el('b').hasAttribute('data-print-root')).toBe(true);
		expect(el('slot-a').hasAttribute('data-print-chain')).toBe(false);
		expect(el('a').hasAttribute('data-print-root')).toBe(false);
		window.dispatchEvent(new Event('afterprint'));

		// Two pages side by side: the one the focus is in, or whose own Print ran.
		el('slot-a').style.display = '';
		el('a').innerHTML = '<input id="field">';
		el('field').focus();
		window.dispatchEvent(new Event('beforeprint'));
		expect(document.querySelectorAll('[data-print-root]')).toHaveLength(1);
		expect(el('a').hasAttribute('data-print-root')).toBe(true);
		window.dispatchEvent(new Event('afterprint'));
		// The test window has no print: one that prints as a browser's does.
		const print = vi.fn(() => {
			window.dispatchEvent(new Event('beforeprint'));
			expect(document.querySelectorAll('[data-print-root]')).toHaveLength(1);
			expect(el('b').hasAttribute('data-print-root')).toBe(true);
			window.dispatchEvent(new Event('afterprint'));
		});
		const had = window.print;
		window.print = print;
		printThis(el('b'));
		window.print = had;
		expect(print).toHaveBeenCalledOnce();
		expect(marked()).toEqual([]);
	});

	// In the Mac app `window.print` is Tauri's command, which answers with a
	// promise, needs its permission, and is not known to fire `beforeprint`.
	it('the page’s own Print marks the page before printing, and says so when the print fails', async () => {
		document.body.innerHTML = '<nav id="side">Sidebar</nav><main><div id="scroller"><p>Page</p></div></main>';
		const scroller = document.getElementById('scroller')!;
		const target = new EventTarget() as EventTarget & { print: () => unknown };
		const marked: boolean[] = [];
		target.print = vi.fn(() => {
			marked.push(scroller.hasAttribute('data-print-root') && document.getElementById('side')!.hasAttribute('data-print-off'));
			return Promise.resolve();
		});
		const release = printAlone(() => scroller, target as unknown as Window);
		printThis(scroller, target as unknown as Window);
		expect(marked).toEqual([true]);
		await settle();
		// Handed over, the marks stay for the print until the window is used again.
		expect(scroller.hasAttribute('data-print-root')).toBe(true);
		target.dispatchEvent(new Event('pointerdown'));
		expect(document.querySelectorAll('[data-print-root], [data-print-chain], [data-print-off]')).toHaveLength(0);

		vi.mocked(toast.error).mockClear();
		target.print = vi.fn(() => Promise.reject(new Error('Command plugin:webview|print not allowed by ACL')));
		printThis(scroller, target as unknown as Window);
		await settle();
		expect(toast.error).toHaveBeenCalledOnce();
		expect(document.querySelectorAll('[data-print-root], [data-print-chain], [data-print-off]')).toHaveLength(0);
		release();

		for (const file of ['../../../src-tauri/capabilities/mac.json', '../../../src-tauri/capabilities/default.json']) {
			expect(JSON.parse(read(file)).permissions, file).toContain('core:webview:allow-print');
		}
		// The iOS app's shell has no print command: Print is not offered there.
		expect(read('../components/views/PageContent.svelte')).toMatch(/onPrint=\{format === "tree" && canPrint \? printPage : undefined\}/);
	});

	/** The print block of document.css. */
	const printCss = () => {
		const css = read('document.css');
		const start = css.indexOf('@media print {');
		return css.slice(start);
	};

	it('prints in paper’s colours whatever the screen’s theme, code tokens included', () => {
		const css = printCss();
		expect(css).toMatch(/html,\s+html\[data-theme\] \{\s+--foreground: black;\s+--foreground-muted:/);
		expect(css).toMatch(/--background: white;/);
		expect(css).toMatch(/\.doc-block-codeBlock code span\[style\] \{\s+color: inherit !important;/);
	});

	// A highlight keeps its colour on paper, and its colours were the dark
	// theme's: near-white words on a pale tint over white paper, or a solid
	// dark block. A link and a proposal's halves printed pale too.
	it('prints a highlight as a light marker under black ink, and links and proposals in dark ink, whatever the theme', () => {
		const paper = printCss().match(/html,\s+html\[data-theme\] \{([^}]*)\}/)?.[1] ?? '';
		const token = (name: string) => paper.match(new RegExp(`--${name}: ([^;]+);`))?.[1];
		expect(token('highlight-foreground')).toBe('black');
		expect(token('highlight')).toMatch(/^color-mix\(in srgb, yellow \d+%, white\)$/);
		for (const name of ['primary', 'success', 'error']) expect(token(name), name).toMatch(/^color-mix\(in srgb, \w+ \d+%, black\)$/);
		for (const name of ['success-subtle', 'error-subtle']) expect(token(name), name).toMatch(/, white\)$/);
		// What reads them: the mark, the link, the proposals.
		const css = read('document.css');
		expect(css).toMatch(/\.doc-prose mark \{[^}]*background: var\(--color-highlight\);\s+color: var\(--color-highlight-foreground, inherit\);/);
		expect(css).toMatch(/\.doc-prose a \{\s+color: var\(--color-primary\);/);
		expect(css).toMatch(/border-bottom: 1px solid var\(--color-success\);/);
		expect(css).toMatch(/text-decoration-color: var\(--color-error\);/);
	});

	// FloatingContent is not portalled: the toolbar open over a selection,
	// or the `/` menu, sits inside the page and printed over it on ⌘P.
	it('no menu or bar floating over the page prints with it', () => {
		const block = printCss().match(/\{\s*display: none !important;\s*\}/)!;
		const selectors = printCss().slice(0, block.index);
		expect(selectors).toContain('[data-print-root] .floating-content');
		for (const file of [
			'../components/SelectionToolbar.svelte',
			'../components/SlashMenu.svelte',
			'../components/RefPicker.svelte',
			'../components/pages/AiPromptPopover.svelte',
		]) {
			expect(read(file), file).toMatch(/<FloatingContent\b/);
		}
		expect(read('../floating/core/FloatingContent.svelte')).toMatch(/class="floating-content \{className\}"/);
	});

	// The print dialog drops backgrounds by default: a done to-do printed as
	// an empty box, its tick the paper's colour, and highlights vanished.
	it('a done to-do prints ticked in ink, and a highlight keeps its colour', () => {
		const css = printCss();
		expect(css).toMatch(/input\[type='checkbox'\]:checked \{\s+background: none;\s+border-color: var\(--color-foreground\);/);
		expect(css).toMatch(/input\[type='checkbox'\]:checked::after \{\s+border-color: var\(--color-foreground\);/);
		expect(css).toMatch(/\.doc-prose mark \{\s+-webkit-print-color-adjust: exact;\s+print-color-adjust: exact;/);
	});

	it('an applet that never mounted prints as a box naming it, not the height its frame reserves', () => {
		expect(printCss()).toMatch(/\.doc-applet-placeholder \{\s+height: auto !important;/);
	});

	it('wraps a long line of code onto the sheet rather than cut it off', () => {
		const css = printCss();
		expect(css).toMatch(/\.doc-block-codeBlock \{\s+overflow: visible;\s+\}/);
		expect(css).toMatch(/\.doc-block-codeBlock pre,\s+\.doc-block-codeBlock code \{\s+overflow: visible;\s+white-space: pre-wrap;\s+overflow-wrap: anywhere;/);
	});
});

describe('the build doc', () => {
	// The parity table outlives the plan that asked for it, and once lived
	// only in a spike file that never shipped.
	it('names what a block page keeps, replaces and cuts of the CodeMirror editor', () => {
		const doc = read('../../../../../agents/build/document.md');
		const start = doc.indexOf('## Parity with the CodeMirror editor');
		expect(start, 'the parity section').toBeGreaterThan(0);
		const section = doc.slice(start, doc.indexOf('\n## ', start + 1));
		for (const named of [
			'View as markdown',
			'CriticMarkup comments',
			'Live preview, render mode, widget height, mouse freeze, list renumber, empty-mark tidy',
			'Find with replace, replace all, match case, regexp, whole word',
			'drag rows and columns',
			'Copy: a selection is its markdown',
			'Ask AI, a code block included',
			'The drawn caret (`caret.ts`)',
			'Enter in a table moves to the cell below',
			"Backspace at a heading's start makes it text",
			'Typed `![name](url)` draws what its address is',
		]) {
			expect(section, named).toContain(named);
		}
		expect(section).toContain('**Replaced**');
		expect(section).toContain('**Cut**');
		expect(doc).not.toContain('the parity table in the plan');
	});
});

describe('keys a markdown page has', () => {
	/** The position before the first cell holding `text`. */
	function cellPos(editor: Editor, text: string): number {
		let found = -1;
		editor.state.doc.descendants((node, pos) => {
			if (found >= 0) return false;
			if ((node.type.name === 'tableCell' || node.type.name === 'tableHeader') && node.textContent === text) {
				found = pos;
				return false;
			}
			return true;
		});
		return found;
	}

	const TABLE =
		'<table><tbody><tr><th><p>Day</p></th><th><p>Cost</p></th></tr><tr><td><p>Fri</p></td><td><p>40</p></td></tr><tr><td><p>Sat</p></td><td><p>55</p></td></tr><tr><td><p>Sun</p></td><td><p>20</p></td></tr></tbody></table>';

	it('Backspace at the start of a heading makes it a paragraph, the same block; again joins it to the one above', () => {
		const editor = bound('<p>Bar</p><h2>Foo</h2>');
		const id = editor.state.doc.child(1).attrs[contract.id.attr];
		editor.commands.setTextSelection(posIn(editor, 'Foo'));
		expect(press(editor, 'Backspace').defaultPrevented).toBe(true);
		const block = editor.state.doc.child(1);
		expect([block.type.name, block.textContent, block.attrs[contract.id.attr]]).toEqual(['paragraph', 'Foo', id]);
		press(editor, 'Backspace');
		expect(editor.state.doc.childCount).toBe(1);
		expect(editor.state.doc.child(0).textContent).toBe('BarFoo');

		const first = bound('<h1>Title</h1><p>Text</p>');
		first.commands.setTextSelection(1);
		press(first, 'Backspace');
		expect(first.state.doc.child(0).type.name).toBe('paragraph');
		expect(first.state.doc.child(0).textContent).toBe('Title');
	});

	it('⌘` marks inline code and ⌘⇧X strikes through, as on a markdown page', () => {
		const editor = bound('<p>Run the build</p>');
		const from = posIn(editor, 'the build');
		editor.commands.setTextSelection({ from, to: from + 'the build'.length });
		expect(press(editor, '`', { mod: true }).defaultPrevented).toBe(true);
		expect(editor.isActive('code')).toBe(true);
		expect(press(editor, 'x', { mod: true, shift: true }).defaultPrevented).toBe(true);
		expect(editor.isActive('strike')).toBe(true);
	});

	it('Enter in a cell moves to the cell below; in the last row it stays; in a list in a cell it adds an item', () => {
		const editor = bound(TABLE);
		editor.commands.setTextSelection(posIn(editor, 'Fri', 3));
		const before = editor.state.doc;
		expect(press(editor, 'Enter').defaultPrevented).toBe(true);
		expect(editor.state.doc.eq(before)).toBe(true);
		expect(editor.state.selection.$head.parent.textContent).toBe('Sat');
		expect(cellAt(editor.state)).toMatchObject({ row: 2, col: 0 });
		press(editor, 'Enter');
		expect(cellAt(editor.state)).toMatchObject({ row: 3, col: 0 });
		press(editor, 'Enter');
		expect(cellAt(editor.state)).toMatchObject({ row: 3, col: 0 });
		expect(editor.state.doc.eq(before)).toBe(true);

		const list = bound('<table><tbody><tr><td><ul><li><p>one</p></li></ul></td></tr></tbody></table>');
		list.commands.setTextSelection(posIn(list, 'one', 3));
		press(list, 'Enter');
		let items = 0;
		list.state.doc.descendants((n) => {
			if (n.type.name === 'listItem') items++;
			return true;
		});
		expect(items).toBe(2);
	});

	/** Each top-level block: its type, and its text or its address. */
	function blocks(editor: Editor): string[] {
		const out: string[] = [];
		editor.state.doc.forEach((n) => out.push(n.isAtom ? `${n.type.name}:${n.attrs.src ?? n.attrs.ref ?? ''}` : `${n.type.name}:${n.textContent}`));
		return out;
	}

	// ProseMirror's join deletes a widget beside the caret outright; nothing
	// showed it first.
	for (const [kind, html] of [
		['an image', '<img src="/api/media/harbour.png" alt="Harbour">'],
		['an applet', '<virtues-applet ref="sleep-week" height="200"></virtues-applet>'],
		['a file', '<virtues-file src="/api/media/plan.pdf" data-name="plan.pdf"></virtues-file>'],
		['a rule', '<hr>'],
	]) {
		it(`Backspace or Delete beside ${kind} selects it, and the next key deletes it`, () => {
			const editor = bound(`<p>Above</p>${html}<p>After</p>`);
			const before = blocks(editor);
			editor.commands.setTextSelection(posIn(editor, 'After'));
			expect(press(editor, 'Backspace').defaultPrevented).toBe(true);
			expect(blocks(editor)).toEqual(before);
			expect(editor.state.selection).toBeInstanceOf(NodeSelection);
			expect((editor.state.selection as NodeSelection).node.isAtom).toBe(true);
			press(editor, 'Backspace');
			expect(blocks(editor)).toEqual(['paragraph:Above', 'paragraph:After']);

			const forward = bound(`<p>Above</p>${html}<p>After</p>`);
			forward.commands.setTextSelection(posIn(forward, 'Above', 5));
			expect(press(forward, 'Delete').defaultPrevented).toBe(true);
			expect(blocks(forward)).toEqual(before);
			expect(forward.state.selection).toBeInstanceOf(NodeSelection);
		});
	}

	it('Backspace or Delete at a code block’s edge never joins it to the text beside it', () => {
		const html = '<p>Intro</p><pre data-language="python"><code>def f():\n    return 1</code></pre><p>Outro</p>';
		const editor = bound(html);
		const before = editor.state.doc;
		editor.commands.setTextSelection(posIn(editor, 'def f()'));
		press(editor, 'Backspace');
		expect(editor.state.doc.eq(before)).toBe(true);
		expect(editor.state.selection.$from.parent.textContent).toBe('Intro');
		press(editor, 'Delete');
		expect(editor.state.doc.eq(before)).toBe(true);
		expect(editor.state.selection.$from.parent.type.name).toBe('codeBlock');
		editor.commands.setTextSelection(posIn(editor, 'Outro'));
		press(editor, 'Backspace');
		expect(editor.state.doc.eq(before)).toBe(true);
		expect(editor.state.selection.$from.parent.type.name).toBe('codeBlock');
		const code = editor.state.doc.child(1);
		expect([code.type.name, code.attrs.language, code.textContent]).toEqual(['codeBlock', 'python', 'def f():\n    return 1']);
	});

	// Tiptap joins a line to the block beside it on more keys than Backspace
	// and Delete: deleting words back to a line's start (⌥⌫ on a Mac,
	// Ctrl-Backspace elsewhere) reached the edge and took the image above, or
	// turned the line into code and lost its bold.
	for (const [platform, backward, forward] of [
		[
			'MacIntel',
			[{ mod: true }, { shift: true }, { alt: true }, { key: 'h', ctrl: true }],
			[{ mod: true }, { key: 'd', ctrl: true }, { key: 'Backspace', ctrl: true, alt: true }, { alt: true }, { key: 'd', alt: true }],
		],
		['Win32', [{ mod: true }, { shift: true }], [{ mod: true }]],
	] as [string, KeyAt[], KeyAt[]][]) {
		it(`on ${platform}, every key that joins lines selects a widget first and never joins a code block`, () => {
			Object.defineProperty(navigator, 'platform', { value: platform, configurable: true });
			try {
				const cases: [KeyAt, 'Backspace' | 'Delete'][] = [
					...backward.map((k): [KeyAt, 'Backspace'] => [k, 'Backspace']),
					...forward.map((k): [KeyAt, 'Delete'] => [k, 'Delete']),
				];
				for (const [k, plain] of cases) {
					const name = `${plain} ${JSON.stringify(k)}`;
					const image = bound('<p>Above</p><img src="/api/media/harbour.png" alt="Harbour"><p>After</p>');
					const before = blocks(image);
					image.commands.setTextSelection(plain === 'Backspace' ? posIn(image, 'After') : posIn(image, 'Above', 5));
					keyAt(image, k, plain);
					expect(blocks(image), name).toEqual(before);
					expect(image.state.selection, name).toBeInstanceOf(NodeSelection);

					const code = bound(
						plain === 'Backspace'
							? '<pre data-language="rust"><code>fn main() {}</code></pre><p>After <strong>bold</strong></p>'
							: '<p>Before <strong>b</strong></p><pre data-language="rust"><code>fn main() {}</code></pre>',
					);
					const doc = code.state.doc;
					code.commands.setTextSelection(plain === 'Backspace' ? posIn(code, 'After') : posIn(code, 'Before b', 'Before b'.length));
					keyAt(code, k, plain);
					expect(code.state.doc.eq(doc), name).toBe(true);
					expect(code.state.selection.$from.parent.type.name, name).toBe('codeBlock');
				}
			} finally {
				delete (navigator as { platform?: string }).platform;
			}
		});
	}

	// An empty line between two code blocks or two widgets is a line the
	// keys remove, never one they step around, and never a reason to take
	// the widget instead.
	it('Backspace or Delete in an empty line beside a code block or a widget removes the line, not the block', () => {
		const code = bound('<pre><code>one</code></pre><p></p><pre><code>two</code></pre>');
		code.commands.setTextSelection(posIn(code, 'one', 3) + 2);
		expect(code.state.selection.$from.parent.type.name).toBe('paragraph');
		expect(press(code, 'Backspace').defaultPrevented).toBe(true);
		expect(blocks(code)).toEqual(['codeBlock:one', 'codeBlock:two']);
		expect(code.state.selection.$from.parent.textContent).toBe('one');
		expect(code.state.selection.$from.parentOffset).toBe(3);

		const ahead = bound('<pre><code>one</code></pre><p></p><pre><code>two</code></pre>');
		ahead.commands.setTextSelection(posIn(ahead, 'one', 3) + 2);
		press(ahead, 'Delete');
		expect(blocks(ahead)).toEqual(['codeBlock:one', 'codeBlock:two']);
		expect(ahead.state.selection.$from.parent.textContent).toBe('two');
		expect(ahead.state.selection.$from.parentOffset).toBe(0);

		const img = '<img src="/api/media/harbour.png" alt="Harbour">';
		for (const key of ['Backspace', 'Delete']) {
			const images = bound(`${img}<p></p>${img}`);
			images.commands.setTextSelection(images.state.doc.child(0).nodeSize + 1);
			press(images, key);
			expect(blocks(images), key).toEqual(['image:/api/media/harbour.png', 'image:/api/media/harbour.png']);
			expect(images.state.selection, key).toBeInstanceOf(NodeSelection);
			expect(images.state.selection.from, key).toBe(key === 'Backspace' ? 0 : images.state.doc.child(0).nodeSize);
		}

		const page = bound(`<p>Intro</p>${img}<p></p><p>After</p>`);
		page.commands.setTextSelection(page.state.doc.child(0).nodeSize + page.state.doc.child(1).nodeSize + 1);
		press(page, 'Backspace');
		expect(blocks(page)).toEqual(['paragraph:Intro', 'image:/api/media/harbour.png', 'paragraph:After']);
		expect(page.state.selection).toBeInstanceOf(NodeSelection);
	});

	it('Enter in a code block keeps the line’s indent; the third Enter at its end still leaves it', () => {
		const editor = bound('<pre data-language="rust"><code>fn main() {\n    let x = 1;</code></pre><p>After</p>');
		const code = () => editor.state.doc.child(0).textContent;
		editor.commands.setTextSelection(posIn(editor, 'let x = 1;', 'let x = 1;'.length));
		press(editor, 'Enter');
		expect(code()).toBe('fn main() {\n    let x = 1;\n    ');
		editor.commands.insertContent('y');
		expect(code()).toBe('fn main() {\n    let x = 1;\n    y');
		press(editor, 'Enter');
		press(editor, 'Enter');
		expect(code()).toBe('fn main() {\n    let x = 1;\n    y\n\n    ');
		press(editor, 'Enter');
		expect(code()).toBe('fn main() {\n    let x = 1;\n    y');
		expect(editor.state.selection.$from.parent.type.name).toBe('paragraph');
	});

	it('⌘] nests the caret’s list item and ⌘[ takes it out, and neither leaves the page for the browser', () => {
		const list = bound('<ul><li><p>one</p></li><li><p>two</p></li></ul><p>After</p>');
		const nested = () => list.state.doc.child(0).child(0).childCount;
		list.commands.setTextSelection(posIn(list, 'two', 1));
		expect(press(list, ']', { mod: true }).defaultPrevented).toBe(true);
		expect(list.state.doc.child(0).childCount).toBe(1);
		expect(nested()).toBe(2);
		expect(press(list, '[', { mod: true }).defaultPrevented).toBe(true);
		expect(list.state.doc.child(0).childCount).toBe(2);

		const tasks = bound('<ul data-type="taskList"><li data-type="taskItem" data-checked="false"><p>Book</p></li><li data-type="taskItem" data-checked="false"><p>Pack</p></li></ul>');
		tasks.commands.setTextSelection(posIn(tasks, 'Pack', 1));
		press(tasks, ']', { mod: true });
		expect(tasks.state.doc.child(0).childCount).toBe(1);

		// Where there is no item, ⌘[ is still the page's, never the browser's Back.
		list.commands.setTextSelection(posIn(list, 'After', 1));
		const before = list.state.doc;
		expect(press(list, '[', { mod: true }).defaultPrevented).toBe(true);
		expect(list.state.doc.eq(before)).toBe(true);
	});

	// The CodeMirror editor indents and outdents a code line on these keys;
	// the block page took them and did nothing, or nested the whole item.
	it('⌘] and ⌘[ in a code block indent and outdent every line the selection touches, in a list item too', () => {
		const editor = bound('<pre data-language="python"><code>if x:\nreturn 1\nreturn 2</code></pre><p>After</p>');
		const code = () => editor.state.doc.child(0).textContent;
		editor.commands.setTextSelection(posIn(editor, 'return 1'));
		expect(press(editor, ']', { mod: true }).defaultPrevented).toBe(true);
		expect(code()).toBe('if x:\n  return 1\nreturn 2');
		expect(editor.state.selection.$from.parentOffset).toBe('if x:\n  '.length);
		// A selection over two lines indents both; one ending at a line's start leaves that line.
		editor.commands.setTextSelection({ from: posIn(editor, 'return 1', 2), to: posIn(editor, 'return 2', 3) });
		press(editor, ']', { mod: true });
		expect(code()).toBe('if x:\n    return 1\n  return 2');
		editor.commands.setTextSelection({ from: posIn(editor, 'if x'), to: posIn(editor, 'if x:\n') + 'if x:\n'.length });
		press(editor, ']', { mod: true });
		expect(code()).toBe('  if x:\n    return 1\n  return 2');
		editor.commands.setTextSelection({ from: posIn(editor, 'if x'), to: posIn(editor, 'return 2', 1) });
		press(editor, '[', { mod: true });
		expect(code()).toBe('if x:\n  return 1\nreturn 2');
		press(editor, '[', { mod: true });
		press(editor, '[', { mod: true });
		expect(code()).toBe('if x:\nreturn 1\nreturn 2');
		expect(editor.state.doc.child(0).attrs.language).toBe('python');

		const inList = bound('<ul><li><p>one</p></li><li><p>two</p><pre><code>a\nb</code></pre></li></ul>');
		inList.commands.setTextSelection(posIn(inList, 'a\nb', 2));
		press(inList, ']', { mod: true });
		expect(inList.state.doc.child(0).childCount).toBe(2);
		expect(inList.state.doc.child(0).child(1).child(1).textContent).toBe('a\n  b');
	});

	// The CodeMirror editor edits a fence's info string to any language; the
	// picker offered its sixteen and nothing else for a block already made.
	it('a code block takes a language the picker does not offer, typed after Other…', () => {
		expect(typedLanguage('  kotlin script ')).toBe('kotlin');
		expect(typedLanguage(' c++ ')).toBe('c++');
		expect(typedLanguage('   ')).toBe('');
		expect(typedLanguage(OTHER_LANGUAGE)).not.toBe(OTHER_LANGUAGE);
		expect(LANGUAGE_CHOICES.map(([value]) => value)).not.toContain(OTHER_LANGUAGE);
		const source = read('../components/pages/nodes/CodeBlockNode.svelte');
		expect(source).toMatch(/<option value=\{OTHER_LANGUAGE\}>Other…<\/option>/);
		expect(source).toMatch(/if \(select\.value === OTHER_LANGUAGE\) \{/);
		expect(source).toMatch(/const lang = typedLanguage\(typed\);\s+if \(lang !== language\) view\.setAttrs\(\{ language: lang \|\| null \}\);/);
		expect(source).toMatch(/<input\s+class="doc-code-language"[\s\S]*?onkeydown=\{onTypedKey\}\s+onblur=\{commit\}\s+aria-label="Language"/);
	});

	// Enter or Escape in the language field took the field away with the
	// focus in it, and the focus fell to the page's body: a Tab from the top
	// of the app away from the code.
	it('closing the language field from the keyboard gives the keyboard back to the picker', () => {
		const source = read('../components/pages/nodes/CodeBlockNode.svelte');
		expect(source).toMatch(/<select bind:this=\{picker\} class="doc-code-language"/);
		expect(source).toMatch(/async function backToPicker\(\) \{\s+await tick\(\);\s+picker\?\.focus\(\);\s+\}/);
		expect(source).toMatch(/if \(e\.key === "Enter"\) \{\s+e\.preventDefault\(\);\s+commit\(\);\s+void backToPicker\(\);/);
		expect(source).toMatch(/\} else if \(e\.key === "Escape"\) \{\s+e\.preventDefault\(\);\s+typing = false;\s+void backToPicker\(\);/);
		// Leaving the field writes it, the focus staying where it went.
		expect(source).toMatch(/onblur=\{commit\}/);
		expect(source.match(/backToPicker\(\)/g)).toHaveLength(3);
	});

	// The CodeMirror editor moves, copies and deletes every line a selection
	// touches; the block page did nothing once a selection reached a second block.
	it('⌥↓, ⇧⌥↓ and ⌘⇧K act on every item or block a selection touches', () => {
		const list = bound('<ul><li><p>a</p></li><li><p>b</p></li><li><p>c</p></li></ul><p>After</p>');
		const items = () => list.state.doc.child(0).content.content.map((n) => n.textContent);
		list.commands.setTextSelection({ from: posIn(list, 'a'), to: posIn(list, 'b', 1) });
		expect(press(list, 'ArrowDown', { alt: true }).defaultPrevented).toBe(true);
		expect(items()).toEqual(['c', 'a', 'b']);
		expect(list.state.doc.textBetween(list.state.selection.from, list.state.selection.to, '|')).toBe('a|b');
		press(list, 'ArrowDown', { alt: true, shift: true });
		expect(items()).toEqual(['c', 'a', 'b', 'a', 'b']);
		press(list, 'k', { mod: true, shift: true });
		expect(items()).toEqual(['c', 'a', 'b']);

		const page = bound('<p>one</p><p>two</p><p>three</p>');
		page.commands.setTextSelection({ from: posIn(page, 'one'), to: posIn(page, 'two', 2) });
		press(page, 'ArrowDown', { alt: true });
		expect(blocks(page)).toEqual(['paragraph:three', 'paragraph:one', 'paragraph:two']);
		press(page, 'ArrowUp', { alt: true, shift: true });
		expect(blocks(page)).toEqual(['paragraph:three', 'paragraph:one', 'paragraph:two', 'paragraph:one', 'paragraph:two']);
		press(page, 'k', { mod: true, shift: true });
		expect(blocks(page)).toEqual(['paragraph:three', 'paragraph:one', 'paragraph:two']);
		// A selection ending at the start of a block leaves that block where it is.
		page.commands.setTextSelection({ from: posIn(page, 'three'), to: posIn(page, 'one') });
		press(page, 'ArrowDown', { alt: true });
		expect(blocks(page)).toEqual(['paragraph:one', 'paragraph:three', 'paragraph:two']);

		// Every item of a list takes the list with it.
		const whole = bound('<ul><li><p>a</p></li><li><p>b</p></li></ul><p>After</p>');
		whole.commands.setTextSelection({ from: posIn(whole, 'a'), to: posIn(whole, 'b', 1) });
		press(whole, 'k', { mod: true, shift: true });
		expect(blocks(whole)).toEqual(['paragraph:After']);
	});

	it('⌥↑ and ⌥↓ move the caret’s item or block, ⇧⌥↓ copies it, ⌘⇧K deletes it; in a table ⌥↓ moves the row', () => {
		const list = bound('<ul><li><p>one</p></li><li><p>two</p></li><li><p>three</p></li></ul><p>After</p>');
		const items = () => list.state.doc.child(0).content.content.map((n) => n.textContent);
		const id = list.state.doc.child(0).child(1).attrs[contract.id.attr];
		list.commands.setTextSelection(posIn(list, 'two', 1));
		expect(press(list, 'ArrowUp', { alt: true }).defaultPrevented).toBe(true);
		expect(items()).toEqual(['two', 'one', 'three']);
		expect(list.state.doc.child(0).child(0).attrs[contract.id.attr]).toBe(id);
		expect(list.state.selection.$from.parent.textContent).toBe('two');
		press(list, 'ArrowDown', { alt: true });
		press(list, 'ArrowDown', { alt: true });
		expect(items()).toEqual(['one', 'three', 'two']);
		press(list, 'ArrowDown', { alt: true, shift: true });
		expect(items()).toEqual(['one', 'three', 'two', 'two']);
		const copy = list.state.doc.child(0).child(3).attrs[contract.id.attr];
		expect(copy).not.toBe(id);
		press(list, 'k', { mod: true, shift: true });
		expect(items()).toEqual(['one', 'three', 'two']);

		const page = bound('<h2>Plan</h2><p>First</p><p>Second</p>');
		page.commands.setTextSelection(posIn(page, 'Second'));
		press(page, 'ArrowUp', { alt: true });
		expect(blocks(page)).toEqual(['heading:Plan', 'paragraph:Second', 'paragraph:First']);

		const table = bound(TABLE);
		table.commands.setTextSelection(posIn(table, 'Fri'));
		press(table, 'ArrowDown', { alt: true });
		const rows: string[] = [];
		table.state.doc.descendants((n) => {
			if (n.type.name === 'tableRow') rows.push(n.textContent);
			return true;
		});
		expect(rows).toEqual(['DayCost', 'Sat55', 'Fri40', 'Sun20']);
		expect(table.state.doc.childCount).toBe(1);
	});

	/** Type `text` at the caret one character at a time, input rules running. */
	function typeKeys(editor: Editor, text: string) {
		for (const ch of text) {
			const { from, to } = editor.state.selection;
			const handled = editor.view.someProp('handleTextInput', (f) => f(editor.view, from, to, ch, () => editor.state.tr.insertText(ch, from, to)));
			if (!handled) editor.view.dispatch(editor.state.tr.insertText(ch, from, to));
		}
	}

	// The CodeMirror editor draws `- [ ]` as a to-do, and writes one so.
	it('typed `- [ ] ` and `- [x] ` make a to-do, not a bullet starting with brackets', () => {
		const open = bound('<p></p>');
		open.commands.setTextSelection(1);
		typeKeys(open, '- [ ] Buy milk');
		const todo = open.state.doc.child(0).child(0);
		expect([open.state.doc.childCount, open.state.doc.child(0).type.name, todo.type.name, todo.attrs.checked, todo.textContent]).toEqual([
			1,
			'taskList',
			'taskItem',
			false,
			'Buy milk',
		]);

		const done = bound('<p></p>');
		done.commands.setTextSelection(1);
		typeKeys(done, '- [x] Pack');
		const item = done.state.doc.child(0).child(0);
		expect([done.state.doc.child(0).type.name, item.type.name, item.attrs.checked, item.textContent]).toEqual(['taskList', 'taskItem', true, 'Pack']);

		// An item in the middle of a list becomes a to-do list of its own.
		const middle = bound('<ul><li><p>one</p></li><li><p>two</p></li><li><p>three</p></li></ul>');
		middle.commands.setTextSelection(posIn(middle, 'two'));
		typeKeys(middle, '[ ] ');
		expect(blocks(middle)).toEqual(['bulletList:one', 'taskList:two', 'bulletList:three']);
	});

	it('a selection of cells has its table: the bar acts on it, and the cell menu keeps it', () => {
		const editor = bound(TABLE);
		const sel = CellSelection.create(editor.state.doc, cellPos(editor, 'Fri'), cellPos(editor, 'Sun'));
		editor.view.dispatch(editor.state.tr.setSelection(sel));
		expect(cellAt(editor.state)).toMatchObject({ row: 3, col: 0 });
		expect(tableActions(editor).map((a) => a.id)).toContain('delete-row');
		// The menu opened on a cell inside the selection keeps it.
		const kept = selectionForCellMenu(editor.state, cellPos(editor, 'Sat') + 2);
		expect(kept).toBe(editor.state.selection);
		// Opened on a cell outside it, the caret goes there.
		expect(selectionForCellMenu(editor.state, cellPos(editor, '55') + 2)).toBeInstanceOf(TextSelection);
		tableActions(editor)
			.find((a) => a.id === 'delete-row')!
			.run();
		const rows: string[] = [];
		editor.state.doc.descendants((n) => {
			if (n.type.name === 'tableRow') rows.push(n.textContent);
			return true;
		});
		expect(rows).toEqual(['DayCost']);
	});
});
