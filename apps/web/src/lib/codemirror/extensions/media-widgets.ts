/**
 * Media & File Widget Decorations
 *
 * Detects ![alt](url) markdown syntax and renders inline widgets:
 * - Images → <img> preview
 * - Audio → <audio> player with header
 * - Video → <video> player
 * - Other files → compact file card (icon + name + size)
 *
 * Right-click context menu on all media types: Go to, Copy, Turn into reference, Edit, Remove.
 *
 * Type determined by file extension. Uses StateField (not ViewPlugin)
 * because block widgets require direct decoration provision via
 * EditorView.decorations facet.
 */

import { type EditorState, type Extension, type Range, StateField } from '@codemirror/state';
import { Decoration, type DecorationSet, type EditorView, EditorView as EditorViewValue, WidgetType } from '@codemirror/view';
import { contextMenu } from '$lib/stores/contextMenu.svelte';

const MEDIA_REGEX = /!\[([^\]]*)\]\(([^)]+)\)/g;

const IMAGE_EXTS = /\.(png|jpg|jpeg|gif|webp|svg|bmp|ico|avif|heic|heif|tiff?)$/i;
const AUDIO_EXTS = /\.(mp3|wav|ogg|m4a|aac|flac|opus|wma)$/i;
const VIDEO_EXTS = /\.(mp4|webm|mov|avi|mkv|m4v|ogv)$/i;

type FileType = 'image' | 'audio' | 'video' | 'file';

function detectFileType(url: string, alt: string): FileType {
	if (IMAGE_EXTS.test(url) || IMAGE_EXTS.test(alt)) return 'image';
	if (AUDIO_EXTS.test(url) || AUDIO_EXTS.test(alt)) return 'audio';
	if (VIDEO_EXTS.test(url) || VIDEO_EXTS.test(alt)) return 'video';
	return 'file';
}

/** Parse alt text for optional width: "alt|600" → { alt: "alt", width: 600 } */
function parseAltWidth(raw: string): { alt: string; width: number | null } {
	const pipeIdx = raw.lastIndexOf('|');
	if (pipeIdx < 0) return { alt: raw, width: null };
	const maybeWidth = raw.slice(pipeIdx + 1).trim();
	const num = parseInt(maybeWidth, 10);
	if (Number.isNaN(num) || num <= 0 || num > 10000) return { alt: raw, width: null };
	return { alt: raw.slice(0, pipeIdx).trim(), width: num };
}

function getFilename(url: string, alt: string): string {
	if (alt) return alt;
	try {
		const path = new URL(url, 'https://x').pathname;
		return path.split('/').pop() || url;
	} catch {
		return url;
	}
}

function getFileExtension(name: string): string {
	const dot = name.lastIndexOf('.');
	return dot >= 0 ? name.slice(dot + 1).toLowerCase() : '';
}

/** Map file extension to a Remix Icon name */
function getFileIcon(ext: string): string {
	const map: Record<string, string> = {
		pdf: 'ri:file-pdf-2-line',
		doc: 'ri:file-word-line', docx: 'ri:file-word-line',
		xls: 'ri:file-excel-line', xlsx: 'ri:file-excel-line', csv: 'ri:file-excel-line',
		ppt: 'ri:file-ppt-line', pptx: 'ri:file-ppt-line',
		zip: 'ri:file-zip-line', gz: 'ri:file-zip-line', tar: 'ri:file-zip-line', rar: 'ri:file-zip-line', '7z': 'ri:file-zip-line',
		txt: 'ri:file-text-line', md: 'ri:file-text-line', rtf: 'ri:file-text-line',
		js: 'ri:file-code-line', ts: 'ri:file-code-line', py: 'ri:file-code-line', rs: 'ri:file-code-line',
		html: 'ri:file-code-line', css: 'ri:file-code-line', json: 'ri:file-code-line',
	};
	return map[ext] || 'ri:file-line';
}

// =============================================================================
// Context Menu
// =============================================================================

