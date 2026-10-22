// @vitest-environment happy-dom
/**
 * The Svelte node views (`svelteNodeView`, `svelteContentNodeView`): what
 * they hand their component as the page changes around it. Each component
 * is a stand-in that records the `view` it was mounted with; `$state`
 * outside the compiler is a plain object, so a field the node view assigns
 * shows as a new value.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';

const mounted = vi.hoisted(() => {
	(globalThis as Record<string, unknown>).$state = <T>(v: T) => v;
	return [] as { name: string; view: { attrs: Record<string, unknown>; editable: boolean; type: string } }[];
});

vi.mock('svelte', async (orig) => ({
	...(await orig<typeof import('svelte')>()),
	mount: (component: (props: unknown) => void, o: { props: unknown }) => {
		component(o.props);
		return {};
	},
	unmount: () => undefined,
}));

function standIn(name: string) {
	return (props: { view: (typeof mounted)[number]['view'] }) => mounted.push({ name, view: props.view });
}

vi.mock('$lib/components/pages/nodes/MentionNode.svelte', () => ({ default: standIn('MentionNode') }));
vi.mock('$lib/components/pages/nodes/MediaNode.svelte', () => ({ default: standIn('MediaNode') }));
vi.mock('$lib/components/pages/nodes/AppletNode.svelte', () => ({ default: standIn('AppletNode') }));
vi.mock('$lib/components/pages/nodes/CalloutNode.svelte', () => ({ default: standIn('CalloutNode') }));
vi.mock('$lib/components/pages/nodes/CodeBlockNode.svelte', () => ({ default: standIn('CodeBlockNode') }));
vi.mock('$lib/components/pages/nodes/FootnoteNode.svelte', () => ({ default: standIn('FootnoteNode') }));
vi.mock('$lib/components/pages/nodes/FigureNode.svelte', () => ({ default: standIn('FigureNode') }));
vi.mock('svelte-sonner', () => ({ toast: Object.assign(vi.fn(), { error: vi.fn() }) }));

import type { Editor } from '@tiptap/core';
import { createPageEditor } from './editor';
import { aiPresence, setAiTrail } from './presence';
import { pageDoc } from './test-utils';

const editors: Editor[] = [];

afterEach(() => {
	for (const e of editors.splice(0)) if (!e.isDestroyed) e.destroy();
	document.body.replaceChildren();
	mounted.length = 0;
});

function open(html: string): Editor {
	const { doc } = pageDoc(html);
	const element = document.createElement('div');
	document.body.append(element);
	const editor = createPageEditor({
		element,
		doc: { ydoc: doc, provider: null },
		editable: true,
		placeholder: 'Write',
		plugins: [aiPresence()],
	});
	editors.push(editor);
	return editor;
}

const PAGE =
	'<p>Lunch with <virtues-mention to="/person/person_1" label="Nick"></virtues-mention> on Friday.</p>' +
	'<aside data-tone="tip"><p>Bring a jacket.</p></aside>';

function viewOf(name: string) {
	const found = mounted.find((m) => m.name === name)?.view;
	if (!found) throw new Error(`${name} was not mounted`);
	return found;
}

describe('node views', () => {
	it('keep the attributes they hand the component when only the decorations around it change', () => {
		const editor = open(PAGE);
		const mention = viewOf('MentionNode');
		const callout = viewOf('CalloutNode');
		const mentionAttrs = mention.attrs;
		const calloutAttrs = callout.attrs;
		// A wash over the mention and the callout: a decoration change, no edit.
		setAiTrail(editor, { from: 1, to: editor.state.doc.content.size - 1 });
		expect(editor.view.dom.querySelector('.doc-ai-trail')).not.toBeNull();
		expect(mention.attrs).toBe(mentionAttrs);
		expect(callout.attrs).toBe(calloutAttrs);
	});

	it('hand the component new attributes when the node changes', () => {
		const editor = open(PAGE);
		const mention = viewOf('MentionNode');
		let at = -1;
		editor.state.doc.descendants((node, pos) => {
			if (node.type.name === 'mention') at = pos;
			return at < 0;
		});
		const before = mention.attrs;
		editor.view.dispatch(editor.state.tr.setNodeAttribute(at, 'label', 'David Okafor'));
		expect(mention.attrs).not.toBe(before);
		expect(mention.attrs).toMatchObject({ label: 'David Okafor' });
	});

	it('follow the editor becoming read only, and editable again', () => {
		const editor = open(PAGE);
		const mention = viewOf('MentionNode');
		const callout = viewOf('CalloutNode');
		expect(mention.editable).toBe(true);
		editor.setEditable(false);
		expect(mention.editable).toBe(false);
		expect(callout.editable).toBe(false);
		editor.setEditable(true);
		expect(mention.editable).toBe(true);
		expect(callout.editable).toBe(true);
	});
});
