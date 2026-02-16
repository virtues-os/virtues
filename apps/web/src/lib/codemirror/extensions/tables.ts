/**
 * Table Decorations
 *
 * Renders GFM markdown tables as editable HTML <table> elements with
 * Obsidian-style interactive controls:
 * - Click into any cell to edit directly
 * - Hover: + strips on bottom/right to add rows/columns
 * - Hover: drag handles on top (columns) and left (rows) to reorder
 * - Right-click any cell for delete row / delete column
 * - Drag across cells to select a range (Cmd+C to copy)
 * - Cmd+B / Cmd+I / Cmd+U / Cmd+E for inline formatting in cells
 * - Tab/Shift+Tab navigate cells, Enter moves down, Escape exits
 *
 * Uses StateField (not ViewPlugin) because multi-line replace decorations
 * require direct decoration provision via EditorView.decorations facet.
 *
 * Requires GFM extension enabled in the Lezer markdown parser.
 */

import { syntaxTree } from '@codemirror/language';
import { type EditorState, type Extension, type Range, StateField } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, WidgetType } from '@codemirror/view';
import { contextMenu } from '$lib/stores/contextMenu.svelte';

type Alignment = 'left' | 'center' | 'right';

function parseCells(line: string): string[] {
	let trimmed = line.trim();
	if (trimmed.startsWith('|')) trimmed = trimmed.slice(1);
	if (trimmed.endsWith('|')) trimmed = trimmed.slice(0, -1);
	return trimmed.split('|').map(c => c.trim());
}

function parseAlignment(cell: string): Alignment | null {
	const t = cell.trim();
	if (!/^:?-+:?$/.test(t)) return null;
	if (t.startsWith(':') && t.endsWith(':')) return 'center';
	if (t.endsWith(':')) return 'right';
	return 'left';
}

function parseTable(text: string): {
	headers: string[];
	alignments: (Alignment | null)[];
	rows: string[][];
} | null {
	const lines = text.split('\n').filter(l => l.trim());
	if (lines.length < 2) return null;

	const headers = parseCells(lines[0]);
	const delimCells = parseCells(lines[1]);

	const validDelim = delimCells.every(c => /^:?-+:?$/.test(c.trim()));
	if (!validDelim) return null;

	const alignments = delimCells.map(parseAlignment);
	const rows = lines.slice(2).map(parseCells);
	return { headers, alignments, rows };
}

function serializeTable(headers: string[], alignments: (Alignment | null)[], rows: string[][]): string {
	const headerLine = '| ' + headers.join(' | ') + ' |';
	const delimLine = '| ' + alignments.map(a => {
		if (a === 'center') return ':---:';
		if (a === 'right') return '---:';
		return '---';
	}).join(' | ') + ' |';
	const dataLines = rows.map(row => '| ' + row.join(' | ') + ' |');
	return [headerLine, delimLine, ...dataLines].join('\n');
}

function highlightColumn(table: HTMLTableElement, colIdx: number, className: string) {
	table.querySelectorAll('tr').forEach(tr => {
		const cells = tr.children;
		if (cells[colIdx]) cells[colIdx].classList.add(className);
	});
}

function clearColumnHighlight(table: HTMLTableElement, className: string) {
	table.querySelectorAll(`.${className}`).forEach(el => el.classList.remove(className));
}

function reorderColumn(
	table: HTMLTableElement,
	alignments: (Alignment | null)[],
	fromIdx: number,
	toIdx: number,
) {
	const [alignment] = alignments.splice(fromIdx, 1);
	alignments.splice(toIdx, 0, alignment);

	table.querySelectorAll('tr').forEach(tr => {
		const cells = Array.from(tr.children);
		const movedCell = cells[fromIdx];
		tr.removeChild(movedCell);
		if (toIdx >= tr.children.length) {
			tr.appendChild(movedCell);
		} else {
			tr.insertBefore(movedCell, tr.children[toIdx]);
		}
	});
}

/** Get cell coordinates (rowIndex, colIndex) from a cell element. Returns header as row -1. */
function getCellCoords(table: HTMLTableElement, cell: HTMLElement): { row: number; col: number } | null {
	const tr = cell.closest('tr');
	if (!tr) return null;
	const col = Array.from(tr.children).indexOf(cell);
	const isHeader = !!cell.closest('thead');
	if (isHeader) return { row: -1, col };
	const tbody = table.querySelector('tbody');
	if (!tbody) return null;
	const row = Array.from(tbody.children).indexOf(tr);
	return { row, col };
}

