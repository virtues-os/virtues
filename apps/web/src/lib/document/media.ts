/**
 * Files into a block page: pasted, dropped or picked files become an image,
 * audio, video or file block the moment they are given, in one edit with
 * taking out the selection they replace (over selected table cells, every
 * cell empties and they land in the first). Each block holds its file's
 * final address from the start, `/drive/<id>`, with a Drive id made here
 * (`mintFileId`), and the bytes follow outside the page (`uploads.ts`). So
 * the page changes once, when the person acts: nothing is written after an
 * upload, which can then race nothing (not typing, not undo, not another
 * device), and a block whose file is still on its way is an ordinary block
 * that undo takes out and redo puts back, and that syncs, moves and copies
 * like any other. What it shows while its file is on its way, or after it
 * failed, is the block's own (`MediaNode.svelte`).
 *
 * Files arrive on a paste only when they are what was copied. Office copies
 * a picture of the range beside its HTML; the HTML is the content then, and
 * the editor pastes it, a picture in the range included (`filesAreTheContent`).
 */

import { Extension } from '@tiptap/core';
import type { Node as PmNode, Schema } from '@tiptap/pm/model';
import { Plugin, TextSelection, type Selection } from '@tiptap/pm/state';
import { CellSelection } from '@tiptap/pm/tables';
import type { EditorView } from '@tiptap/pm/view';
import { toast } from 'svelte-sonner';
import { decoded } from '$lib/utils/urlUtils';
import type { Anchor } from './anchor';
import { ownUndoStep } from './history';
import { kindOfLink, type MediaKind } from './media-kind';
import { pastPageLimit, tellPastLimit } from './paste';
import { insertBlockAt } from './place';
import { getTreeEditor, openTreeEditors } from './registry';
import { mintFileId, startUpload, unsettledUploads, uploadBlocksBack, uploadBlocksLeft } from './uploads';

/** The largest file a page holds: the server's `MAX_FILE_SIZE` (`api/media.rs`). */
export const MAX_UPLOAD_BYTES = 100 * 1024 * 1024;

/** Send a file to the address its block holds: `startUpload` by default. */
export type Send = (id: string, file: File) => void;

