/**
 * Svelte components as ProseMirror node views, for the contract's widgets
 * (an applet, a mention, media) and for blocks with chrome around their
 * text (a callout's tone, a code block's language and Copy). The editor
 * never measures or steps around them: ProseMirror treats an atom node as
 * one position, and the component owns its own DOM and events.
 *
 * Mounting a view writes nothing. A component writes the document only
 * through `setAttrs`, in answer to the person, and the document's own
 * changes reach it through `view.attrs`.
 */

import { mount, unmount, type Component } from 'svelte';
import type { Editor, NodeViewRenderer, NodeViewRendererProps } from '@tiptap/core';
import type { Node as PmNode } from '@tiptap/pm/model';
import { NodeSelection } from '@tiptap/pm/state';

export type ViewState = {
	/** The node's type name: `image`, `audio`, `callout`, … */
	type: string;
	attrs: Record<string, unknown>;
	selected: boolean;
	/** Whether the page can be edited here; a read-only page offers no handles. */
	editable: boolean;
	/** Write attributes back to the document, in one transaction that syncs like any other. */
	setAttrs: (patch: Record<string, unknown>) => void;
	/** Remove the node. */
	remove: () => void;
	/** Replace the node with others (`Turn into link`). */
	replaceWith: (nodes: PmNode | PmNode[]) => void;
	/** Select the node, as a click on it would. */
	select: () => void;
	/** The node's text now: a code block's code, for Copy. */
	text: () => string;
	editor: Editor;
};

export interface NodeViewOptions {
	/**
	 * Mount the component only once the node scrolls into view: for widgets
	 * that cost a frame or a request each (an applet's face). Until then the
	 * node's DOM holds `placeholder`.
	 */
	lazy?: boolean;
	/** What a lazy node shows until it is mounted, and what it prints as when never shown. */
	placeholder?: (attrs: Record<string, unknown>) => HTMLElement;
	/** A class on the node's outer element. */
	className?: string;
}

/**
 * Mount a widget's component into `target`. A component that throws while
 * drawing (an attribute it cannot read) leaves its block saying so, not the
 * whole page unopened: a node view that throws stops the editor building.
 */
function mountOrSay(
	component: Component<{ view: ViewState }>,
	target: HTMLElement,
	view: ViewState,
): Record<string, unknown> | null {
	try {
		return mount(component, { target, props: { view } });
	} catch (e) {
		console.error(`[document] a ${view.type} block could not be drawn`, e);
		const note = document.createElement('span');
		note.className = 'doc-widget-failed';
		note.textContent = "Couldn't show this block. The rest of the page works as usual.";
		target.replaceChildren(note);
		return null;
	}
}

/** Controls inside a widget keep their events; the editor does not see them. */
const WIDGET_CONTROL = 'button, input, select, textarea, a[href], audio, video, iframe, [data-widget-control]';

function viewState(props: NodeViewRendererProps, current: () => PmNode): ViewState {
	const { editor, getPos } = props;
	const node = props.node;
	const pos = () => (typeof getPos === 'function' ? getPos() : undefined);
	const view: ViewState = $state({
		type: node.type.name,
		attrs: { ...node.attrs },
		selected: false,
		editable: editor.isEditable,
		editor,
		setAttrs: (patch) => {
			const at = pos();
			if (at === undefined || !editor.isEditable) return;
			editor.view.dispatch(editor.state.tr.setNodeMarkup(at, undefined, { ...current().attrs, ...patch }));
		},
		remove: () => {
			const at = pos();
			if (at === undefined || !editor.isEditable) return;
			editor.view.dispatch(editor.state.tr.delete(at, at + current().nodeSize));
		},
		replaceWith: (nodes) => {
			const at = pos();
			if (at === undefined || !editor.isEditable) return;
			editor.view.dispatch(editor.state.tr.replaceWith(at, at + current().nodeSize, nodes));
		},
		select: () => {
			const at = pos();
			if (at === undefined) return;
			editor.view.dispatch(editor.state.tr.setSelection(NodeSelection.create(editor.state.doc, at)));
		},
		text: () => current().textContent,
	});
	return view;
}

/** Mirror the node's attributes as `data-*` on its outer element, for styling and print. */
function mirrorAttrs(dom: HTMLElement, node: PmNode): void {
	for (const [key, value] of Object.entries(node.attrs)) {
		const name = `data-${key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`;
		if (key === 'id') continue;
		if (value === null || value === undefined || value === false) dom.removeAttribute(name);
		else dom.setAttribute(name, String(value));
	}
}

