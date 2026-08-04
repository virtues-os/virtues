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
import { linkEditor } from '$lib/stores/linkEditor.svelte';
import { isEntityRoute } from '$lib/utils/refRoutes';

import { collectCodeRanges, inCode } from './code-context';
import { onContextGesture } from './long-press';

const MEDIA_REGEX = /!\[([^\]]*)\]\(([^)]+)\)/g;

const IMAGE_EXTS = /\.(png|jpg|jpeg|gif|webp|svg|bmp|ico|avif|heic|heif|tiff?)$/i;
const AUDIO_EXTS = /\.(mp3|wav|ogg|m4a|aac|flac|opus|wma)$/i;
const VIDEO_EXTS = /\.(mp4|webm|mov|avi|mkv|m4v|ogv)$/i;

type FileType = 'image' | 'audio' | 'video' | 'file';

function detectFileType(url: string, alt: string): FileType {
	if (IMAGE_EXTS.test(url) || IMAGE_EXTS.test(alt)) return 'image';
	if (AUDIO_EXTS.test(url) || AUDIO_EXTS.test(alt)) return 'audio';
	if (VIDEO_EXTS.test(url) || VIDEO_EXTS.test(alt)) return 'video';
	// Extensionless EXTERNAL urls default to image: `![…]` is image syntax,
	// and the modern web serves images from extensionless CDN/signed urls
	// (unsplash's `photo-…?w=400` rendered as a file-card button before this).
	// A wrong guess self-heals — ImageWidget swaps to a file card on error.
	// Internal paths (drive downloads) keep extension detection: the drive
	// picker embeds pdfs and archives with `![name](/api/drive/…)`, and their
	// alt carries the real filename for the tests above.
	if (/^https?:\/\//i.test(url)) return 'image';
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

/**
 * The file-card DOM, shared by FileCardWidget and by ImageWidget's error
 * fallback (an extensionless external url guessed as image that turns out to
 * be something else degrades to this card instead of an error box).
 * Context gestures are the caller's to attach.
 */
function buildFileCardDOM(src: string, name: string): HTMLAnchorElement {
	const card = document.createElement('a');
	card.className = 'cm-file-card';
	card.href = src;
	card.target = '_blank';
	card.rel = 'noopener';
	card.addEventListener('click', (e) => {
		e.stopPropagation();
	});

	const ext = getFileExtension(name);

	const icon = document.createElement('iconify-icon');
	icon.setAttribute('icon', getFileIcon(ext));
	icon.setAttribute('width', '20');
	icon.className = 'cm-file-card-icon';
	card.appendChild(icon);

	const info = document.createElement('div');
	info.className = 'cm-file-card-info';

	const nameEl = document.createElement('span');
	nameEl.className = 'cm-file-card-name';
	nameEl.textContent = name;
	info.appendChild(nameEl);

	if (ext) {
		const extEl = document.createElement('span');
		extEl.className = 'cm-file-card-ext';
		extEl.textContent = ext.toUpperCase();
		info.appendChild(extEl);
	}

	card.appendChild(info);

	const dl = document.createElement('iconify-icon');
	dl.setAttribute('icon', 'ri:download-line');
	dl.setAttribute('width', '16');
	dl.className = 'cm-file-card-download';
	card.appendChild(dl);

	return card;
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

/**
 * The media markdown's CURRENT range, derived from the DOM at action time.
 *
 * Widgets capture from/to at construction, but `eq()` keeps the DOM (and the
 * listener closures with it) alive across rebuilds, so any edit earlier in
 * the document silently shifts the real range out from under those captured
 * numbers. With stale numbers, "Remove" deletes the wrong span and "Edit"
 * parses garbage — verified live: after a two-character edit upstream, the
 * Edit panel came up with an empty label because the slice no longer matched.
 * Same re-derivation trick as the checkbox and copy-button handlers.
 */
function mediaRangeAtDOM(view: EditorView, dom: HTMLElement): { from: number; to: number } | null {
	const pos = view.posAtDOM(dom);
	const line = view.state.doc.lineAt(Math.min(pos, view.state.doc.length));
	let first: { from: number; to: number } | null = null;
	MEDIA_REGEX.lastIndex = 0;
	for (let m = MEDIA_REGEX.exec(line.text); m !== null; m = MEDIA_REGEX.exec(line.text)) {
		const from = line.from + m.index;
		const to = from + m[0].length;
		if (!first) first = { from, to };
		if (pos >= from && pos <= to) return { from, to };
	}
	return first;
}

function showMediaContextMenu(
	x: number,
	y: number,
	view: EditorView,
	from: number,
	to: number,
	href: string,
	isImage = false,
) {
	/** Rewrite the whole `![alt|w](url)` with a new width (null strips it). */
	const setWidth = (w: number | null) => {
		const m = /^!\[([^\]]*)\]\(([^)]+)\)$/.exec(view.state.sliceDoc(from, to));
		if (!m) return;
		const { alt } = parseAltWidth(m[1]);
		view.dispatch({
			changes: { from, to, insert: `![${alt}${w ? `|${w}` : ''}](${m[2]})` },
		});
	};

	const resizeItems = isImage
		? [
				{
					id: 'width-small',
					label: 'Small (320px)',
					icon: 'ri:contract-left-right-line',
					dividerBefore: true,
					action: () => setWidth(320),
				},
				{
					id: 'width-medium',
					label: 'Medium (600px)',
					icon: 'ri:pause-line',
					action: () => setWidth(600),
				},
				{
					id: 'width-original',
					label: 'Original size',
					icon: 'ri:expand-left-right-line',
					action: () => setWidth(null),
				},
			]
		: [];

	contextMenu.show({ x, y }, [
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
				// This used to drop the caret at the media line so the raw
				// `![alt](url)` would reveal — media never reveals now, so that
				// dispatch did nothing at all. Same panel as links; the alt text
				// is the "Text" field, and a `|width` suffix survives the edit.
				const m = /^!\[([^\]]*)\]\(([^)]+)\)$/.exec(view.state.sliceDoc(from, to));
				const { alt, width } = parseAltWidth(m?.[1] ?? '');
				linkEditor.show(
					{ label: alt, href: m?.[2] ?? href },
					({ label, href: newHref }) => {
						view.dispatch({
							changes: {
								from,
								to,
								insert: `![${label}${width ? `|${width}` : ''}](${newHref})`,
							},
						});
						view.focus();
					},
					{ x, y, width: 0, height: 0 },
				);
			},
		},
		...resizeItems,
		{
			id: 'remove',
			label: 'Remove',
			icon: 'ri:delete-bin-line',
			variant: 'destructive' as const,
			dividerBefore: isImage,
			action: () => {
				view.dispatch({ changes: { from, to, insert: '' } });
			},
		},
	]);
}

