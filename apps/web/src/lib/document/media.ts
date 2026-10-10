/**
 * Files into a block page: pasted, dropped or picked files are uploaded
 * (`/api/media/upload`) and become an image, audio, video or file block.
 * While files upload the page shows one placeholder for them, which is a
 * decoration and never a document node: nothing half-made reaches the
 * shared document or another device. The placeholder holds its place by an
 * anchor (`anchor.ts`), so another device's edit meanwhile moves it with the
 * text around it. Files given together land together, in the order they
 * were given whichever finishes first, each after the one before it. A
 * failed upload is said, and the rest still land. Files pasted over a
 * selection replace it as a markdown paste does (`paste.ts`): it stays,
 * drawn as going, and goes in the edit that places the first file; if none
 * uploads, or someone changed its words meanwhile, it stays. Over selected table cells, every one of them is
 * emptied in that edit and the files land in the first. Undo while files
 * upload calls off those still to land (`callOffUpload`): the placeholder
 * goes, a selection they would have replaced stays, and nothing lands.
 *
 * Files arrive on a paste only when they are what was copied. Office copies
 * a picture of the range beside its HTML; the HTML is the content then, and
 * the editor pastes it, a picture in the range included (`filesAreTheContent`).
 */

import { Extension } from '@tiptap/core';
import type { Node as PmNode, Schema } from '@tiptap/pm/model';
import { Plugin, PluginKey, type EditorState, type Selection } from '@tiptap/pm/state';
import { CellSelection } from '@tiptap/pm/tables';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';
import { toast } from 'svelte-sonner';
import { uploadMedia, type MediaFile } from '$lib/api/client';
import { decoded } from '$lib/utils/urlUtils';
import {
	anchorAt,
	goingDecorations,
	heldPos,
	heldSelection,
	holdRangeThrough,
	holdSelection,
	holdThrough,
	holdsAsTaken,
	landApart,
	nextWait,
	type Anchor,
	type Held,
	type HeldRange,
} from './anchor';
import type { MediaKind } from './media-kind';
import { insertBlockAt } from './place';

export type Upload = (file: File) => Promise<Pick<MediaFile, 'url' | 'filename' | 'mime_type'>>;

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

/** Files uploading together, under one placeholder. */
interface Pending {
	id: string;
	/** When it started waiting, among uploads and pastes (`nextWait`). */
	order: number;
	/** The names of those still to land, in order. */
	names: string[];
	/** Where the next block goes. */
	at: Held;
	/** The selection the files replace: it goes when the first of them lands. */
	replaces?: HeldRange;
}

type UploadMeta =
	| { add: Pending }
	| { remove: string }
	/** A file failed; `names` are those still to land. */
	| { failed: { id: string; names: string[] } }
	/** A block landed: the next goes at `pos`, in the document the meta's transaction makes; `names` are those still to land. */
	| { landed: { id: string; pos: number; names: string[] } }
	/** The place taken again as an anchor, once the document it is in is the page's. */
	| { anchor: { id: string; at: Anchor } };

interface Uploads {
	/** The placeholders and the selections going, drawn where they are now. */
	set: DecorationSet;
	pending: Pending[];
}

const uploadsKey = new PluginKey<Uploads>('mediaUploads');

function placeholderLabel(names: string[]): string {
	return names.length === 1 ? `Uploading ${names[0]}…` : `Uploading ${names.length} files…`;
}

function placeholder(p: Pending): HTMLElement {
	const el = document.createElement('div');
	el.className = 'doc-upload';
	el.setAttribute('contenteditable', 'false');
	const spinner = document.createElement('span');
	spinner.className = 'doc-upload-spinner';
	const label = document.createElement('span');
	label.textContent = placeholderLabel(p.names);
	el.append(spinner, label);
	return el;
}

/** What a pending upload draws: its placeholder, and the selection it replaces as going. */
function decorations(doc: PmNode, p: Pending): Decoration[] {
	const out: Decoration[] = [];
	if (p.at.pos !== null) {
		out.push(
			Decoration.widget(Math.min(p.at.pos, doc.content.size), () => placeholder(p), {
				uploadId: p.id,
				key: `${p.id}:${p.names.length}`,
				side: 1,
			}),
		);
	}
	out.push(...goingDecorations(doc, p.replaces));
	return out;
}

