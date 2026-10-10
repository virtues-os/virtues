/**
 * What a link to a file is drawn as: an image, audio, video or a file card,
 * from its address and its name. One rule for both page editors (the block
 * editor's embeds, `links.ts`, and CodeMirror's media widgets,
 * `codemirror/extensions/media-widgets.ts`) and the server's converter
 * (`media_kind` in the crate), pinned to it by the cases both read
 * (`crates/virtues-document/tests/corpus/media-kinds.json`). It imports
 * nothing, so CodeMirror reaches it without the block editor.
 */

export type MediaKind = 'image' | 'audio' | 'video' | 'file';

const IMAGE_EXT = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'bmp', 'ico', 'avif', 'heic', 'heif', 'tif', 'tiff'];
const AUDIO_EXT = ['mp3', 'wav', 'ogg', 'm4a', 'aac', 'flac', 'opus', 'wma'];
const VIDEO_EXT = ['mp4', 'webm', 'mov', 'avi', 'mkv', 'm4v', 'ogv'];

/**
 * A name's or an address's extension, lowercase: what follows the last `.`
 * of its last segment, a query or fragment left out. Empty when it has none:
 * a name that is all extension (`.mp3`) has none.
 */
function extension(s: string): string {
	const last = s.split(/[?#]/)[0].split('/').pop() ?? '';
	const dot = last.lastIndexOf('.');
	const ext = dot > 0 ? last.slice(dot + 1) : '';
	return /^[a-z0-9]+$/i.test(ext) ? ext.toLowerCase() : '';
}

/**
 * A link's block, from its address and name: an image, audio or video
 * extension on either makes that block; past that, a web address with no
 * extension of its own is an image (the web serves images from such
 * addresses), and anything else is a file, a web address ending in `.pdf`
 * included.
 */
export function kindOfLink(src: string, name: string): MediaKind {
	const onSrc = extension(src);
	const onName = extension(name);
	const has = (list: string[]) => list.includes(onSrc) || list.includes(onName);
	if (has(IMAGE_EXT)) return 'image';
	if (has(AUDIO_EXT)) return 'audio';
	if (has(VIDEO_EXT)) return 'video';
	// The URL parser's reading: tabs and line breaks dropped, control
	// characters and spaces trimmed from both ends.
	const address = src.replace(/[\t\n\r]/g, '').replace(/^[\u0000-\u0020]+|[\u0000-\u0020]+$/g, '');
	return /^https?:/i.test(address) && !onSrc ? 'image' : 'file';
}

/**
 * A media link's name and width, from its alt text: `alt|600` is `alt`
 * drawn 600 pixels wide. The last `|` starts a width when only digits
 * follow it, as the server's converter reads it; otherwise it is part of
 * the name. The name decides the kind (`kindOfLink`), so a width never
 * turns `photo.jpg|600` into a file.
 */
export function altWidth(raw: string): { alt: string; width: number | null } {
	const pipe = raw.lastIndexOf('|');
	if (pipe < 0) return { alt: raw, width: null };
	const digits = raw.slice(pipe + 1).trim();
	const width = /^\d+$/.test(digits) ? Number(digits) : 0;
	if (width <= 0 || width > 10000) return { alt: raw, width: null };
	return { alt: raw.slice(0, pipe).trim(), width };
}
