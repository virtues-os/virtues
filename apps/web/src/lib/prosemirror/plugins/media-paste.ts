/**
 * Media Paste/Drop Plugin for ProseMirror
 *
 * Handles:
 * - Pasting images from clipboard (screenshots, copied images)
 * - Dragging and dropping image files from desktop
 * - Shows upload progress with decorations
 *
 * Files are uploaded to the /api/media endpoint which uses content-addressed
 * storage for automatic deduplication.
 */

import { Plugin, PluginKey } from 'prosemirror-state';
import type { EditorState, Transaction } from 'prosemirror-state';
import type { EditorView } from 'prosemirror-view';
import { Decoration, DecorationSet } from 'prosemirror-view';
import { schema } from '../schema';

// =============================================================================
// PLUGIN KEY
// =============================================================================

export const mediaPasteKey = new PluginKey<MediaPasteState>('mediaPaste');

// =============================================================================
// TYPES
// =============================================================================

export interface MediaPasteState {
	/** Currently uploading files: uploadId -> position */
	uploads: Map<string, UploadInfo>;
}

export interface UploadInfo {
	/** Position in document where placeholder is shown */
	pos: number;
	/** Upload progress (0-100) */
	progress: number;
	/** Error message if upload failed */
	error: string | null;
	/** Original filename */
	filename: string;
}

export type UploadFunction = (
	file: File,
	onProgress?: (percent: number) => void
) => Promise<{ url: string; filename: string }>;

// =============================================================================
// PLUGIN STATE
// =============================================================================

const initialState: MediaPasteState = {
	uploads: new Map(),
};

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

/** Generate a unique upload ID */
function generateUploadId(): string {
	return `upload_${Date.now()}_${Math.random().toString(36).slice(2, 9)}`;
}

/** Check if a file is an image */
function isImageFile(file: File): boolean {
	return file.type.startsWith('image/');
}

/** Check if a file is supported media (image, video, audio) */
function isSupportedMedia(file: File): boolean {
	return (
		file.type.startsWith('image/') ||
		file.type.startsWith('video/') ||
		file.type.startsWith('audio/')
	);
}

/** Get clipboard items that are images */
function getClipboardImages(clipboardData: DataTransfer | null): File[] {
	if (!clipboardData) return [];

	const files: File[] = [];
	for (const item of clipboardData.items) {
		if (item.type.startsWith('image/')) {
			const file = item.getAsFile();
			if (file) files.push(file);
		}
	}
	return files;
}

/** Get dropped files that are supported media */
function getDroppedMedia(dataTransfer: DataTransfer | null): File[] {
	if (!dataTransfer) return [];

	const files: File[] = [];
	for (const file of dataTransfer.files) {
		if (isSupportedMedia(file)) {
			files.push(file);
		}
	}
	return files;
}

// =============================================================================
// UPLOAD HANDLING
// =============================================================================

/**
 * Handle image upload - creates placeholder, uploads, replaces with node
 */
async function handleImageUpload(
	view: EditorView,
	file: File,
	uploadFn: UploadFunction,
	pos: number
) {
	const uploadId = generateUploadId();

	// Add upload to state
	const tr = view.state.tr.setMeta(mediaPasteKey, {
		type: 'start',
		uploadId,
		pos,
		filename: file.name,
	});
	view.dispatch(tr);

	try {
		// Upload with progress tracking
		const result = await uploadFn(file, (progress) => {
			// Update progress in state
			const progressTr = view.state.tr.setMeta(mediaPasteKey, {
				type: 'progress',
				uploadId,
				progress,
			});
			view.dispatch(progressTr);
		});

		// Remove upload from state
		const completeTr = view.state.tr.setMeta(mediaPasteKey, {
			type: 'complete',
			uploadId,
		});
		view.dispatch(completeTr);

		// Get current upload info to find position
		const state = mediaPasteKey.getState(view.state);
		const info = state?.uploads.get(uploadId);
		if (!info) return;

		// Insert image node at the position
		const imageNode = schema.nodes.image.create({
			src: result.url,
			alt: file.name,
			title: file.name,
		});

		// Insert the image
		const insertTr = view.state.tr.insert(info.pos, imageNode);
		view.dispatch(insertTr);
	} catch (error) {
		// Mark upload as failed
		const errorMessage = error instanceof Error ? error.message : 'Upload failed';
		const errorTr = view.state.tr.setMeta(mediaPasteKey, {
			type: 'error',
			uploadId,
			error: errorMessage,
		});
		view.dispatch(errorTr);

		// Auto-remove error after 3 seconds
		setTimeout(() => {
			const removeTr = view.state.tr.setMeta(mediaPasteKey, {
				type: 'complete',
				uploadId,
			});
			view.dispatch(removeTr);
		}, 3000);
	}
}

