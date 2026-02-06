/**
 * Placeholder Plugin for ProseMirror
 *
 * Shows placeholder text when the editor is empty.
 */

import { Plugin, PluginKey } from 'prosemirror-state';
import { Decoration, DecorationSet } from 'prosemirror-view';

export const placeholderKey = new PluginKey('placeholder');

interface PlaceholderPluginOptions {
	text?: string;
}

export function createPlaceholderPlugin(options: PlaceholderPluginOptions = {}) {
	const placeholderText = options.text ?? 'Type / to see options or @ to link entities';

	return new Plugin({
		key: placeholderKey,

		props: {
			decorations(state) {
				const { doc } = state;

				// Check if doc is empty (single empty paragraph)
				const isEmpty =
					doc.childCount === 1 &&
					doc.firstChild?.isTextblock &&
					doc.firstChild.content.size === 0;

				if (!isEmpty) {
					return DecorationSet.empty;
				}

				// Add placeholder decoration to the first node
				const decoration = Decoration.node(0, doc.firstChild!.nodeSize, {
					class: 'is-empty',
					'data-placeholder': placeholderText,
				});

				return DecorationSet.create(doc, [decoration]);
			},
		},
	});
}
