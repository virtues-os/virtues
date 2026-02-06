/**
 * Markdown Parser & Serializer for ProseMirror
 *
 * Extends prosemirror-markdown to handle:
 * - Entity links: [Name](/person/id), [Name](/page/id), etc.
 * - Media embeds: ![file.mp3](url) → audio_player, ![file.mp4](url) → video_player
 * - CriticMarkup: {++additions++} and {--deletions--}
 * - Checkboxes: - [ ] and - [x]
 * - Tables (GFM)
 */

import MarkdownIt from 'markdown-it';
import { MarkdownParser, MarkdownSerializer } from 'prosemirror-markdown';
import { schema } from './schema';

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

/**
 * Detect entity URLs (internal app routes)
 */
const ENTITY_PREFIXES = [
	'/person/',
	'/place/',
	'/thing/',
	'/org/',
	'/page/',
	'/day/',
	'/year/',
	'/source/',
	'/chat/',
	'/drive/',
];

function isEntityUrl(url: string): boolean {
	return ENTITY_PREFIXES.some((prefix) => url.startsWith(prefix));
}

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

/**
 * Check if URL is a drive file (for file card rendering)
 */
function isDriveUrl(url: string): boolean {
	return url.startsWith('/drive/');
}

/**
 * Check if URL is external
 */
function isExternalUrl(url: string): boolean {
	return url.startsWith('http://') || url.startsWith('https://');
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
// POST-PROCESS PARSED DOCUMENT
// =============================================================================

/**
 * Post-process the parsed document to:
 * 1. Convert images with audio/video extensions to audio_player/video_player nodes
 * 2. Convert links to entity URLs to entity_link nodes
 * 3. Convert drive file links to file_card nodes
 */
export function postProcessDocument(doc: ReturnType<typeof markdownParser.parse>) {
	// This would require walking the document and replacing nodes
	// For now, we'll handle this in the node views instead
	// The parser creates standard nodes, and node views render them appropriately
	return doc;
}

// =============================================================================
// SERIALIZER CONFIGURATION
// =============================================================================

export const markdownSerializer = new MarkdownSerializer(
	{
		// Block nodes
		blockquote(state, node) {
			state.wrapBlock('> ', null, node, () => state.renderContent(node));
		},
		code_block(state, node) {
			const lang = node.attrs.language || '';
			state.write('```' + lang + '\n');
			state.text(node.textContent, false);
			state.ensureNewLine();
			state.write('```');
			state.closeBlock(node);
		},
		heading(state, node) {
			state.write('#'.repeat(node.attrs.level) + ' ');
			state.renderInline(node);
			state.closeBlock(node);
		},
		horizontal_rule(state, node) {
			state.write('---');
			state.closeBlock(node);
		},
		bullet_list(state, node) {
			state.renderList(node, '  ', () => '- ');
		},
		ordered_list(state, node) {
			const start = node.attrs.order || 1;
			state.renderList(node, '  ', (i) => `${start + i}. `);
		},
		list_item(state, node) {
			state.renderContent(node);
		},
		paragraph(state, node) {
			state.renderInline(node);
			state.closeBlock(node);
		},
		image(state, node) {
			state.write(`![${state.esc(node.attrs.alt || '')}](${node.attrs.src}${node.attrs.title ? ` "${state.esc(node.attrs.title)}"` : ''})`);
		},
		hard_break(state) {
			state.write('\n');
		},
		text(state, node) {
			state.text(node.text || '');
		},

		// Table nodes
		table(state, node) {
			// Collect rows
			const rows: string[][] = [];
			const aligns: (string | null)[] = [];

			node.forEach((row) => {
				const cells: string[] = [];

				row.forEach((cell) => {
					// Use textContent to recursively get all text (handles paragraph wrappers)
					cells.push(cell.textContent.trim());

					if (rows.length === 0) {
						aligns.push(cell.attrs.align);
					}
				});

				rows.push(cells);
			});

			if (rows.length === 0) return;

			// Write header row
			state.write('| ' + rows[0].join(' | ') + ' |');
			state.ensureNewLine();

			// Write separator row with alignment
			const sep = aligns.map((align) => {
				if (align === 'left') return ':---';
				if (align === 'right') return '---:';
				if (align === 'center') return ':---:';
				return '---';
			});
			state.write('| ' + sep.join(' | ') + ' |');
			state.ensureNewLine();

			// Write data rows
			for (let i = 1; i < rows.length; i++) {
				state.write('| ' + rows[i].join(' | ') + ' |');
				state.ensureNewLine();
			}

			state.closeBlock(node);
		},
		table_row() {
			// Handled by table
		},
		table_cell() {
			// Handled by table
		},
		table_header() {
			// Handled by table
		},

		// Custom nodes
		entity_link(state, node) {
			state.write(`[${node.attrs.label}](${node.attrs.href})`);
		},
		audio_player(state, node) {
			state.write(`![${node.attrs.name}](${node.attrs.src})`);
			state.closeBlock(node);
		},
		video_player(state, node) {
			state.write(`![${node.attrs.name}](${node.attrs.src})`);
			state.closeBlock(node);
		},
		file_card(state, node) {
			state.write(`[${node.attrs.name}](${node.attrs.href})`);
		},
		checkbox(state, node) {
			state.write(node.attrs.checked ? '[x] ' : '[ ] ');
		},
	},
	{
		// Mark serializers
		em: { open: '*', close: '*', mixable: true, expelEnclosingWhitespace: true },
		strong: { open: '**', close: '**', mixable: true, expelEnclosingWhitespace: true },
		code: { open: '`', close: '`', escape: false },
		strikethrough: { open: '~~', close: '~~', mixable: true, expelEnclosingWhitespace: true },
		link: {
			open(_state, mark) {
				return '[';
			},
			close(state, mark) {
				return `](${mark.attrs.href}${mark.attrs.title ? ` "${state.esc(mark.attrs.title)}"` : ''})`;
			},
		},
	}
);

// =============================================================================
// CONVENIENCE FUNCTIONS
// =============================================================================

/**
 * Parse markdown string to ProseMirror document
 */
export function parseMarkdown(markdown: string) {
	const doc = markdownParser.parse(markdown);
	return postProcessDocument(doc);
}

/**
 * Serialize ProseMirror document to markdown string
 */
export function serializeMarkdown(doc: Parameters<typeof markdownSerializer.serialize>[0]) {
	return markdownSerializer.serialize(doc);
}