// =============================================================================
// Widget Classes
// =============================================================================

// Images/video load async and grow AFTER CodeMirror measured the block, which
// leaves every line below mispositioned (clicks + arrow up/down land on the
// wrong line). Observe the widget and ask CodeMirror to re-measure when its size
// settles. requestMeasure is batched, so this is cheap. The observer is stored
// on the element so destroy() can disconnect it regardless of widget reuse.
type MeasuredEl = HTMLElement & { _cmResizeObs?: ResizeObserver };

function remeasureOnResize(view: EditorView, el: HTMLElement) {
	const ro = new ResizeObserver(() => view.requestMeasure());
	ro.observe(el);
	(el as MeasuredEl)._cmResizeObs = ro;
}

function disconnectRemeasure(dom: HTMLElement) {
	(dom as MeasuredEl)._cmResizeObs?.disconnect();
}

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
			// Wrong guess or dead link — degrade to the file card, which stays
			// clickable and keeps this wrapper's context menu, instead of the
			// old dead-end error box. The ResizeObserver already attached to
			// the wrapper re-measures the height change for CodeMirror.
			wrapper.replaceChildren(
				buildFileCardDOM(this.src, getFilename(this.src, this.displayAlt)),
			);
		};

		wrapper.appendChild(img);

		onContextGesture(wrapper, (x, y) => {
			const range = mediaRangeAtDOM(view, wrapper);
			if (!range) return;
			showMediaContextMenu(x, y, view, range.from, range.to, this.src, true);
		});

		remeasureOnResize(view, wrapper);
		return wrapper;
	}

	destroy(dom: HTMLElement) {
		disconnectRemeasure(dom);
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

		onContextGesture(wrapper, (x, y) => {
			const range = mediaRangeAtDOM(view, wrapper);
			if (!range) return;
			showMediaContextMenu(x, y, view, range.from, range.to, this.src);
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

		onContextGesture(wrapper, (x, y) => {
			const range = mediaRangeAtDOM(view, wrapper);
			if (!range) return;
			showMediaContextMenu(x, y, view, range.from, range.to, this.src);
		});

		remeasureOnResize(view, wrapper);
		return wrapper;
	}

	destroy(dom: HTMLElement) {
		disconnectRemeasure(dom);
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
		// The card sits in a wrapper so the vertical spacing lives in a box
		// CodeMirror can measure (see .cm-file-card / .cm-table-wrapper notes
		// in theme.css — margins on a block widget corrupt the heightmap).
		const wrapper = document.createElement('div');
		wrapper.className = 'cm-file-card-wrapper';
		wrapper.appendChild(buildFileCardDOM(this.src, this.name));

		onContextGesture(wrapper, (x, y) => {
			const range = mediaRangeAtDOM(view, wrapper);
			if (!range) return;
			showMediaContextMenu(x, y, view, range.from, range.to, this.src);
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

	// An `![x](y)` inside a code fence is example text, not an image to embed.
	const codeRanges = collectCodeRanges(state, 0, doc.length);

	for (let lineNum = 1; lineNum <= doc.lines; lineNum++) {
		const line = doc.line(lineNum);
		MEDIA_REGEX.lastIndex = 0;

		for (let match = MEDIA_REGEX.exec(line.text); match !== null; match = MEDIA_REGEX.exec(line.text)) {
			const rawAlt = match[1];
			const url = match[2];
			const matchFrom = line.from + match.index;
			if (inCode(codeRanges, matchFrom, matchFrom + match[0].length)) continue;
			// App refs (`![@X](/person/id)`, `![file](/drive/id)`) are ref embeds,
			// rendered by ref-links; media widgets only handle direct/external urls.
			if (isEntityRoute(url)) continue;
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

			// The `![alt](url)` line is always hidden. It used to reappear when
			// the caret landed on it, which pushed the widget and everything
			// below it down by a line for as long as the caret stayed.
			builder.push(Decoration.replace({}).range(from, to));
		}
	}

	return Decoration.set(builder, true);
}

const mediaField = StateField.define<DecorationSet>({
	create(state) {
		return buildMediaDecorations(state);
	},
	update(decos, tr) {
		if (!tr.docChanged) return decos;

		// The builder scans the WHOLE document (a StateField has no viewport,
		// and block widgets have to come from one), so rebuilding on every
		// keystroke was O(document). Most edits cannot possibly change a media
		// widget: the rebuild is skipped — decorations just mapped through —
		// unless the edit (a) touches a line that contains media syntax,
		// (b) touches an existing widget's range, or (c) involves a backtick or
		// tilde, which can open/close a code fence and flip media far away from
		// the edit into or out of code context.
		let rebuild = false;
		tr.changes.iterChanges((fromA, toA, _fromB, toB, inserted) => {
			if (rebuild) return;

			const removed = tr.startState.sliceDoc(fromA, toA);
			const added = inserted.toString();
			if (/[`~]/.test(removed) || /[`~]/.test(added)) {
				rebuild = true;
				return;
			}

			const startLine = tr.state.doc.lineAt(Math.min(_fromB, tr.state.doc.length));
			const endLine = tr.state.doc.lineAt(Math.min(toB, tr.state.doc.length));
			for (let n = startLine.number; n <= endLine.number; n++) {
				MEDIA_REGEX.lastIndex = 0;
				if (MEDIA_REGEX.test(tr.state.doc.line(n).text)) {
					rebuild = true;
					return;
				}
			}

			decos.between(Math.max(0, fromA - 1), Math.min(tr.startState.doc.length, toA + 1), () => {
				rebuild = true;
				return false;
			});
		});

		return rebuild ? buildMediaDecorations(tr.state) : decos.map(tr.changes);
	},
	provide: (field) => EditorViewValue.decorations.from(field),
});

export const mediaWidgets: Extension = mediaField;
