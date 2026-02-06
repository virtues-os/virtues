/**
 * ProseMirror Node Views
 *
 * Custom rendering for special nodes like entity links, media players, file cards.
 * Each node view provides interactive DOM elements that integrate with the editor.
 */

import type { Node } from 'prosemirror-model';
import type { EditorView, NodeView } from 'prosemirror-view';

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

/**
 * Get file icon based on extension
 */
function getFileIcon(filename: string): string {
	const ext = filename.split('.').pop()?.toLowerCase() || '';
	if (['pdf'].includes(ext)) return 'ri:file-pdf-line';
	if (['doc', 'docx'].includes(ext)) return 'ri:file-word-line';
	if (['xls', 'xlsx'].includes(ext)) return 'ri:file-excel-line';
	if (['ppt', 'pptx'].includes(ext)) return 'ri:file-ppt-line';
	if (['zip', 'rar', 'tar', 'gz', '7z'].includes(ext)) return 'ri:file-zip-line';
	if (['txt', 'md'].includes(ext)) return 'ri:file-text-line';
	if (['js', 'ts', 'py', 'rs', 'go', 'java', 'c', 'cpp', 'h'].includes(ext)) return 'ri:file-code-line';
	return 'ri:file-line';
}

/**
 * Get domain from URL for favicon
 */
function getDomain(url: string): string {
	try {
		return new URL(url).hostname;
	} catch {
		return '';
	}
}

/**
 * Create an iconify-icon element
 */
function createIcon(iconName: string, width = 14): HTMLElement {
	const icon = document.createElement('iconify-icon');
	icon.setAttribute('icon', iconName);
	icon.setAttribute('width', width.toString());
	return icon;
}

/**
 * Dispatch a page-navigate event for SvelteKit navigation
 */
function dispatchNavigate(element: HTMLElement, href: string) {
	element.dispatchEvent(
		new CustomEvent('page-navigate', {
			bubbles: true,
			detail: { href },
		})
	);
}

// =============================================================================
// ENTITY LINK NODE VIEW
// =============================================================================

export class EntityLinkView implements NodeView {
	dom: HTMLElement;

	constructor(node: Node, view: EditorView, getPos: () => number | undefined) {
		const href = node.attrs.href;
		const label = node.attrs.label;

		this.dom = document.createElement('a');
		this.dom.className = 'pm-entity-link';
		this.dom.setAttribute('href', href);
		this.dom.textContent = `@${label}`;

		// Click handler
		this.dom.addEventListener('click', (e) => {
			e.preventDefault();
			e.stopPropagation();
			dispatchNavigate(this.dom, href);
		});
	}

	stopEvent() {
		return true;
	}

	ignoreMutation() {
		return true;
	}
}

// =============================================================================
// EXTERNAL LINK NODE VIEW (for links rendered as marks, used via decoration)
// =============================================================================

// Note: External links use the standard 'link' mark, not a custom node
// This class is provided for potential future use with decorations

// =============================================================================
// IMAGE NODE VIEW
// =============================================================================

export class ImageView implements NodeView {
	dom: HTMLElement;
	img: HTMLImageElement;

	constructor(node: Node, view: EditorView, getPos: () => number | undefined) {
		this.dom = document.createElement('span');
		this.dom.className = 'pm-image-wrapper';

		this.img = document.createElement('img');
		this.img.className = 'pm-image';
		this.img.src = node.attrs.src;
		this.img.alt = node.attrs.alt || '';
		this.img.loading = 'lazy';

		this.img.onerror = () => {
			this.dom.classList.add('pm-image-error');
			this.img.style.display = 'none';
			const placeholder = document.createElement('span');
			placeholder.className = 'pm-image-placeholder';
			placeholder.textContent = `[Image: ${node.attrs.alt || 'failed to load'}]`;
			this.dom.appendChild(placeholder);
		};

		this.dom.appendChild(this.img);
	}

	stopEvent() {
		return false;
	}
}

// =============================================================================
// AUDIO PLAYER NODE VIEW
// =============================================================================

export class AudioPlayerView implements NodeView {
	dom: HTMLElement;

