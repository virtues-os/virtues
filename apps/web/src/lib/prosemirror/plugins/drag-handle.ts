/**
 * Line Numbers Plugin for ProseMirror
 *
 * Provides line numbers in a left gutter:
 * - Gutter is positioned absolutely within the editor wrapper
 * - Line numbers use DOM offsetTop (relative positioning, no viewport math)
 * - Draggable for block reordering
 * - Togglable via toolbar
 */

import { Plugin, PluginKey } from 'prosemirror-state';
import type { EditorView } from 'prosemirror-view';

// =============================================================================
// PLUGIN KEY
// =============================================================================

export const dragHandleKey = new PluginKey<DragHandleState>('lineNumbers');

// =============================================================================
// TYPES
// =============================================================================

interface DragHandleState {
	enabled: boolean;
}

interface DragHandlePluginOptions {
	enabled?: boolean;
}

// =============================================================================
// CONSTANTS
// =============================================================================

const DRAG_MIME_TYPE = 'application/x-pm-block-drag';

// Shared elements (reused across drags)
let dragGhost: HTMLDivElement | null = null;
let dropIndicator: HTMLDivElement | null = null;

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

function createDragGhost(lineNum: number): HTMLDivElement {
	if (!dragGhost) {
		dragGhost = document.createElement('div');
		dragGhost.className = 'pm-drag-ghost';
		document.body.appendChild(dragGhost);
	}
	dragGhost.textContent = `Line ${lineNum}`;
	return dragGhost;
}

function hideDragGhost(): void {
	if (dragGhost) {
		dragGhost.style.transform = 'translate(-9999px, -9999px)';
	}
}

function createDropIndicator(): HTMLDivElement {
	if (!dropIndicator) {
		dropIndicator = document.createElement('div');
		dropIndicator.className = 'pm-drop-indicator';
		document.body.appendChild(dropIndicator);
	}
	return dropIndicator;
}

function showDropIndicator(wrapper: HTMLElement, y: number): void {
	const indicator = createDropIndicator();
	const rect = wrapper.getBoundingClientRect();
	indicator.style.left = `${rect.left}px`;
	indicator.style.width = `${rect.width}px`;
	indicator.style.top = `${y - 1}px`;
	indicator.classList.add('visible');
}

function hideDropIndicator(): void {
	if (dropIndicator) {
		dropIndicator.classList.remove('visible');
	}
}

function createGutterElement(): HTMLDivElement {
	const gutter = document.createElement('div');
	gutter.className = 'pm-line-gutter';
	return gutter;
}

function findDropPosition(
	view: EditorView,
	clientY: number
): { targetPos: number; indicatorY: number } | null {
	const { doc } = view.state;
	let targetPos = 0;
	let indicatorY = 0;
	let foundTarget = false;

	doc.forEach((node, offset) => {
		if (foundTarget || !node.isBlock) return;

		const blockStart = offset;
		const blockEnd = offset + node.nodeSize;

		const coords = view.coordsAtPos(blockStart);
		const endCoords = view.coordsAtPos(blockEnd);
		const blockMiddle = (coords.top + endCoords.top) / 2;

		if (clientY < blockMiddle) {
			targetPos = blockStart;
			indicatorY = coords.top;
			foundTarget = true;
		} else {
			targetPos = blockEnd;
			indicatorY = endCoords.top;
		}
	});

	return { targetPos, indicatorY };
}

function executeMove(view: EditorView, fromPos: number, targetPos: number): boolean {
	const doc = view.state.doc;
	const fromNode = doc.nodeAt(fromPos);
	if (!fromNode) return false;

	const nodeSize = fromNode.nodeSize;
	const fromEnd = fromPos + nodeSize;

	// Don't move if dropping on itself
	if (targetPos >= fromPos && targetPos <= fromEnd) {
		return false;
	}

	const tr = view.state.tr;

	if (targetPos > fromPos) {
		tr.delete(fromPos, fromEnd);
		tr.insert(targetPos - nodeSize, fromNode);
	} else {
		tr.delete(fromPos, fromEnd);
		tr.insert(targetPos, fromNode);
	}

	view.dispatch(tr);
	return true;
}

