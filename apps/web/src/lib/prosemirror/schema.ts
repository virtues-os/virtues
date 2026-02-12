/**
 * ProseMirror Schema for Pages Editor
 *
 * Defines the document structure with support for:
 * - Standard markdown elements (headings, lists, blockquotes, code blocks)
 * - Tables with cell-level editing (via prosemirror-tables)
 * - Custom nodes: entity links, media players, file cards, checkboxes
 * - Standard marks: bold, italic, underline, code, strikethrough, links
 */

import { Schema, type MarkSpec, type Node as PMNode, type NodeSpec } from 'prosemirror-model';
import { tableNodes } from 'prosemirror-tables';

// =============================================================================
// NODE SPECIFICATIONS
// =============================================================================

const nodes: Record<string, NodeSpec> = {
	// Document root
	doc: {
		content: 'block+',
	},

	// Basic text
	text: {
		group: 'inline',
	},

	// Paragraph
	paragraph: {
		content: 'inline*',
		group: 'block',
		parseDOM: [{ tag: 'p' }],
		toDOM() {
			return ['p', 0];
		},
	},

	// Headings (1-6)
	heading: {
		attrs: { level: { default: 1 } },
		content: 'inline*',
		group: 'block',
		defining: true,
		parseDOM: [
			{ tag: 'h1', attrs: { level: 1 } },
			{ tag: 'h2', attrs: { level: 2 } },
			{ tag: 'h3', attrs: { level: 3 } },
			{ tag: 'h4', attrs: { level: 4 } },
			{ tag: 'h5', attrs: { level: 5 } },
			{ tag: 'h6', attrs: { level: 6 } },
		],
		toDOM(node) {
			return ['h' + node.attrs.level, 0];
		},
	},

	// Blockquote
	blockquote: {
		content: 'block+',
		group: 'block',
		defining: true,
		parseDOM: [{ tag: 'blockquote' }],
		toDOM() {
			return ['blockquote', 0];
		},
	},

	// Horizontal rule
	horizontal_rule: {
		group: 'block',
		parseDOM: [{ tag: 'hr' }],
		toDOM() {
			return ['hr'];
		},
	},

	// Code block with language attribute
	code_block: {
		content: 'text*',
		marks: '',
		group: 'block',
		code: true,
		defining: true,
		attrs: { language: { default: '' } },
		parseDOM: [
			{
				tag: 'pre',
				preserveWhitespace: 'full',
				getAttrs(node) {
					const dom = node as HTMLElement;
					const code = dom.querySelector('code');
					const className = code?.className || '';
					const match = className.match(/language-(\w+)/);
					return { language: match ? match[1] : '' };
				},
			},
		],
		toDOM(node) {
			return [
				'pre',
				node.attrs.language ? { 'data-language': node.attrs.language } : {},
				['code', { class: node.attrs.language ? `language-${node.attrs.language}` : '' }, 0],
			];
		},
	},

	// Bullet list
	bullet_list: {
		content: 'list_item+',
		group: 'block',
		parseDOM: [{ tag: 'ul' }],
		toDOM() {
			return ['ul', 0];
		},
	},

	// Ordered list with style variants (decimal, roman, alpha)
	ordered_list: {
		content: 'list_item+',
		group: 'block',
		attrs: {
			order: { default: 1 },
			listStyleType: { default: 'decimal' }, // decimal, upper-roman, lower-roman, upper-alpha, lower-alpha
		},
		parseDOM: [
			{
				tag: 'ol',
				getAttrs(node) {
					const dom = node as HTMLElement;
					const style = dom.style.listStyleType || 'decimal';
					return {
						order: dom.hasAttribute('start') ? +dom.getAttribute('start')! : 1,
						listStyleType: style,
					};
				},
			},
		],
		toDOM(node) {
			const attrs: Record<string, string> = {};
			if (node.attrs.order !== 1) attrs.start = node.attrs.order;
			if (node.attrs.listStyleType !== 'decimal') {
				attrs.style = `list-style-type: ${node.attrs.listStyleType}`;
			}
			return Object.keys(attrs).length ? ['ol', attrs, 0] : ['ol', 0];
		},
	},

	// List item
	list_item: {
		content: 'paragraph block*',
		parseDOM: [{ tag: 'li' }],
		toDOM() {
			return ['li', 0];
		},
		defining: true,
	},

	// Hard break
	hard_break: {
		inline: true,
		group: 'inline',
		selectable: false,
		parseDOM: [{ tag: 'br' }],
		toDOM() {
			return ['br'];
		},
	},

	// Standard image
	image: {
		inline: true,
		attrs: {
			src: {},
			alt: { default: null },
			title: { default: null },
		},
		group: 'inline',
		draggable: true,
		parseDOM: [
			{
				tag: 'img[src]',
				getAttrs(node) {
					const dom = node as HTMLElement;
					return {
						src: dom.getAttribute('src'),
						alt: dom.getAttribute('alt'),
						title: dom.getAttribute('title'),
					};
				},
			},
		],
		toDOM(node) {
			return ['img', node.attrs];
		},
	},

	// =============================================================================
	// CUSTOM NODES
	// =============================================================================

	// Entity link: [Name](/person/id), [Name](/page/id), etc.
	entity_link: {
		inline: true,
		attrs: {
			href: {},
			label: {},
		},
		group: 'inline',
		leafText: (node: PMNode) => node.attrs.label || '',
		parseDOM: [
			{
				tag: 'a.entity-link',
				getAttrs(node) {
					const dom = node as HTMLElement;
					return {
						href: dom.getAttribute('href'),
						label: dom.textContent,
					};
				},
			},
		],
		toDOM(node) {
			return ['a', { class: 'entity-link', href: node.attrs.href }, node.attrs.label];
		},
	},

	// Audio player: ![filename.mp3](/drive/id)
	audio_player: {
		group: 'block',
		attrs: {
			src: {},
			name: { default: '' },
		},
		parseDOM: [
			{
				tag: 'div.audio-player',
				getAttrs(node) {
					const dom = node as HTMLElement;
					const audio = dom.querySelector('audio');
					return {
						src: audio?.getAttribute('src') || '',
						name: dom.getAttribute('data-name') || '',
					};
				},
			},
		],
		toDOM(node) {
			return [
				'div',
				{ class: 'audio-player', 'data-name': node.attrs.name },
				['audio', { src: node.attrs.src, controls: 'true' }],
			];
		},
	},

	// Video player: ![filename.mp4](/drive/id)
	video_player: {
		group: 'block',
		attrs: {
			src: {},
			name: { default: '' },
		},
		parseDOM: [
			{
				tag: 'div.video-player',
				getAttrs(node) {
					const dom = node as HTMLElement;
					const video = dom.querySelector('video');
					return {
						src: video?.getAttribute('src') || '',
						name: dom.getAttribute('data-name') || '',
					};
				},
			},
		],
		toDOM(node) {
			return [
				'div',
				{ class: 'video-player', 'data-name': node.attrs.name },
				['video', { src: node.attrs.src, controls: 'true' }],
			];
		},
	},

	// File card: [filename.pdf](/drive/id) for non-media files
	file_card: {
		inline: true,
		attrs: {
			href: {},
			name: {},
		},
		group: 'inline',
		leafText: (node: PMNode) => node.attrs.name || '',
		parseDOM: [
			{
				tag: 'a.file-card',
				getAttrs(node) {
					const dom = node as HTMLElement;
					return {
						href: dom.getAttribute('href'),
						name: dom.textContent,
					};
				},
			},
		],
		toDOM(node) {
			return ['a', { class: 'file-card', href: node.attrs.href }, node.attrs.name];
		},
	},

	// Checkbox for todo items: - [ ] or - [x]
	checkbox: {
		inline: true,
		attrs: {
			checked: { default: false },
		},
		group: 'inline',
		selectable: false,
		parseDOM: [
			{
				tag: 'input[type=checkbox]',
				getAttrs(node) {
					const dom = node as HTMLInputElement;
					return { checked: dom.checked };
				},
			},
		],
		toDOM(node) {
			return ['input', { type: 'checkbox', checked: node.attrs.checked ? 'checked' : null }];
		},
	},
};