	constructor(node: Node, view: EditorView, getPos: () => number | undefined) {
		this.dom = document.createElement('div');
		this.dom.className = 'pm-audio-wrapper';

		// Header with icon and name
		const header = document.createElement('div');
		header.className = 'pm-audio-header';
		header.appendChild(createIcon('ri:music-2-line', 16));

		const nameSpan = document.createElement('span');
		nameSpan.className = 'pm-audio-name';
		nameSpan.textContent = node.attrs.name;
		header.appendChild(nameSpan);
		this.dom.appendChild(header);

		// Audio element
		const audio = document.createElement('audio');
		audio.className = 'pm-audio-player';
		audio.src = node.attrs.src;
		audio.controls = true;
		audio.preload = 'metadata';
		this.dom.appendChild(audio);
	}

	stopEvent() {
		return true;
	}

	ignoreMutation() {
		return true;
	}
}

// =============================================================================
// VIDEO PLAYER NODE VIEW
// =============================================================================

export class VideoPlayerView implements NodeView {
	dom: HTMLElement;

	constructor(node: Node, view: EditorView, getPos: () => number | undefined) {
		this.dom = document.createElement('div');
		this.dom.className = 'pm-video-wrapper';

		const video = document.createElement('video');
		video.className = 'pm-video-player';
		video.src = node.attrs.src;
		video.controls = true;
		video.preload = 'metadata';
		this.dom.appendChild(video);
	}

	stopEvent() {
		return true;
	}

	ignoreMutation() {
		return true;
	}
}

// =============================================================================
// FILE CARD NODE VIEW
// =============================================================================

export class FileCardView implements NodeView {
	dom: HTMLElement;

	constructor(node: Node, view: EditorView, getPos: () => number | undefined) {
		const href = node.attrs.href;
		const name = node.attrs.name;

		this.dom = document.createElement('a');
		this.dom.className = 'pm-file-card';
		this.dom.setAttribute('href', href);

		// File icon
		this.dom.appendChild(createIcon(getFileIcon(name), 20));

		// File name
		const nameSpan = document.createElement('span');
		nameSpan.className = 'pm-file-card-name';
		nameSpan.textContent = name;
		this.dom.appendChild(nameSpan);

		// Click handler
		this.dom.addEventListener('click', (e) => {
			e.preventDefault();
			e.stopPropagation();
			dispatchNavigate(this.dom, href);
		});
	}

	stopEvent() {
		return true;
	}

	ignoreMutation() {
		return true;
	}
}

// =============================================================================
// CHECKBOX NODE VIEW
// =============================================================================

export class CheckboxView implements NodeView {
	dom: HTMLInputElement;

	constructor(node: Node, view: EditorView, getPos: () => number | undefined) {
		this.dom = document.createElement('input');
		this.dom.type = 'checkbox';
		this.dom.className = 'pm-checkbox';
		this.dom.checked = node.attrs.checked;

		this.dom.addEventListener('mousedown', (e) => {
			e.preventDefault();
			e.stopPropagation();

			const pos = getPos();
			if (pos === undefined) return;

			// Toggle checked state
			view.dispatch(
				view.state.tr.setNodeMarkup(pos, null, {
					...node.attrs,
					checked: !node.attrs.checked,
				})
			);
		});
	}

	stopEvent(event: Event) {
		return event.type === 'mousedown' || event.type === 'click';
	}

	ignoreMutation() {
		return true;
	}
}

// =============================================================================
// HORIZONTAL RULE NODE VIEW
// =============================================================================

export class HorizontalRuleView implements NodeView {
	dom: HTMLElement;

	constructor(node: Node, view: EditorView, getPos: () => number | undefined) {
		this.dom = document.createElement('hr');
		this.dom.className = 'pm-hr';
	}
}

// =============================================================================
// NODE VIEW FACTORY
// =============================================================================

/**
 * Create node views map for ProseMirror EditorView
 */
export function createNodeViews() {
	return {
		entity_link: (node: Node, view: EditorView, getPos: () => number | undefined) =>
			new EntityLinkView(node, view, getPos),
		image: (node: Node, view: EditorView, getPos: () => number | undefined) =>
			new ImageView(node, view, getPos),
		audio_player: (node: Node, view: EditorView, getPos: () => number | undefined) =>
			new AudioPlayerView(node, view, getPos),
		video_player: (node: Node, view: EditorView, getPos: () => number | undefined) =>
			new VideoPlayerView(node, view, getPos),
		file_card: (node: Node, view: EditorView, getPos: () => number | undefined) =>
			new FileCardView(node, view, getPos),
		checkbox: (node: Node, view: EditorView, getPos: () => number | undefined) =>
			new CheckboxView(node, view, getPos),
		horizontal_rule: (node: Node, view: EditorView, getPos: () => number | undefined) =>
			new HorizontalRuleView(node, view, getPos),
	};
}