export function svelteNodeView(
	component: Component<{ view: ViewState }>,
	tag: 'div' | 'span',
	options: NodeViewOptions = {},
): NodeViewRenderer {
	return (props) => {
		const { editor } = props;
		let node = props.node;
		const dom = document.createElement(tag);
		dom.contentEditable = 'false';
		dom.classList.add('doc-widget', `doc-widget-${node.type.name}`);
		if (options.className) dom.classList.add(options.className);
		mirrorAttrs(dom, node);
		const view = viewState(props, () => node);

		let instance: Record<string, unknown> | null = null;
		let tried = false;
		const mountNow = () => {
			if (tried) return;
			tried = true;
			dom.replaceChildren();
			instance = mountOrSay(component, dom, view);
		};

		let observer: IntersectionObserver | null = null;
		if (options.lazy && typeof IntersectionObserver !== 'undefined') {
			if (options.placeholder) dom.append(options.placeholder(view.attrs));
			observer = new IntersectionObserver(
				(entries) => {
					if (!entries.some((e) => e.isIntersecting)) return;
					observer?.disconnect();
					observer = null;
					mountNow();
				},
				{ rootMargin: '400px 0px' },
			);
			observer.observe(dom);
		} else {
			mountNow();
		}

		return {
			dom,
			update(next) {
				if (next.type !== node.type) return false;
				node = next;
				view.attrs = { ...next.attrs };
				view.editable = editor.isEditable;
				mirrorAttrs(dom, next);
				return true;
			},
			selectNode() {
				view.selected = true;
				dom.classList.add('ProseMirror-selectednode');
			},
			deselectNode() {
				view.selected = false;
				dom.classList.remove('ProseMirror-selectednode');
			},
			// Clicks and keys on the widget's own controls stay with the widget.
			stopEvent(event) {
				const target = event.target as HTMLElement | null;
				return !!target?.closest(WIDGET_CONTROL);
			},
			// The component re-renders its own DOM; none of it is document content.
			ignoreMutation: () => true,
			destroy() {
				observer?.disconnect();
				if (instance) void unmount(instance);
				instance = null;
			},
		};
	};
}

/**
 * A block with editable content and chrome around it: the component renders
 * the chrome (a tone picker, a language picker and Copy) in an uneditable
 * part, and ProseMirror renders the content in `contentDOM`.
 *
 * For `pre`, the content goes in a `code` inside the `pre`, as the HTML has it.
 */
export function svelteContentNodeView(
	component: Component<{ view: ViewState }>,
	tag: 'aside' | 'pre' | 'div',
): NodeViewRenderer {
	return (props) => {
		const { editor } = props;
		let node = props.node;
		const dom = document.createElement(tag === 'pre' ? 'div' : tag);
		dom.classList.add('doc-block', `doc-block-${node.type.name}`);
		mirrorAttrs(dom, node);
		const chrome = document.createElement('div');
		chrome.contentEditable = 'false';
		chrome.className = 'doc-block-chrome';
		let contentDOM: HTMLElement;
		if (tag === 'pre') {
			const pre = document.createElement('pre');
			contentDOM = document.createElement('code');
			pre.append(contentDOM);
			dom.append(chrome, pre);
		} else {
			contentDOM = document.createElement('div');
			contentDOM.className = 'doc-block-content';
			dom.append(chrome, contentDOM);
		}
		const view = viewState(props, () => node);
		const instance = mountOrSay(component, chrome, view);

		return {
			dom,
			contentDOM,
			update(next) {
				if (next.type !== node.type) return false;
				node = next;
				view.attrs = { ...next.attrs };
				view.editable = editor.isEditable;
				mirrorAttrs(dom, next);
				return true;
			},
			stopEvent(event) {
				const target = event.target as HTMLElement | null;
				return !!target && chrome.contains(target);
			},
			// Only the content is the document's; the chrome and the outer
			// element's mirrored attributes are the view's own.
			ignoreMutation(mutation) {
				if (mutation.type === 'selection') return false;
				return !contentDOM.contains(mutation.target);
			},
			destroy() {
				if (instance) void unmount(instance);
			},
		};
	};
}