// =============================================================================
// TABLE NODES (from prosemirror-tables)
// =============================================================================

const tableNodeSpecs = tableNodes({
	tableGroup: 'block',
	cellContent: 'block+',
	cellAttributes: {
		align: {
			default: null,
			getFromDOM(dom) {
				return (dom as HTMLElement).style.textAlign || null;
			},
			setDOMAttr(value, attrs) {
				if (value) attrs.style = (attrs.style || '') + `text-align: ${value};`;
			},
		},
	},
});

// Merge table nodes into our node specs
Object.assign(nodes, tableNodeSpecs);

// =============================================================================
// MARK SPECIFICATIONS
// =============================================================================

const marks: Record<string, MarkSpec> = {
	// Standard link
	link: {
		attrs: {
			href: {},
			title: { default: null },
		},
		inclusive: false,
		parseDOM: [
			{
				tag: 'a[href]',
				getAttrs(node) {
					const dom = node as HTMLElement;
					return {
						href: dom.getAttribute('href'),
						title: dom.getAttribute('title'),
					};
				},
			},
		],
		toDOM(node) {
			return ['a', { href: node.attrs.href, title: node.attrs.title }, 0];
		},
	},

	// Emphasis (italic)
	em: {
		parseDOM: [
			{ tag: 'i' },
			{ tag: 'em' },
			{ style: 'font-style=italic' },
			{ style: 'font-style=oblique' },
		],
		toDOM() {
			return ['em', 0];
		},
	},

	// Strong (bold)
	strong: {
		parseDOM: [
			{ tag: 'strong' },
			{ tag: 'b' },
			{
				style: 'font-weight',
				getAttrs: (value) => /^(bold(er)?|[5-9]\d{2,})$/.test(value as string) && null,
			},
		],
		toDOM() {
			return ['strong', 0];
		},
	},

	// Inline code
	code: {
		parseDOM: [{ tag: 'code' }],
		toDOM() {
			return ['code', 0];
		},
	},

	// Strikethrough
	strikethrough: {
		parseDOM: [{ tag: 's' }, { tag: 'del' }, { style: 'text-decoration=line-through' }],
		toDOM() {
			return ['del', 0];
		},
	},

	// Underline (not standard markdown, but useful)
	underline: {
		parseDOM: [{ tag: 'u' }, { style: 'text-decoration=underline' }],
		toDOM() {
			return ['u', 0];
		},
	},

};

// =============================================================================
// EXPORT SCHEMA
// =============================================================================

export const schema = new Schema({ nodes, marks });

// Re-export for convenience
export { nodes, marks };