function pendingOf(view: EditorView, id: string): Pending | undefined {
	return uploadsKey.getState(view.state)?.pending.find((p) => p.id === id);
}

/** Where the files under `id` land next; null once the text that place was in is gone. */
function placeholderPos(view: EditorView, id: string): number | null {
	const pending = pendingOf(view, id);
	return pending ? heldPos(view.state, pending.at) : null;
}

/** Uploads waiting: none means nothing is uploading. */
export function pendingUploads(view: EditorView): number {
	return uploadsKey.getState(view.state)?.pending.length ?? 0;
}

/** The uploads waiting to land, as Undo weighs them against pastes. */
export function waitingUploads(state: EditorState): { id: string; order: number }[] {
	return (uploadsKey.getState(state)?.pending ?? []).map(({ id, order }) => ({ id, order }));
}

/**
 * Call off the files under `id` still to land, as Undo does while they
 * upload: the placeholder goes, the selection they would have replaced
 * stays, and none of them lands when its upload finishes. An upload waiting
 * is no edit yet, so y-undo would pass over it to the person's own writing,
 * and its landing, a new edit, would then clear the redo that could bring
 * that writing back.
 */
export function callOffUpload(view: EditorView, id: string): void {
	const pending = pendingOf(view, id);
	if (!pending) return;
	view.dispatch(view.state.tr.setMeta(uploadsKey, { remove: id }));
	toast(pending.names.length === 1 ? `Undo stopped uploading ${pending.names[0]}` : `Undo stopped uploading ${pending.names.length} files`);
}

let nextUpload = 0;

type Uploaded = { ok: true; media: Awaited<ReturnType<Upload>> } | { ok: false; error: unknown };

/**
 * Upload `files` and put their blocks where `at` is by the time each is
 * done, wherever edits made meanwhile have moved it: a position in the page
 * as it is now, or an anchor taken earlier (the insert menu's, held while
 * the file dialog was open). The files upload together and land in the
 * order given, each after the one before. If the place is gone by then
 * (someone deleted the text it was in), they land at the caret, and the
 * person is told. With `replacing`, that selection stays, drawn as going,
 * and goes in the edit that places the first file; if none uploads, it
 * stays. Each landing is an undo step of its own. Resolves when every
 * upload has settled.
 */
export async function uploadFiles(
	view: EditorView,
	files: File[],
	at: number | Anchor,
	upload: Upload = uploadMedia,
	replacing?: Selection,
): Promise<void> {
	if (!files.length) return;
	const id = `upload-${++nextUpload}`;
	const start = typeof at === 'number' ? at : at.resolve();
	const place: Held = {
		at: typeof at === 'number' ? anchorAt(view.state, at) : at,
		pos: start === null ? null : Math.min(start, view.state.doc.content.size),
	};
	const replaces = replacing && holdSelection(view.state, replacing);
	view.dispatch(
		view.state.tr.setMeta(uploadsKey, { add: { id, order: nextWait(), names: files.map((f) => f.name), at: place, replaces } }),
	);
	const results = files.map((file) =>
		upload(file).then(
			(media): Uploaded => ({ ok: true, media }),
			(error): Uploaded => ({ ok: false, error }),
		),
	);
	let toldMoved = false;
	for (let i = 0; i < files.length; i++) {
		const file = files[i];
		const result = await results[i];
		// Undo called the rest off while they uploaded.
		if (view.isDestroyed || !pendingOf(view, id)) return;
		const names = files.slice(i + 1).map((f) => f.name);
		if (!result.ok) {
			toast.error(`Couldn't upload ${file.name}`, {
				description: result.error instanceof Error ? result.error.message : String(result.error),
			});
			if (names.length) view.dispatch(view.state.tr.setMeta(uploadsKey, { failed: { id, names } }));
			continue;
		}
		const { media } = result;
		const kind = kindOfMime(file.type || media.mime_type);
		const node = mediaNode(view.state.schema, kind, media.url, file.name || media.filename);
		if (!node) continue;
		const tr = view.state.tr;
		let pos = placeholderPos(view, id);
		const going = pendingOf(view, id)?.replaces;
		const replaced = going ? heldSelection(view.state, going) : null;
		if (going && replaced && holdsAsTaken(going, replaced)) {
			const first = Math.min(...replaced.ranges.map((r) => r.$from.pos));
			replaced.replace(tr);
			pos = tr.mapping.map(first, -1);
			// An emptied cell holds an empty line, and the file takes its place.
			const after = tr.doc.resolve(pos).nodeAfter;
			if (replaced instanceof CellSelection && after?.isTextblock && !after.content.size) pos += 1;
		} else if (replaced) {
			toast('Added beside your selection', {
				description: 'Someone changed the words you selected while the files uploaded, so they stay.',
			});
		}
		if (pos === null) {
			pos = view.state.selection.from;
			if (!toldMoved) {
				toldMoved = true;
				toast('Added at the cursor', {
					description: 'Someone deleted the text you added the files to while they uploaded.',
				});
			}
		}
		const end = insertBlockAt(tr, pos, node);
		tr.setMeta(uploadsKey, names.length ? { landed: { id, pos: end, names } } : { remove: id });
		landApart(view, tr);
		// The next block's place, held through other devices' edits from here.
		const next = names.length ? placeholderPos(view, id) : null;
		if (next !== null) view.dispatch(view.state.tr.setMeta(uploadsKey, { anchor: { id, at: anchorAt(view.state, next) } }));
	}
	if (!view.isDestroyed && pendingIds(view).includes(id)) {
		view.dispatch(view.state.tr.setMeta(uploadsKey, { remove: id }));
	}
}

