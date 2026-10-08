/**
 * Svelte components as ProseMirror node views, for the contract's widgets
 * (an applet, a mention). The editor never measures or steps around them:
 * ProseMirror treats an atom node as one position, and the component owns
 * its own DOM and events.
 */

import { mount, unmount, type Component } from 'svelte';
import type { NodeViewRenderer } from '@tiptap/core';

export type ViewState = {
	attrs: Record<string, unknown>;
	selected: boolean;
	/** Write attributes back to the document, in one transaction that syncs like any other. */
	setAttrs: (patch: Record<string, unknown>) => void;
};

export function svelteNodeView(
	component: Component<{ view: ViewState }>,
	tag: 'div' | 'span',
): NodeViewRenderer {
	return (props) => {
		const { editor, getPos } = props;
		let node = props.node;
		const dom = document.createElement(tag);
		dom.contentEditable = 'false';
		const view: ViewState = $state({
			attrs: { ...node.attrs },
			selected: false,
			setAttrs: (patch) => {
				const pos = getPos();
				if (pos === undefined) return;
				editor.view.dispatch(
					editor.state.tr.setNodeMarkup(pos, undefined, { ...node.attrs, ...patch }),
				);
			},
		});
		const instance = mount(component, { target: dom, props: { view } });
		return {
			dom,
			update(next) {
				if (next.type !== node.type) return false;
				node = next;
				view.attrs = { ...next.attrs };
				return true;
			},
			selectNode() {
				view.selected = true;
			},
			deselectNode() {
				view.selected = false;
			},
			// Clicks and keys on the widget's own controls stay with the widget.
			stopEvent(event) {
				const target = event.target as HTMLElement | null;
				return !!target?.closest('button, input, select, textarea, [data-widget-control]');
			},
			// The component re-renders its own DOM; none of it is document content.
			ignoreMutation: () => true,
			destroy() {
				void unmount(instance);
			},
		};
	};
}