function showMediaContextMenu(
	e: MouseEvent,
	view: EditorView,
	from: number,
	to: number,
	href: string,
) {
	e.preventDefault();
	e.stopPropagation();

	contextMenu.show({ x: e.clientX, y: e.clientY }, [
		{
			id: 'go-to',
			label: 'Go to',
			icon: 'ri:arrow-right-up-line',
			action: () => {
				window.open(href, '_blank', 'noopener');
			},
		},
		{
			id: 'copy-link',
			label: 'Copy link',
			icon: 'ri:file-copy-line',
			action: () => {
				const fullUrl = href.startsWith('/') ? `${window.location.origin}${href}` : href;
				navigator.clipboard.writeText(fullUrl);
			},
		},
		{
			id: 'turn-into-reference',
			label: 'Turn into reference',
			icon: 'ri:link',
			dividerBefore: true,
			action: () => {
				// Remove the '!' before '[' to convert ![alt](url) to [alt](url)
				view.dispatch({ changes: { from, to: from + 1, insert: '' } });
			},
		},
		{
			id: 'edit',
			label: 'Edit',
			icon: 'ri:edit-line',
			action: () => {
				view.dispatch({ selection: { anchor: from } });
				view.focus();
			},
		},
		{
			id: 'remove',
			label: 'Remove',
			icon: 'ri:delete-bin-line',
			variant: 'destructive' as const,
			action: () => {
				view.dispatch({ changes: { from, to, insert: '' } });
			},
		},
	]);
}

// =============================================================================
// Widget Classes
// =============================================================================

class ImageWidget extends WidgetType {
	private displayAlt: string;
	private width: number | null;

	constructor(private src: string, private rawAlt: string, private from: number, private to: number) {
		super();
		const parsed = parseAltWidth(rawAlt);
		this.displayAlt = parsed.alt;
		this.width = parsed.width;
	}

	toDOM(view: EditorView) {
		const wrapper = document.createElement('div');
		wrapper.className = 'cm-image-wrapper';

		const img = document.createElement('img');
		img.className = 'cm-image';
		img.src = this.src;
		img.alt = this.displayAlt;
		img.loading = 'lazy';
		if (this.width) {
			img.style.width = `${this.width}px`;
			img.style.maxWidth = '100%';
		}
		img.onerror = () => {
			wrapper.textContent = `Image failed to load: ${this.displayAlt || this.src}`;
			wrapper.className = 'cm-image-error';
		};

		wrapper.appendChild(img);

		wrapper.addEventListener('contextmenu', (e) => {
			showMediaContextMenu(e, view, this.from, this.to, this.src);
		});

		return wrapper;
	}

	eq(other: ImageWidget) {
		return other.src === this.src && other.rawAlt === this.rawAlt;
	}
}

class AudioWidget extends WidgetType {
	constructor(private src: string, private name: string, private from: number, private to: number) {
		super();
	}

	toDOM(view: EditorView) {
		const wrapper = document.createElement('div');
		wrapper.className = 'cm-audio-wrapper';

		const header = document.createElement('div');
		header.className = 'cm-audio-header';

		const icon = document.createElement('iconify-icon');
		icon.setAttribute('icon', 'ri:music-2-line');
		icon.setAttribute('width', '16');
		header.appendChild(icon);

		const nameEl = document.createElement('span');
		nameEl.className = 'cm-audio-name';
		nameEl.textContent = this.name;
		header.appendChild(nameEl);

		const audio = document.createElement('audio');
		audio.className = 'cm-audio-player';
		audio.src = this.src;
		audio.controls = true;
		audio.preload = 'metadata';

		wrapper.appendChild(header);
		wrapper.appendChild(audio);

		wrapper.addEventListener('contextmenu', (e) => {
			showMediaContextMenu(e, view, this.from, this.to, this.src);
		});

		return wrapper;
	}

	eq(other: AudioWidget) {
		return other.src === this.src;
	}
}

class VideoWidget extends WidgetType {
	constructor(private src: string, private from: number, private to: number) {
		super();
	}

	toDOM(view: EditorView) {
		const wrapper = document.createElement('div');
		wrapper.className = 'cm-video-wrapper';

		const video = document.createElement('video');
		video.className = 'cm-video-player';
		video.src = this.src;
		video.controls = true;
		video.preload = 'metadata';

		wrapper.appendChild(video);

		wrapper.addEventListener('contextmenu', (e) => {
			showMediaContextMenu(e, view, this.from, this.to, this.src);
		});

		return wrapper;
	}