/** The Drive file an address points at (`/drive/<id>` or its download address), if any. */
export function driveFileId(src: string): string | null {
	const m = /^\/(?:drive\/([^/?#]+)|api\/drive\/files\/([^/?#]+)\/download)(?:[?#].*)?$/.exec(src);
	return m ? decoded(m[1] ?? m[2]) : null;
}

/** What a media block with no name of its own is called: its address's last part. */
export function nameOfSrc(src: string): string {
	const path = src.split(/[?#]/)[0];
	return decoded(path.split('/').filter(Boolean).pop() ?? src);
}

/**
 * The address a media block loads from. A Drive ref (`/drive/<id>`) names
 * the file, not its bytes, so it loads from the file's download address.
 */
export function mediaSrc(src: string): string {
	const id = driveFileId(src);
	return id === null ? src : `/api/drive/files/${encodeURIComponent(id)}/download`;
}

/** A file's block, from its type: what the browser says it is. */
export function kindOfMime(mime: string | null | undefined): MediaKind {
	if (!mime) return 'file';
	if (mime.startsWith('image/')) return 'image';
	if (mime.startsWith('audio/')) return 'audio';
	if (mime.startsWith('video/')) return 'video';
	return 'file';
}

/**
 * The block for a file at `src`, in this schema. A kind the schema does not
 * hold falls back to a file block, then to nothing.
 */
export function mediaNode(schema: Schema, kind: MediaKind, src: string, name: string): PmNode | null {
	if (kind === 'image' && schema.nodes.image) return schema.nodes.image.create({ src, alt: name || null });
	const type = schema.nodes[kind] ?? schema.nodes.file;
	if (!type) return null;
	return type.create({ src, name: name || null });
}

/** The extension a file of this type is written with, for a name that has none. */
const EXTENSION_OF: Record<string, string> = {
	'image/jpeg': 'jpg',
	'image/png': 'png',
	'image/gif': 'gif',
	'image/webp': 'webp',
	'image/svg+xml': 'svg',
	'image/heic': 'heic',
	'image/heif': 'heif',
	'image/avif': 'avif',
	'image/bmp': 'bmp',
	'image/tiff': 'tiff',
	'audio/mpeg': 'mp3',
	'audio/mp4': 'm4a',
	'audio/x-m4a': 'm4a',
	'audio/aac': 'aac',
	'audio/wav': 'wav',
	'audio/x-wav': 'wav',
	'audio/ogg': 'ogg',
	'audio/flac': 'flac',
	'audio/webm': 'webm',
	'video/mp4': 'mp4',
	'video/quicktime': 'mov',
	'video/webm': 'webm',
	'application/pdf': 'pdf',
};

/**
 * What a file's block calls it: its name, with an extension from its type
 * when it has none. The page's markdown reads a Drive file's kind from its
 * name's extension (`kindOfLink`, `media_kind` in the crate), so a picture
 * pasted as `image` would come back from the markdown as a file.
 */
export function nameForBlock(file: File): string {
	const name = file.name || 'file';
	const ext = EXTENSION_OF[file.type.toLowerCase()];
	if (!ext || /\.[a-z0-9]+$/i.test(name)) return name;
	return `${name}.${ext}`;
}

/** A file's block, from its type, else from its name. */
export function kindOfFile(file: File, name: string): MediaKind {
	const byType = kindOfMime(file.type);
	return byType !== 'file' ? byType : kindOfLink('', name);
}

/**
 * Put a block for each of `files` at `at`, in the order given, now, and send
 * each file after: `at` is a position in the page as it is, or an anchor
 * taken earlier (the insert menu's, held while the file dialog was open).
 * With `replacing`, that selection goes in the same edit. One undo takes it
 * all back. A file larger than a page holds is said and left out, and
 * files that would take the page past what it holds (`MAX_NODES`,
 * `MAX_DEPTH`) are refused and said, as a paste is (`pastPageLimit`).
 */
export function addFiles(
	view: EditorView,
	files: File[],
	at: number | Anchor,
	o: { replacing?: Selection; send?: Send } = {},
): void {
	const fit = files.filter((file) => file.size <= MAX_UPLOAD_BYTES);
	for (const file of files) {
		if (file.size > MAX_UPLOAD_BYTES) {
			toast.error(`${file.name} is over 100 MB`, {
				description: 'A page holds files up to 100 MB. Upload it to Drive and link to it from the page instead.',
			});
		}
	}
	if (!fit.length) return;
	const { state } = view;
	let pos = typeof at === 'number' ? at : at.resolve();
	if (pos === null) {
		pos = state.selection.from;
		toast('Added at the cursor', {
			description: 'Someone deleted the line you were adding the files to while you chose them.',
		});
	}
	const tr = state.tr;
	pos = Math.min(pos, tr.doc.content.size);
	const replacing = o.replacing && !o.replacing.empty ? o.replacing : null;
	if (replacing) {
		const first = Math.min(...replacing.ranges.map((r) => r.$from.pos));
		replacing.replace(tr);
		pos = tr.mapping.map(first, -1);
		// An emptied cell holds an empty line, and the file takes its place.
		const after = tr.doc.resolve(pos).nodeAfter;
		if (replacing instanceof CellSelection && after?.isTextblock && !after.content.size) pos += 1;
	}
	const caretHere = tr.selection.empty && tr.selection.from === pos;
	const sent: [string, File][] = [];
	for (const file of fit) {
		const id = mintFileId();
		const name = nameForBlock(file);
		const node = mediaNode(state.schema, kindOfFile(file, name), `/drive/${id}`, name);
		if (!node) continue;
		pos = insertBlockAt(tr, pos, node);
		sent.push([id, file]);
	}
	if (!sent.length) return;
	if (caretHere) tr.setSelection(TextSelection.near(tr.doc.resolve(pos)));
	// The server refuses an edit that takes a page past what it holds, and
	// the page would stop saving: nothing goes in, and nothing is sent.
	const past = pastPageLimit(state.doc, tr);
	if (past) {
		tellPastLimit(past, 'files');
		return;
	}
	ownUndoStep(view, tr.scrollIntoView());
	const send = o.send ?? startUpload;
	for (const [id, file] of sent) send(id, file);
}

/** Files on a paste or a drop. */
function filesOf(data: DataTransfer | null): File[] {
	return data ? Array.from(data.files ?? []) : [];
}

/**
 * Whether a paste's files are what was copied. A picture copied from a web
 * page is HTML holding that image and no words: the file is it. Any other
 * HTML is the content, and a file beside it is a picture of that content
 * (Word, Excel and PowerPoint copy one): the HTML is pasted, not the
 * picture. That holds when the range has a picture in it too, which Word's
 * HTML carries as an image on the copying computer (`file:`), a picture
 * the page cannot load and drops; its words are what was copied. Read in
 * an inert document: nothing in a paste's HTML loads or runs.
 */
export function filesAreTheContent(data: DataTransfer | null): boolean {
	const html = data?.getData('text/html') ?? '';
	if (!html.trim()) return true;
	const body = new DOMParser().parseFromString(html, 'text/html').body;
	const images = [...body.querySelectorAll('img')];
	if (!images.length || images.some((img) => /^file:/i.test(img.getAttribute('src')?.trim() ?? ''))) return false;
	return !(body.textContent ?? '').trim();
}

/**
 * Which of `ids` `doc` names: by a file's block, or by a link to the
 * file's address, as Turn into link leaves one. Either way the page points
 * at the address the file fills, so the file is still the page's.
 */
export function filesNamedIn(doc: PmNode, ids: ReadonlySet<string>): Set<string> {
	const held = new Set<string>();
	const name = (src: unknown) => {
		const id = typeof src === 'string' ? driveFileId(src) : null;
		if (id !== null && ids.has(id)) held.add(id);
	};
	doc.descendants((node) => {
		if (node.isText) {
			for (const mark of node.marks) if (mark.type.name === 'link') name(mark.attrs.href);
			return false;
		}
		if (node.isAtom) {
			name(node.attrs.src);
			return false;
		}
		return true;
	});
	return held;
}

/**
 * Tell the upload queue when a block whose file is still on its way leaves
 * every page open here, or is in one again (`uploadBlocksLeft`,
 * `uploadBlocksBack`): undo, Remove and a cut take it out, redo and a paste
 * put it back, and so may another device's edit; opening a page shows its
 * own. A block turned into a link to its address has not left
 * (`filesNamedIn`), and neither has one still in another open page. What
 * the queue does with a block that left is its own: it asks the server
 * whether any page still names the file. Closing a page takes nothing out:
 * edits it holds that the server has not had would say no page names a
 * file it does. Only while this device holds such a file is any page
 * looked through.
 */
function followFileBlocks(view: EditorView, before: PmNode | null): void {
	const watched = unsettledUploads();
	if (!watched.size) return;
	const now = filesNamedIn(view.state.doc, watched);
	if (before === null) {
		uploadBlocksBack(now);
		return;
	}
	const was = filesNamedIn(before, watched);
	uploadBlocksBack([...now].filter((id) => !was.has(id)));
	const others = openTreeEditors().filter((editor) => editor.view !== view && !editor.isDestroyed);
	const left = [...was].filter((id) => !now.has(id));
	uploadBlocksLeft(left.filter((id) => !others.some((editor) => filesNamedIn(editor.state.doc, new Set([id])).size)));
}

/**
 * How many images on the page `pageId`, open here, are files this device
 * has not finished sending: shared now, the page would go without them. The
 * server refuses such a share once it has heard of the file, and this
 * device knows of it first.
 */
export function imagesOnTheirWay(pageId: string): number {
	const editor = getTreeEditor(pageId);
	const unsettled = unsettledUploads();
	if (!editor || !unsettled.size) return 0;
	let n = 0;
	editor.state.doc.descendants((node) => {
		if (node.type.name !== 'image') return true;
		const id = typeof node.attrs.src === 'string' ? driveFileId(node.attrs.src) : null;
		if (id !== null && unsettled.has(id)) n++;
		return false;
	});
	return n;
}

export function mediaUploads(send: Send = startUpload): Extension {
	return Extension.create({
		name: 'pageMediaUploads',
		addProseMirrorPlugins() {
			return [
				new Plugin({
					view(view) {
						followFileBlocks(view, null);
						return {
							update(view, prev) {
								if (view.state.doc !== prev.doc) followFileBlocks(view, prev.doc);
							},
						};
					},
					props: {
						handlePaste(view, event) {
							const files = filesOf(event.clipboardData);
							if (!files.length || !filesAreTheContent(event.clipboardData)) return false;
							const { selection } = view.state;
							addFiles(view, files, selection.from, { replacing: selection, send });
							return true;
						},
						handleDrop(view, event, _slice, moved) {
							if (moved) return false;
							const files = filesOf(event.dataTransfer);
							if (!files.length) return false;
							const at = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
							event.preventDefault();
							addFiles(view, files, at ?? view.state.selection.from, { send });
							return true;
						},
					},
				}),
			];
		},
	});
}