class TableWidget extends WidgetType {
	constructor(
		private headers: string[],
		private alignments: (Alignment | null)[],
		private rows: string[][],
	) {
		super();
	}

	toDOM(view: EditorView) {
		const wrapper = document.createElement('div');
		wrapper.className = 'cm-table-wrapper';

		const table = document.createElement('table');
		table.className = 'cm-table-widget';
		let numCols = this.headers.length;
		const alignments = [...this.alignments];

		// --- Range selection state ---
		let rangeStart: { row: number; col: number } | null = null;
		let rangeEnd: { row: number; col: number } | null = null;
		let isDraggingRange = false;

		const clearRangeSelection = () => {
			table.querySelectorAll('.cm-cell-selected').forEach(el => el.classList.remove('cm-cell-selected'));
			rangeStart = null;
			rangeEnd = null;
		};

		const highlightRange = (start: { row: number; col: number }, end: { row: number; col: number }) => {
			table.querySelectorAll('.cm-cell-selected').forEach(el => el.classList.remove('cm-cell-selected'));
			const minRow = Math.min(start.row, end.row);
			const maxRow = Math.max(start.row, end.row);
			const minCol = Math.min(start.col, end.col);
			const maxCol = Math.max(start.col, end.col);

			const allTrs = Array.from(table.querySelectorAll('tbody tr'));
			for (let r = minRow; r <= maxRow; r++) {
				if (r < 0 || r >= allTrs.length) continue;
				const cells = allTrs[r].children;
				for (let c = minCol; c <= maxCol; c++) {
					if (c < cells.length) cells[c].classList.add('cm-cell-selected');
				}
			}
			// Include header row if range includes row -1
			if (minRow <= -1) {
				const headerCells = table.querySelectorAll('thead th');
				for (let c = minCol; c <= maxCol; c++) {
					if (c < headerCells.length) headerCells[c].classList.add('cm-cell-selected');
				}
			}
		};

		const copyRangeSelection = () => {
			if (!rangeStart || !rangeEnd) return;
			const minRow = Math.min(rangeStart.row, rangeEnd.row);
			const maxRow = Math.max(rangeStart.row, rangeEnd.row);
			const minCol = Math.min(rangeStart.col, rangeEnd.col);
			const maxCol = Math.max(rangeStart.col, rangeEnd.col);

			const lines: string[] = [];
			// Header if included
			if (minRow <= -1) {
				const headerCells = Array.from(table.querySelectorAll('thead th'));
				lines.push(headerCells.slice(minCol, maxCol + 1).map(c => c.textContent || '').join('\t'));
			}
			const allTrs = Array.from(table.querySelectorAll('tbody tr'));
			const startR = Math.max(0, minRow);
			for (let r = startR; r <= maxRow && r < allTrs.length; r++) {
				const cells = Array.from(allTrs[r].children);
				lines.push(cells.slice(minCol, maxCol + 1).map(c => c.textContent || '').join('\t'));
			}
			navigator.clipboard.writeText(lines.join('\n'));
		};

		const selectCellContent = (cell: HTMLElement) => {
			const range = document.createRange();
			range.selectNodeContents(cell);
			const sel = window.getSelection();
			sel?.removeAllRanges();
			sel?.addRange(range);
		};

		const syncToDocument = () => {
			const newHeaders: string[] = [];
			table.querySelectorAll('thead th').forEach(th => newHeaders.push(th.textContent || ''));

			const newRows: string[][] = [];
			table.querySelectorAll('tbody tr').forEach(tr => {
				const row: string[] = [];
				tr.querySelectorAll('td').forEach(td => row.push(td.textContent || ''));
				newRows.push(row);
			});

			const newMarkdown = serializeTable(newHeaders, alignments, newRows);

			const pos = view.posAtDOM(wrapper);
			let tableFrom = -1;
			let tableTo = -1;
			syntaxTree(view.state).iterate({
				enter(node) {
					if (node.name === 'Table' && pos >= node.from && pos <= node.to + 1) {
						tableFrom = node.from;
						tableTo = node.to;
					}
				}
			});

			if (tableFrom >= 0 && tableTo >= 0) {
				const oldText = view.state.sliceDoc(tableFrom, tableTo);
				if (oldText !== newMarkdown) {
					view.dispatch({
						changes: { from: tableFrom, to: tableTo, insert: newMarkdown }
					});
				}
			}
		};

		/** Apply inline markdown formatting to selected text in a cell */
		const applyFormatting = (cell: HTMLElement, wrapper: string) => {
			const sel = window.getSelection();
			if (!sel || sel.rangeCount === 0) return;
			const range = sel.getRangeAt(0);
			if (!cell.contains(range.startContainer)) return;

			const text = range.toString();
			if (!text) return;

			// Check if already wrapped — remove if so
			const fullText = cell.textContent || '';
			const start = range.startOffset;
			const end = range.endOffset;
			const before = fullText.slice(Math.max(0, start - wrapper.length), start);
			const after = fullText.slice(end, end + wrapper.length);

			if (before === wrapper && after === wrapper) {
				// Unwrap
				cell.textContent = fullText.slice(0, start - wrapper.length) + text + fullText.slice(end + wrapper.length);
				// Restore selection
				const newRange = document.createRange();
				const textNode = cell.firstChild;
				if (textNode) {
					newRange.setStart(textNode, start - wrapper.length);
					newRange.setEnd(textNode, start - wrapper.length + text.length);
					sel.removeAllRanges();
					sel.addRange(newRange);
				}
			} else {
				// Wrap
				cell.textContent = fullText.slice(0, start) + wrapper + text + wrapper + fullText.slice(end);
				const newRange = document.createRange();
				const textNode = cell.firstChild;
				if (textNode) {
					newRange.setStart(textNode, start + wrapper.length);
					newRange.setEnd(textNode, start + wrapper.length + text.length);
					sel.removeAllRanges();
					sel.addRange(newRange);
				}
			}
		};

		const makeCell = (tag: 'th' | 'td', text: string, colIdx: number): HTMLElement => {
			const cell = document.createElement(tag);
			cell.contentEditable = 'true';
			cell.textContent = text;
			const align = alignments[colIdx];
			if (align) cell.style.textAlign = align;

			cell.addEventListener('keydown', (e) => {
				const meta = e.metaKey || e.ctrlKey;

				// Formatting shortcuts
				if (meta && e.key === 'b') {
					e.preventDefault();
					applyFormatting(cell, '**');
					return;
				}
				if (meta && e.key === 'i') {
					e.preventDefault();
					applyFormatting(cell, '*');
					return;
				}
				if (meta && e.key === 'u') {
					e.preventDefault();
					// Underline uses HTML tags in this codebase
					const sel = window.getSelection();
					if (sel && sel.rangeCount > 0) {
						const range = sel.getRangeAt(0);
						const text = range.toString();
						if (text) {
							const fullText = cell.textContent || '';
							const start = range.startOffset;
							const end = range.endOffset;
							if (fullText.slice(start - 3, start) === '<u>' && fullText.slice(end, end + 4) === '</u>') {
								cell.textContent = fullText.slice(0, start - 3) + text + fullText.slice(end + 4);
							} else {
								cell.textContent = fullText.slice(0, start) + '<u>' + text + '</u>' + fullText.slice(end);
							}
						}
					}
					return;
				}
				if (meta && e.key === 'e') {
					e.preventDefault();
					applyFormatting(cell, '`');
					return;
				}

				// Copy range selection
				if (meta && e.key === 'c' && rangeStart) {
					e.preventDefault();
					copyRangeSelection();
					return;
				}

				if (e.key === 'Tab') {
					e.preventDefault();
					const cells = Array.from(table.querySelectorAll('[contenteditable]')) as HTMLElement[];
					const idx = cells.indexOf(cell);
					const nextIdx = e.shiftKey ? idx - 1 : idx + 1;

					if (nextIdx >= 0 && nextIdx < cells.length) {
						cells[nextIdx].focus();
						selectCellContent(cells[nextIdx]);
					} else if (!e.shiftKey && nextIdx >= cells.length) {
						addRow(table.querySelectorAll('tbody tr').length);
					} else if (e.shiftKey && nextIdx < 0) {
						syncToDocument();
						view.focus();
					}
				} else if (e.key === 'Escape') {
					e.preventDefault();
					if (rangeStart) {
						clearRangeSelection();
					} else {
						syncToDocument();
						view.focus();
					}
				} else if (e.key === 'Enter' && !e.shiftKey) {
					e.preventDefault();
					const cells = Array.from(table.querySelectorAll('[contenteditable]')) as HTMLElement[];
					const idx = cells.indexOf(cell);
					const belowIdx = idx + numCols;
					if (belowIdx < cells.length) {
						cells[belowIdx].focus();
						selectCellContent(cells[belowIdx]);
					}
				}
			});

			// --- Range selection: mousedown starts tracking ---
			cell.addEventListener('mousedown', (e) => {
				// Only left button, not right-click
				if (e.button !== 0) return;
				clearRangeSelection();
				const coords = getCellCoords(table, cell);
				if (!coords) return;
				rangeStart = coords;
				isDraggingRange = false;
			});

			return cell;
		};

		// --- Range selection: mousemove over cells while dragging ---
		table.addEventListener('mousemove', (e) => {
			if (!rangeStart || e.buttons !== 1) return;
			const target = (e.target as HTMLElement).closest('th, td') as HTMLElement | null;
			if (!target || !table.contains(target)) return;
			const coords = getCellCoords(table, target);
			if (!coords) return;

			// If moved to a different cell, enter range mode
			if (coords.row !== rangeStart.row || coords.col !== rangeStart.col) {
				if (!isDraggingRange) {
					isDraggingRange = true;
					// Blur active cell to exit edit mode
					if (document.activeElement instanceof HTMLElement && table.contains(document.activeElement)) {
						document.activeElement.blur();
					}
					window.getSelection()?.removeAllRanges();
				}
				rangeEnd = coords;
				highlightRange(rangeStart, rangeEnd);
			}
		});

		table.addEventListener('mouseup', () => {
			if (isDraggingRange) {
				isDraggingRange = false;
			}
		});

		// Global keydown for Cmd+C when range is selected but no cell focused
		wrapper.addEventListener('keydown', (e) => {
			if ((e.metaKey || e.ctrlKey) && e.key === 'c' && rangeStart && rangeEnd) {
				e.preventDefault();
				copyRangeSelection();
			}
		});

		// --- Delete row/column ---
		const deleteRow = (rowIdx: number) => {
			const tbody = table.querySelector('tbody');
			if (!tbody) return;
			const rows = Array.from(tbody.children);
			if (rowIdx < 0 || rowIdx >= rows.length) return;
			if (rows.length <= 1) return; // Keep at least one row
			tbody.removeChild(rows[rowIdx]);
			syncToDocument();
			positionControls();
		};

		const deleteColumn = (colIdx: number) => {
			if (numCols <= 1) return; // Keep at least one column
			table.querySelectorAll('tr').forEach(tr => {
				const cells = Array.from(tr.children);
				if (cells[colIdx]) tr.removeChild(cells[colIdx]);
			});
			alignments.splice(colIdx, 1);
			numCols--;
			syncToDocument();
			positionControls();
		};

		// --- Context menu (uses global store for consistent UI) ---
		const clearDeletePreview = () => {
			table.querySelectorAll('.cm-delete-preview').forEach(el => el.classList.remove('cm-delete-preview'));
		};

		const highlightRow = (rowIdx: number) => {
			const bodyRows = Array.from(table.querySelectorAll('tbody tr'));
			const tr = bodyRows[rowIdx] as HTMLElement | undefined;
			if (tr) tr.querySelectorAll('th, td').forEach(c => c.classList.add('cm-delete-preview'));
		};

		const highlightCol = (colIdx: number) => {
			highlightColumn(table, colIdx, 'cm-delete-preview');
		};

		table.addEventListener('contextmenu', (e) => {
			const target = (e.target as HTMLElement).closest('th, td') as HTMLElement | null;
			if (!target) return;
			e.preventDefault();

			const coords = getCellCoords(table, target);
			if (!coords) return;

			const isHeader = coords.row === -1;
			const items = [];

			if (!isHeader) {
				items.push({
					id: 'delete-row',
					label: 'Delete row',
					icon: 'ri:delete-row',
					variant: 'destructive' as const,
					action: () => deleteRow(coords.row),
					onMouseEnter: () => highlightRow(coords.row),
					onMouseLeave: clearDeletePreview,
				});
			}

			items.push({
				id: 'delete-col',
				label: 'Delete column',
				icon: 'ri:delete-column',
				variant: 'destructive' as const,
				action: () => deleteColumn(coords.col),
				onMouseEnter: () => highlightCol(coords.col),
				onMouseLeave: clearDeletePreview,
			});

			contextMenu.show({ x: e.clientX, y: e.clientY }, items);
		});

		const addRow = (atIndex: number) => {
			const tbody = table.querySelector('tbody');
			if (!tbody) return;
			const rows = Array.from(tbody.children) as HTMLElement[];
			const tr = document.createElement('tr');
			for (let i = 0; i < numCols; i++) {
				tr.appendChild(makeCell('td', '', i));
			}
			if (atIndex >= rows.length) {
				tbody.appendChild(tr);
			} else {
				tbody.insertBefore(tr, rows[atIndex]);
			}
			syncToDocument();
			positionControls();
			const firstCell = tr.querySelector('td') as HTMLElement;
			if (firstCell) firstCell.focus();
		};

		const addColumn = (atIndex: number) => {
			const headerRow = table.querySelector('thead tr');
			if (!headerRow) return;
			const headerCells = Array.from(headerRow.children) as HTMLElement[];
			const newTh = makeCell('th', '', atIndex);
			if (atIndex >= headerCells.length) {
				headerRow.appendChild(newTh);
			} else {
				headerRow.insertBefore(newTh, headerCells[atIndex]);
			}

			alignments.splice(atIndex, 0, 'left');

			table.querySelectorAll('tbody tr').forEach(tr => {
				const cells = Array.from(tr.children) as HTMLElement[];
				const newTd = makeCell('td', '', atIndex);
				if (atIndex >= cells.length) {
					tr.appendChild(newTd);
				} else {
					tr.insertBefore(newTd, cells[atIndex]);
				}
			});

			numCols++;
			syncToDocument();
			positionControls();
			newTh.focus();
		};

		// Sync when focus leaves the table entirely
		wrapper.addEventListener('focusout', (e) => {
			const related = (e as FocusEvent).relatedTarget as Node | null;
			if (related && wrapper.contains(related)) return;
			setTimeout(() => {
				if (!wrapper.contains(document.activeElement)) {
					syncToDocument();
				}
			}, 0);
		});

		// --- Build table structure ---
		const thead = document.createElement('thead');
		const headerRow = document.createElement('tr');
		for (let i = 0; i < numCols; i++) {
			headerRow.appendChild(makeCell('th', this.headers[i], i));
		}
		thead.appendChild(headerRow);
		table.appendChild(thead);

		const tbody = document.createElement('tbody');
		for (const row of this.rows) {
			const tr = document.createElement('tr');
			for (let i = 0; i < numCols; i++) {
				tr.appendChild(makeCell('td', row[i] || '', i));
			}
			tbody.appendChild(tr);
		}
		table.appendChild(tbody);

		wrapper.appendChild(table);

		// --- Interactive controls (editable mode only) ---
		const editable = view.state.facet(EditorView.editable);

		// Drag handles: top (columns) and left (rows)
		const colDragControls = document.createElement('div');
		colDragControls.className = 'cm-table-col-controls';
		const rowDragControls = document.createElement('div');
		rowDragControls.className = 'cm-table-row-controls';

		// Add strips: bottom (add row) and right (add column)
		const addRowStrip = document.createElement('div');
		addRowStrip.className = 'cm-table-add-row-strip';
		addRowStrip.addEventListener('click', () => {
			addRow(table.querySelectorAll('tbody tr').length);
		});

		const addColStrip = document.createElement('div');
		addColStrip.className = 'cm-table-add-col-strip';
		addColStrip.addEventListener('click', () => {
			addColumn(numCols);
		});

		const positionControls = () => {
			if (!editable) return;
			colDragControls.innerHTML = '';
			rowDragControls.innerHTML = '';

			const wrapperRect = wrapper.getBoundingClientRect();
			const ths = Array.from(table.querySelectorAll('thead th')) as HTMLElement[];
			const bodyRows = Array.from(table.querySelectorAll('tbody tr')) as HTMLElement[];

			// --- Column drag handles (top) ---
			for (let i = 0; i < ths.length; i++) {
				const thRect = ths[i].getBoundingClientRect();

				const handle = document.createElement('div');
				handle.className = 'cm-table-col-drag-handle';
				handle.style.left = `${thRect.left - wrapperRect.left}px`;
				handle.style.width = `${thRect.width}px`;
				const span = document.createElement('span');
				span.textContent = '⋮⋮';
				handle.appendChild(span);

				const colIdx = i;
				handle.addEventListener('mousedown', (e) => {
					e.preventDefault();
					let targetCol = colIdx;
					highlightColumn(table, colIdx, 'cm-col-dragging');

					const onMouseMove = (me: MouseEvent) => {
						clearColumnHighlight(table, 'cm-col-drop-target');
						for (let c = 0; c < ths.length; c++) {
							const rect = ths[c].getBoundingClientRect();
							if (me.clientX >= rect.left && me.clientX < rect.right) {
								targetCol = c;
								if (c !== colIdx) highlightColumn(table, c, 'cm-col-drop-target');
								break;
							}
						}
					};

					const onMouseUp = () => {
						document.removeEventListener('mousemove', onMouseMove);
						document.removeEventListener('mouseup', onMouseUp);
						clearColumnHighlight(table, 'cm-col-dragging');
						clearColumnHighlight(table, 'cm-col-drop-target');
						if (targetCol !== colIdx) {
							reorderColumn(table, alignments, colIdx, targetCol);
							syncToDocument();
							positionControls();
						}
					};

					document.addEventListener('mousemove', onMouseMove);
					document.addEventListener('mouseup', onMouseUp);
				});

				colDragControls.appendChild(handle);
			}

			// --- Row drag handles (left) ---
			for (let i = 0; i < bodyRows.length; i++) {
				const rowRect = bodyRows[i].getBoundingClientRect();

				const handle = document.createElement('div');
				handle.className = 'cm-table-row-drag-handle';
				handle.style.top = `${rowRect.top - wrapperRect.top}px`;
				handle.style.height = `${rowRect.height}px`;
				handle.textContent = '⋮⋮';

				const rowIdx = i;
				handle.addEventListener('mousedown', (e) => {
					e.preventDefault();
					let targetRow = rowIdx;
					const currentRows = Array.from(table.querySelectorAll('tbody tr')) as HTMLElement[];
					currentRows[rowIdx].classList.add('cm-row-dragging');

					const onMouseMove = (me: MouseEvent) => {
						currentRows.forEach((row, ri) => {
							row.classList.remove('cm-row-drop-target');
							const rect = row.getBoundingClientRect();
							if (me.clientY >= rect.top && me.clientY < rect.bottom) {
								targetRow = ri;
								if (ri !== rowIdx) row.classList.add('cm-row-drop-target');
							}
						});
					};

					const onMouseUp = () => {
						document.removeEventListener('mousemove', onMouseMove);
						document.removeEventListener('mouseup', onMouseUp);
						currentRows.forEach(row => {
							row.classList.remove('cm-row-dragging', 'cm-row-drop-target');
						});
						if (targetRow !== rowIdx) {
							const tbodyEl = table.querySelector('tbody');
							if (!tbodyEl) return;
							const rows = Array.from(tbodyEl.children);
							const draggedRow = rows[rowIdx];
							if (targetRow > rowIdx) {
								tbodyEl.insertBefore(draggedRow, rows[targetRow].nextSibling);
							} else {
								tbodyEl.insertBefore(draggedRow, rows[targetRow]);
							}
							syncToDocument();
							positionControls();
						}
					};

					document.addEventListener('mousemove', onMouseMove);
					document.addEventListener('mouseup', onMouseUp);
				});

				rowDragControls.appendChild(handle);
			}
		};

		if (editable) {
			wrapper.appendChild(colDragControls);
			wrapper.appendChild(rowDragControls);
			wrapper.appendChild(addRowStrip);
			wrapper.appendChild(addColStrip);

			requestAnimationFrame(() => positionControls());
		}

		return wrapper;
	}