	eq(other: VideoWidget) {
		return other.src === this.src;
	}
}

class FileCardWidget extends WidgetType {
	constructor(private src: string, private name: string, private from: number, private to: number) {
		super();
	}

	toDOM(view: EditorView) {
		const wrapper = document.createElement('a');
		wrapper.className = 'cm-file-card';
		wrapper.href = this.src;
		wrapper.target = '_blank';
		wrapper.rel = 'noopener';
		wrapper.addEventListener('click', (e) => {
			e.stopPropagation();
		});

		const ext = getFileExtension(this.name);

		const icon = document.createElement('iconify-icon');
		icon.setAttribute('icon', getFileIcon(ext));
		icon.setAttribute('width', '20');
		icon.className = 'cm-file-card-icon';
		wrapper.appendChild(icon);

		const info = document.createElement('div');
		info.className = 'cm-file-card-info';

		const nameEl = document.createElement('span');
		nameEl.className = 'cm-file-card-name';
		nameEl.textContent = this.name;
		info.appendChild(nameEl);

		if (ext) {
			const extEl = document.createElement('span');
			extEl.className = 'cm-file-card-ext';
			extEl.textContent = ext.toUpperCase();
			info.appendChild(extEl);
		}

		wrapper.appendChild(info);

		const dl = document.createElement('iconify-icon');
		dl.setAttribute('icon', 'ri:download-line');
		dl.setAttribute('width', '16');
		dl.className = 'cm-file-card-download';
		wrapper.appendChild(dl);

		wrapper.addEventListener('contextmenu', (e) => {
			showMediaContextMenu(e, view, this.from, this.to, this.src);
		});

		return wrapper;
	}

	eq(other: FileCardWidget) {
		return other.src === this.src && other.name === this.name;
	}

	ignoreEvent() {
		return false;
	}
}

// =============================================================================
// Decoration Builder
// =============================================================================

function buildMediaDecorations(state: EditorState): DecorationSet {
	const builder: Range<Decoration>[] = [];
	const doc = state.doc;

	// Active-line exclusion
	const cursorLine = doc.lineAt(state.selection.main.head).number;

	for (let lineNum = 1; lineNum <= doc.lines; lineNum++) {
		const line = doc.line(lineNum);
		MEDIA_REGEX.lastIndex = 0;

		for (let match = MEDIA_REGEX.exec(line.text); match !== null; match = MEDIA_REGEX.exec(line.text)) {
			const rawAlt = match[1];
			const url = match[2];
			const from = line.from + match.index;
			const to = from + match[0].length;
			// Strip |width suffix for filename/type detection
			const cleanAlt = parseAltWidth(rawAlt).alt;
			const filename = getFilename(url, cleanAlt);
			const type = detectFileType(url, cleanAlt);

			let widget: WidgetType;
			switch (type) {
				case 'audio':
					widget = new AudioWidget(url, filename, from, to);
					break;
				case 'video':
					widget = new VideoWidget(url, from, to);
					break;
				case 'file':
					widget = new FileCardWidget(url, filename, from, to);
					break;
				default:
					widget = new ImageWidget(url, rawAlt, from, to);
					break;
			}

			// Show widget below the markdown line
			builder.push(
				Decoration.widget({
					widget,
					side: 1,
					block: true,
				}).range(to)
			);

			// Hide the markdown syntax when cursor is not on this line
			if (lineNum !== cursorLine) {
				builder.push(Decoration.replace({}).range(from, to));
			}
		}
	}

	return Decoration.set(builder, true);
}

const mediaField = StateField.define<DecorationSet>({
	create(state) {
		return buildMediaDecorations(state);
	},
	update(decos, tr) {
		if (tr.docChanged || tr.selection) {
			return buildMediaDecorations(tr.state);
		}
		return decos;
	},
	provide: (field) => EditorViewValue.decorations.from(field),
});

export const mediaWidgets: Extension = mediaField;
