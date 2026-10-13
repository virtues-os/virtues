/**
 * The page's node views and per-type editor behaviour, handed to
 * `contractExtensions` as its extras: what the contract leaves to the
 * editor (`selectable`, `draggable`, `defining`, `inclusive`) and the Svelte
 * components that draw widgets. Names the contract does not have yet are
 * ignored by `contractExtensions`, so this builds before they land.
 */

import MentionNode from '$lib/components/pages/nodes/MentionNode.svelte';
import MediaNode from '$lib/components/pages/nodes/MediaNode.svelte';
import AppletNode from '$lib/components/pages/nodes/AppletNode.svelte';
import CalloutNode from '$lib/components/pages/nodes/CalloutNode.svelte';
import CodeBlockNode from '$lib/components/pages/nodes/CodeBlockNode.svelte';
import type { Extras } from './schema';
import { appletPlaceholder } from './applet';
import { svelteContentNodeView, svelteNodeView } from './nodeviews.svelte';

export function pageNodeViews(): Extras {
	const media = { selectable: true, draggable: true, addNodeView: () => svelteNodeView(MediaNode, 'div') };
	return {
		nodes: {
			mention: { selectable: true, addNodeView: () => svelteNodeView(MentionNode, 'span') },
			image: media,
			audio: media,
			video: media,
			file: media,
			applet: {
				selectable: true,
				draggable: true,
				addNodeView: () => svelteNodeView(AppletNode, 'div', { lazy: true, placeholder: appletPlaceholder }),
			},
			callout: { defining: true, addNodeView: () => svelteContentNodeView(CalloutNode, 'aside') },
			codeBlock: { addNodeView: () => svelteContentNodeView(CodeBlockNode, 'pre') },
		},
		marks: {
			// Typing at a proposal's edge writes plain text, not more proposal;
			// inside a deletion, `TypingOutsideDeletions` keeps it plain too.
			proposedDeletion: { inclusive: false },
			proposedInsertion: { inclusive: false },
		},
	};
}
