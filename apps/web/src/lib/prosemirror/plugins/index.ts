/**
 * ProseMirror Plugins Index
 *
 * Exports all custom plugins for the page editor.
 */

export {
	createEntityPickerPlugin,
	entityPickerKey,
	insertEntity,
	closeEntityPicker,
	isEntityPickerActive,
	getEntityPickerState,
	getCursorCoords,
	type EntityPickerState,
	type EntitySelection,
	type EntityPickerPluginOptions,
} from './entity-picker';

export {
	createDragHandlePlugin,
	dragHandleKey,
	setDragHandlesEnabled,
	isDragHandlesEnabled,
} from './drag-handle';

export {
	createSlashMenuPlugin,
	slashMenuKey,
	getSlashCommands,
	filterSlashCommands,
	executeSlashCommand,
	closeSlashMenu,
	isSlashMenuActive,
	getSlashMenuState,
	getSlashMenuCoords,
	type SlashMenuState,
	type SlashCommand,
	type SlashMenuPluginOptions,
} from './slash-commands';

export { createPlaceholderPlugin, placeholderKey } from './placeholder';

export { createFormattingInputRules } from './input-rules';

export {
	createSelectionToolbarPlugin,
	selectionToolbarKey,
	isMarkActive,
	getActiveMarks,
	toggleFormat,
	getSelectionToolbarPosition,
	hideSelectionToolbar,
	isSelectionToolbarActive,
	type SelectionToolbarState,
	type SelectionToolbarPosition,
	type SelectionToolbarPluginOptions,
} from './selection-toolbar';

export {
	createTableToolbarPlugin,
	tableToolbarKey,
	executeTableCommand,
	canExecuteTableCommand,
	getTableToolbarPosition,
	hideTableToolbar,
	isTableToolbarActive,
	isCursorInTable,
	type TableToolbarState,
	type TableToolbarPosition,
	type TableToolbarPluginOptions,
	type TableCommand,
} from './table-toolbar';

export {
	createMediaPastePlugin,
	mediaPasteKey,
	isImageFile,
	isSupportedMedia,
	type MediaPasteState,
	type UploadInfo,
	type UploadFunction,
	type MediaPastePluginOptions,
} from './media-paste';

export { createCodeHighlightPlugin } from './code-highlight';