	eq(other: TableWidget) {
		return (
			JSON.stringify(this.alignments) === JSON.stringify(other.alignments) &&
			this.headers.join('|') === other.headers.join('|') &&
			this.rows.map(r => r.join('|')).join('\n') === other.rows.map(r => r.join('|')).join('\n')
		);
	}

	ignoreEvent() {
		return true;
	}
}

function buildTableDecorations(state: EditorState): DecorationSet {
	const builder: Range<Decoration>[] = [];
	const cursorHead = state.selection.main.head;

	syntaxTree(state).iterate({
		enter(node) {
			if (node.name !== 'Table') return;

			const { from, to } = node;

			if (cursorHead >= from && cursorHead <= to) return;

			const text = state.sliceDoc(from, to);
			const parsed = parseTable(text);
			if (!parsed) return;

			builder.push(
				Decoration.replace({
					widget: new TableWidget(parsed.headers, parsed.alignments, parsed.rows),
					block: true,
				}).range(from, to)
			);
		},
	});

	return Decoration.set(builder, true);
}

const tableField = StateField.define<DecorationSet>({
	create(state) {
		return buildTableDecorations(state);
	},
	update(decos, tr) {
		if (tr.docChanged || tr.selection) {
			return buildTableDecorations(tr.state);
		}
		return decos;
	},
	provide: (field) => EditorView.decorations.from(field),
});

export const tables: Extension = tableField;
