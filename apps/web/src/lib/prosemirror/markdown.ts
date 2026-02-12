/**
 * Markdown Parser for ProseMirror
 *
 * Extends prosemirror-markdown to handle:
 * - Media embeds: ![file.mp3](url), ![file.mp4](url)
 * - Checkboxes: - [ ] and - [x]
 * - Tables (GFM)
 *
 * Note: Serialization is handled server-side by the Rust markdown serializer.
 * This parser is only used as a fallback for non-Yjs init paths.
 */

import MarkdownIt from 'markdown-it';
import { MarkdownParser } from 'prosemirror-markdown';
import { schema } from './schema';

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

/**
 * Detect media type from filename extension
 */
function getMediaType(filename: string): 'image' | 'audio' | 'video' | null {
	const ext = filename.split('.').pop()?.toLowerCase() || '';

	const imageExts = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico'];
	const audioExts = ['mp3', 'wav', 'm4a', 'ogg', 'flac', 'aac', 'wma'];
	const videoExts = ['mp4', 'mov', 'webm', 'avi', 'mkv', 'm4v', 'wmv'];

	if (imageExts.includes(ext)) return 'image';
	if (audioExts.includes(ext)) return 'audio';
	if (videoExts.includes(ext)) return 'video';

	return null;
}

// =============================================================================
// MARKDOWN-IT CONFIGURATION
// =============================================================================

// Create markdown-it instance with GFM features
const md = new MarkdownIt({
	html: false,
	linkify: false,
	typographer: false,
});

// Enable GFM tables
md.enable('table');

// Enable strikethrough (~~text~~)
md.enable('strikethrough');

// =============================================================================
// CHECKBOX PLUGIN FOR MARKDOWN-IT
// =============================================================================

// Add checkbox parsing: - [ ] and - [x]
// eslint-disable-next-line @typescript-eslint/no-explicit-any
md.core.ruler.after('inline', 'checkbox', (state: any): void => {
	const tokens = state.tokens;

	for (let i = 0; i < tokens.length; i++) {
		if (tokens[i].type !== 'inline') continue;

		const inline = tokens[i];
		const children = inline.children;
		if (!children) continue;

		for (let j = 0; j < children.length; j++) {
			const token = children[j];
			if (token.type !== 'text') continue;

			// Match checkbox at start of text: [ ] or [x] or [X]
			const match = token.content.match(/^\[([ xX])\]\s*/);
			if (!match) continue;

			const isChecked = match[1].toLowerCase() === 'x';
			const restContent = token.content.slice(match[0].length);

			// Replace with checkbox token + remaining text
			const checkboxToken = new state.Token('checkbox', 'input', 0);
			checkboxToken.attrSet('type', 'checkbox');
			if (isChecked) checkboxToken.attrSet('checked', 'checked');

			const newTokens = [checkboxToken];
			if (restContent) {
				const textToken = new state.Token('text', '', 0);
				textToken.content = restContent;
				newTokens.push(textToken);
			}

			children.splice(j, 1, ...newTokens);
		}
	}
});

// =============================================================================
// PARSER CONFIGURATION
// =============================================================================

export const markdownParser = new MarkdownParser(schema, md, {
	// Block tokens
	blockquote: { block: 'blockquote' },
	paragraph: { block: 'paragraph' },
	list_item: { block: 'list_item' },
	bullet_list: { block: 'bullet_list' },
	ordered_list: {
		block: 'ordered_list',
		getAttrs: (tok) => ({ order: +(tok.attrGet('start') || 1) }),
	},
	heading: {
		block: 'heading',
		getAttrs: (tok) => ({ level: +tok.tag.slice(1) }),
	},
	code_block: {
		block: 'code_block',
		getAttrs: (tok) => ({ language: tok.info || '' }),
	},
	fence: {
		block: 'code_block',
		getAttrs: (tok) => ({ language: tok.info || '' }),
		noCloseToken: true,
	},
	hr: { node: 'horizontal_rule' },

	// Table tokens
	table: { block: 'table' },
	thead: { ignore: true },
	tbody: { ignore: true },
	tr: { block: 'table_row' },
	th: {
		block: 'table_header',
		getAttrs: (tok) => {
			const style = tok.attrGet('style') || '';
			const alignMatch = style.match(/text-align:\s*(\w+)/);
			return { align: alignMatch ? alignMatch[1] : null };
		},
	},
	td: {
		block: 'table_cell',
		getAttrs: (tok) => {
			const style = tok.attrGet('style') || '';
			const alignMatch = style.match(/text-align:\s*(\w+)/);
			return { align: alignMatch ? alignMatch[1] : null };
		},
	},

	// Inline tokens
	image: {
		node: 'image',
		getAttrs: (tok) => {
			const src = tok.attrGet('src') || '';
			const alt = tok.attrGet('alt') || tok.content || '';
			const title = tok.attrGet('title');

			// Detect media type from alt text (which contains filename)
			const mediaType = getMediaType(alt);

			// For audio/video, we'll handle this specially
			// The default parser creates image nodes, we'll convert later
			return {
				src,
				alt,
				title,
				_mediaType: mediaType, // Temporary attr for post-processing
			};
		},
	},
	hardbreak: { node: 'hard_break' },
	softbreak: { node: 'hard_break' },

	// Inline marks
	em: { mark: 'em' },
	strong: { mark: 'strong' },
	s: { mark: 'strikethrough' },
	link: {
		mark: 'link',
		getAttrs: (tok) => ({
			href: tok.attrGet('href'),
			title: tok.attrGet('title') || null,
		}),
	},
	code_inline: { mark: 'code' },

	// Checkbox
	checkbox: {
		node: 'checkbox',
		getAttrs: (tok) => ({
			checked: tok.attrGet('checked') === 'checked',
		}),
	},
});

// =============================================================================
// CONVENIENCE FUNCTIONS
// =============================================================================

/**
 * Parse markdown string to ProseMirror document
 */
export function parseMarkdown(markdown: string) {
	return markdownParser.parse(markdown);
}