function updateGutter(
	gutter: HTMLDivElement,
	view: EditorView,
	enabled: boolean,
	wrapper: HTMLElement | null
): void {
	gutter.innerHTML = '';

	if (!enabled) {
		gutter.style.width = '0';
		return;
	}

	gutter.style.width = '';

	if (!wrapper) return;

	const { doc } = view.state;
	const wrapperRect = wrapper.getBoundingClientRect();
	let lineNum = 0;

	doc.forEach((node, offset) => {
		if (node.isBlock) {
			// Skip horizontal_rule - it's not a "line" of content
			if (node.type.name === 'horizontal_rule') {
				return;
			}

			lineNum++;

			const coords = view.coordsAtPos(offset);
			const topOffset = coords.top - wrapperRect.top;

			const lineEl = document.createElement('div');
			lineEl.className = 'pm-line-number';
			lineEl.draggable = true;
			lineEl.textContent = String(lineNum);
			lineEl.style.top = `${topOffset}px`;

			const blockPos = offset;

			lineEl.addEventListener('dragstart', (e) => {
				if (!e.dataTransfer) return;
				e.dataTransfer.setData(DRAG_MIME_TYPE, String(blockPos));
				e.dataTransfer.effectAllowed = 'move';
				lineEl.classList.add('dragging');

				const ghost = createDragGhost(lineNum);
				e.dataTransfer.setDragImage(ghost, 0, 0);
			});

			lineEl.addEventListener('dragend', () => {
				lineEl.classList.remove('dragging');
				hideDragGhost();
				hideDropIndicator();
			});

			gutter.appendChild(lineEl);
		}
	});
}

// =============================================================================
// PLUGIN
// =============================================================================

export function createDragHandlePlugin(options: DragHandlePluginOptions = {}) {
	let gutter: HTMLDivElement | null = null;
	let enabled = options.enabled ?? true;
	let currentView: EditorView | null = null;

	return new Plugin<DragHandleState>({
		key: dragHandleKey,

		state: {
			init() {
				return { enabled };
			},
			apply(tr, pluginState) {
				const meta = tr.getMeta(dragHandleKey);
				if (meta?.enabled !== undefined) {
					enabled = meta.enabled;
					return { enabled: meta.enabled };
				}
				return pluginState;
			},
		},

		view(editorView) {
			currentView = editorView;
			gutter = createGutterElement();
			const wrapper = editorView.dom.parentElement;

			if (wrapper) {
				wrapper.classList.add('has-line-gutter');
				wrapper.insertBefore(gutter, editorView.dom);

				// Add drag handlers to the wrapper (covers both gutter and editor)
				wrapper.addEventListener('dragover', handleWrapperDragOver);
				wrapper.addEventListener('drop', handleWrapperDrop);
				wrapper.addEventListener('dragleave', handleWrapperDragLeave);
			}

			updateGutter(gutter, editorView, enabled, wrapper);

			function handleWrapperDragOver(event: DragEvent) {
				if (!event.dataTransfer?.types.includes(DRAG_MIME_TYPE)) return;
				if (!currentView || !wrapper) return;

				event.preventDefault();
				event.dataTransfer.dropEffect = 'move';

				const dropPos = findDropPosition(currentView, event.clientY);
				if (dropPos) {
					showDropIndicator(wrapper, dropPos.indicatorY);
				}
			}

			function handleWrapperDrop(event: DragEvent) {
				const blockPosStr = event.dataTransfer?.getData(DRAG_MIME_TYPE);
				if (!blockPosStr || !currentView) return;

				event.preventDefault();
				hideDropIndicator();

				const fromPos = parseInt(blockPosStr, 10);
				if (Number.isNaN(fromPos)) return;

				const dropPos = findDropPosition(currentView, event.clientY);
				if (dropPos) {
					executeMove(currentView, fromPos, dropPos.targetPos);
				}
			}

			function handleWrapperDragLeave(event: DragEvent) {
				// Only hide if leaving the wrapper entirely
				if (!wrapper?.contains(event.relatedTarget as Node)) {
					hideDropIndicator();
				}
			}

			return {
				update(view) {
					currentView = view;
					const state = dragHandleKey.getState(view.state);
					enabled = state?.enabled ?? true;
					if (gutter) {
						updateGutter(gutter, view, enabled, wrapper);
					}
				},
				destroy() {
					if (wrapper) {
						wrapper.removeEventListener('dragover', handleWrapperDragOver);
						wrapper.removeEventListener('drop', handleWrapperDrop);
						wrapper.removeEventListener('dragleave', handleWrapperDragLeave);
						wrapper.classList.remove('has-line-gutter');
					}
					gutter?.remove();
					gutter = null;
					currentView = null;
				},
			};
		},

		props: {
			handleDOMEvents: {
				dragover(_view, event) {
					// Let wrapper handler take care of it
					if (event.dataTransfer?.types.includes(DRAG_MIME_TYPE)) {
						event.preventDefault();
						event.dataTransfer.dropEffect = 'move';
						return true;
					}
					return false;
				},

				drop(_view, event) {
					// Let wrapper handler take care of it
					if (event.dataTransfer?.types.includes(DRAG_MIME_TYPE)) {
						event.preventDefault();
						return true;
					}
					return false;
				},
			},
		},
	});
}

// =============================================================================
// COMMANDS
// =============================================================================

export function setDragHandlesEnabled(view: EditorView, enabled: boolean): void {
	view.dispatch(view.state.tr.setMeta(dragHandleKey, { enabled }));
}

export function isDragHandlesEnabled(view: EditorView): boolean {
	const state = dragHandleKey.getState(view.state);
	return state?.enabled ?? true;
}