// =============================================================================
// PLUGIN
// =============================================================================

export interface MediaPastePluginOptions {
	/** Function to upload a file and return its URL */
	uploadFn: UploadFunction;
}

export function createMediaPastePlugin(options: MediaPastePluginOptions) {
	const { uploadFn } = options;

	return new Plugin<MediaPasteState>({
		key: mediaPasteKey,

		state: {
			init() {
				return initialState;
			},

			apply(tr, state) {
				const meta = tr.getMeta(mediaPasteKey);
				if (!meta) return state;

				const uploads = new Map(state.uploads);

				switch (meta.type) {
					case 'start':
						uploads.set(meta.uploadId, {
							pos: meta.pos,
							progress: 0,
							error: null,
							filename: meta.filename,
						});
						break;

					case 'progress': {
						const info = uploads.get(meta.uploadId);
						if (info) {
							uploads.set(meta.uploadId, {
								...info,
								progress: meta.progress,
							});
						}
						break;
					}

					case 'error': {
						const info = uploads.get(meta.uploadId);
						if (info) {
							uploads.set(meta.uploadId, {
								...info,
								error: meta.error,
							});
						}
						break;
					}

					case 'complete':
						uploads.delete(meta.uploadId);
						break;
				}

				return { uploads };
			},
		},

		props: {
			handlePaste(view, event) {
				const images = getClipboardImages(event.clipboardData);
				if (images.length === 0) return false;

				event.preventDefault();

				// Upload each image
				const pos = view.state.selection.from;
				for (const file of images) {
					handleImageUpload(view, file, uploadFn, pos);
				}

				return true;
			},

			handleDrop(view, event) {
				const media = getDroppedMedia(event.dataTransfer);
				if (media.length === 0) return false;

				event.preventDefault();

				// Get drop position
				const pos = view.posAtCoords({
					left: event.clientX,
					top: event.clientY,
				});
				if (!pos) return false;

				// Upload each file
				for (const file of media) {
					handleImageUpload(view, file, uploadFn, pos.pos);
				}

				return true;
			},

			decorations(state) {
				const pluginState = mediaPasteKey.getState(state);
				if (!pluginState || pluginState.uploads.size === 0) {
					return DecorationSet.empty;
				}

				const decorations: Decoration[] = [];

				for (const [uploadId, info] of pluginState.uploads) {
					// Create a widget decoration showing upload progress
					const widget = Decoration.widget(
						info.pos,
						() => {
							const wrapper = document.createElement('span');
							wrapper.className = 'pm-upload-placeholder';
							wrapper.dataset.uploadId = uploadId;

							if (info.error) {
								wrapper.innerHTML = `
									<span class="pm-upload-error">
										<span class="pm-upload-error-icon">⚠️</span>
										<span class="pm-upload-error-text">${info.error}</span>
									</span>
								`;
							} else {
								wrapper.innerHTML = `
									<span class="pm-upload-progress">
										<span class="pm-upload-spinner"></span>
										<span class="pm-upload-text">${info.filename}</span>
										<span class="pm-upload-percent">${info.progress}%</span>
									</span>
								`;
							}

							return wrapper;
						},
						{ side: 0 }
					);

					decorations.push(widget);
				}

				return DecorationSet.create(state.doc, decorations);
			},
		},
	});
}

// =============================================================================
// EXPORTS
// =============================================================================

export { isImageFile, isSupportedMedia };
