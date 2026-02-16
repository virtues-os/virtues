/**
 * File Paste/Drop Extension
 *
 * Handles pasting and dropping any file into the editor.
 * All file types are accepted — media files (image/video/audio) get rich
 * inline previews, other files get a file card widget.
 * Shows upload placeholders while uploading, then inserts markdown.
 */

import type { Extension } from '@codemirror/state';
import { EditorView } from '@codemirror/view';

export type UploadFn = (file: File, onProgress?: (pct: number) => void) => Promise<string>;

/**
 * Create the file paste/drop extension.
 */
export function createMediaPaste(uploadFn: UploadFn): Extension {
	return EditorView.domEventHandlers({
		paste(event, view) {
			const items = event.clipboardData?.items;
			if (!items) return false;

			const files: File[] = [];
			for (const item of items) {
				if (item.kind === 'file') {
					const file = item.getAsFile();
					if (file) files.push(file);
				}
			}

			if (files.length === 0) return false;
			event.preventDefault();

			const pos = view.state.selection.main.head;
			for (const file of files) {
				handleUpload(view, file, pos, uploadFn);
			}
			return true;
		},

		drop(event, view) {
			const files = event.dataTransfer?.files;
			if (!files || files.length === 0) return false;

			const fileList: File[] = [];
			for (const file of files) {
				fileList.push(file);
			}

			if (fileList.length === 0) return false;
			event.preventDefault();

			const coords = { x: event.clientX, y: event.clientY };
			const pos = view.posAtCoords(coords) ?? view.state.selection.main.head;

			for (const file of fileList) {
				handleUpload(view, file, pos, uploadFn);
			}
			return true;
		},
	});
}

function handleUpload(view: EditorView, file: File, pos: number, uploadFn: UploadFn) {
	const placeholder = `![Uploading ${file.name}...]()\n`;

	// Insert placeholder
	view.dispatch({
		changes: { from: pos, insert: placeholder },
	});

	uploadFn(file)
		.then((url) => {
			// Find and replace the placeholder with the actual markdown
			const doc = view.state.doc.toString();
			const placeholderIdx = doc.indexOf(placeholder);
			if (placeholderIdx === -1) return;

			// All files use ![name](url) — media-widgets.ts handles rendering
			// based on file extension (image/audio/video get previews, rest get file card)
			const markdown = `![${file.name}](${url})\n`;

			view.dispatch({
				changes: {
					from: placeholderIdx,
					to: placeholderIdx + placeholder.length,
					insert: markdown,
				},
			});
		})
		.catch((err) => {
			// Remove placeholder on error
			const doc = view.state.doc.toString();
			const placeholderIdx = doc.indexOf(placeholder);
			if (placeholderIdx !== -1) {
				view.dispatch({
					changes: {
						from: placeholderIdx,
						to: placeholderIdx + placeholder.length,
						insert: '',
					},
				});
			}
			console.error('Upload failed:', err);
		});
}