function pendingIds(view: EditorView): string[] {
	return (uploadsKey.getState(view.state)?.pending ?? []).map((p) => p.id);
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

export function mediaUploads(upload: Upload = uploadMedia): Extension {
	return Extension.create({
		name: 'pageMediaUploads',
		addProseMirrorPlugins() {
			return [
				new Plugin<Uploads>({
					key: uploadsKey,
					state: {
						init: () => ({ set: DecorationSet.empty, pending: [] }),
						apply(tr, prev) {
							const meta = tr.getMeta(uploadsKey) as UploadMeta | undefined;
							if (!meta && !prev.pending.length) return prev;
							let pending: Pending[] = prev.pending.map((p) => ({
								...p,
								at: holdThrough(tr, p.at, 1),
								replaces: p.replaces && holdRangeThrough(tr, p.replaces),
							}));
							if (meta && 'add' in meta) pending = [...pending, meta.add];
							if (meta && 'remove' in meta) pending = pending.filter((p) => p.id !== meta.remove);
							if (meta && 'failed' in meta) {
								pending = pending.map((p) => (p.id === meta.failed.id ? { ...p, names: meta.failed.names } : p));
							}
							if (meta && 'landed' in meta) {
								const { id, pos, names } = meta.landed;
								pending = pending.map((p) =>
									p.id === id ? { ...p, names, at: { at: p.at.at, pos }, replaces: undefined } : p,
								);
							}
							if (meta && 'anchor' in meta) {
								pending = pending.map((p) => (p.id === meta.anchor.id ? { ...p, at: { ...p.at, at: meta.anchor.at } } : p));
							}
							return { set: DecorationSet.create(tr.doc, pending.flatMap((p) => decorations(tr.doc, p))), pending };
						},
					},
					props: {
						decorations: (state) => uploadsKey.getState(state)?.set,
						handlePaste(view, event) {
							const files = filesOf(event.clipboardData);
							if (!files.length || !filesAreTheContent(event.clipboardData)) return false;
							// A selection stays until the first file lands, and goes with it.
							// Selected cells all go, and the files land in the first of them.
							const { selection } = view.state;
							const first = Math.min(...selection.ranges.map((r) => r.$from.pos));
							void uploadFiles(view, files, first, upload, selection);
							return true;
						},
						handleDrop(view, event, _slice, moved) {
							if (moved) return false;
							const files = filesOf(event.dataTransfer);
							if (!files.length) return false;
							const at = view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
							event.preventDefault();
							void uploadFiles(view, files, at ?? view.state.selection.from, upload);
							return true;
						},
					},
				}),
			];
		},
	});
}
