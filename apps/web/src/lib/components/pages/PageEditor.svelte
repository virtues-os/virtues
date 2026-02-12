<script lang="ts">
	import { onMount, onDestroy } from "svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import EntityPicker, {
		type EntityResult,
	} from "$lib/components/EntityPicker.svelte";
	import SlashMenu from "$lib/components/SlashMenu.svelte";
	import SelectionToolbar from "$lib/components/SelectionToolbar.svelte";
	import TableToolbar from "$lib/components/TableToolbar.svelte";

	// ProseMirror imports
	import { EditorState, type Transaction } from "prosemirror-state";
	import { EditorView } from "prosemirror-view";
	import { keymap } from "prosemirror-keymap";
	import {
		baseKeymap,
		chainCommands,
		createParagraphNear,
		exitCode,
		liftEmptyBlock,
		newlineInCode,
		splitBlock,
		toggleMark,
	} from "prosemirror-commands";
	import { history, undo, redo } from "prosemirror-history";
	import { tableEditing, goToNextCell } from "prosemirror-tables";
	import { dropCursor } from "prosemirror-dropcursor";
	import { gapCursor } from "prosemirror-gapcursor";
	import {
		splitListItem,
		liftListItem,
		sinkListItem,
	} from "prosemirror-schema-list";

	// Our custom ProseMirror setup
	import { schema } from "$lib/prosemirror/schema";
	import { parseMarkdown } from "$lib/prosemirror/markdown";
	import { createNodeViews } from "$lib/prosemirror/node-views";
	import {
		createEntityPickerPlugin,
		insertEntity,
		closeEntityPicker,
		isEntityPickerActive,
		getCursorCoords,
		createDragHandlePlugin,
		setDragHandlesEnabled,
		createSlashMenuPlugin,
		getSlashCommands,
		filterSlashCommands,
		executeSlashCommand,
		closeSlashMenu,
		getSlashMenuCoords,
		isSlashMenuActive,
		createPlaceholderPlugin,
		createFormattingInputRules,
		createSelectionToolbarPlugin,
		getActiveMarks,
		createTableToolbarPlugin,
		executeTableCommand,
		createMediaPastePlugin,
		createCodeHighlightPlugin,
		type EntitySelection,
		type SlashCommand,
		type SelectionToolbarPosition,
		type TableToolbarPosition,
		type TableCommand,
	} from "$lib/prosemirror/plugins";
	import { uploadMedia } from "$lib/api/client";

	// Yjs integration
	import type { YjsDocument } from "$lib/yjs";

	// Import ProseMirror theme
	import "$lib/prosemirror/theme.css";

	export interface DocStats {
		wordCount: number;
		charCount: number;
		linkCount: number;
		mediaCount: number;
	}

	interface Props {
		/** Initial markdown content (read-only, used for non-Yjs init and Yjs empty-doc init) */
		initialContent?: string;
		/** Called when the document changes with computed stats */
		onDocChange?: (stats: DocStats) => void;
		placeholder?: string;
		/** Optional Yjs document for real-time collaboration */
		yjsDoc?: YjsDocument;
		/** Whether the editor is disabled/loading (useful for waiting for Yjs sync) */
		disabled?: boolean;
		/** Whether connected to the sync server (for status display) */
		isConnected?: boolean;
		/** Whether synced with the server (for status display) */
		isSynced?: boolean;
		/** Whether drag handles are enabled */
		showDragHandles?: boolean;
		/** Page ID for filtering AI edit events */
		pageId?: string;
	}

	let {
		initialContent = "",
		onDocChange,
		placeholder: placeholderText,
		yjsDoc,
		disabled = false,
		isConnected = true,
		isSynced = true,
		showDragHandles = true,
		pageId,
	}: Props = $props();

	let editorContainer: HTMLDivElement;
	let view: EditorView | null = null;
	let isExternalUpdate = false;

	// Entity picker state
	let showEntityPicker = $state(false);
	let entityPickerPos = $state({ x: 0, y: 0 });
	let entityQuery = $state("");

	// Slash menu state
	let showSlashMenu = $state(false);
	let slashMenuPos = $state({ x: 0, y: 0 });
	let slashQuery = $state("");

	// Selection toolbar state
	let showSelectionToolbar = $state(false);
	let selectionToolbarPos = $state<SelectionToolbarPosition>({ x: 0, y: 0 });

	// Link picker state (EntityPicker replaces LinkPopover)
	let showLinkPicker = $state(false);
	let linkPickerPos = $state({ x: 0, y: 0 });
	let linkPickerInitialQuery = $state('');
	let linkPickerHasExistingLink = $state(false);

	// Table toolbar state
	let showTableToolbar = $state(false);
	let tableToolbarPos = $state<TableToolbarPosition>({ x: 0, y: 0 });

	function openEntityPicker(
		coords: { left: number; top: number; bottom: number },
		query: string,
	) {
		const pickerWidth = 300;
		const pickerHeight = 340;
		const padding = 8;

		let x = coords.left;
		let y = coords.bottom + 6;

		// Keep within horizontal bounds
		if (x + pickerWidth > window.innerWidth - padding) {
			x = window.innerWidth - pickerWidth - padding;
		}
		if (x < padding) {
			x = padding;
		}

		// Keep within vertical bounds
		if (y + pickerHeight > window.innerHeight - padding) {
			y = coords.top - pickerHeight - 6;
		}

		entityPickerPos = { x, y };
		entityQuery = query;
		showEntityPicker = true;
		// Close selection toolbar when entity picker opens
		showSelectionToolbar = false;
	}

	function closeEntityPickerUI() {
		showEntityPicker = false;
		entityQuery = "";
		view?.focus();
	}

	function openSlashMenu(
		coords: { left: number; top: number; bottom: number },
		query: string,
	) {
		const menuWidth = 280;
		const menuHeight = 400;
		const padding = 8;

		let x = coords.left;
		let y = coords.bottom + 6;

		// Keep within horizontal bounds
		if (x + menuWidth > window.innerWidth - padding) {
			x = window.innerWidth - menuWidth - padding;
		}
		if (x < padding) {
			x = padding;
		}

		// Keep within vertical bounds
		if (y + menuHeight > window.innerHeight - padding) {
			y = coords.top - menuHeight - 6;
		}

		slashMenuPos = { x, y };
		slashQuery = query;
		showSlashMenu = true;
		// Close selection toolbar when slash menu opens
		showSelectionToolbar = false;
	}

	function closeSlashMenuUI() {
		showSlashMenu = false;
		slashQuery = "";
		view?.focus();
	}

	function handleSlashSelect(command: SlashCommand) {
		if (!view) return;
		executeSlashCommand(view, command);
		closeSlashMenuUI();
	}

	// Selection toolbar functions
	function openSelectionToolbar(position: SelectionToolbarPosition) {
		// Don't show if other floating UI is open
		if (showEntityPicker || showSlashMenu) return;
		selectionToolbarPos = position;
		showSelectionToolbar = true;
	}

	function closeSelectionToolbar() {
		showSelectionToolbar = false;
	}

	function handleFormat(
		mark: "strong" | "em" | "underline" | "code" | "strikethrough" | "link",
	) {
		if (!view) return;

		// Link opens EntityPicker for entity search or URL input
		if (mark === "link") {
			// Check if selection already has a link to get URL for editing
			const { from, to } = view.state.selection;
			let currentUrl: string | undefined;
			view.state.doc.nodesBetween(from, to, (node) => {
				const linkMark = node.marks.find(
					(m) => m.type === schema.marks.link,
				);
				if (linkMark) {
					currentUrl = linkMark.attrs.href;
					return false; // Stop iteration
				}
			});

			linkPickerInitialQuery = currentUrl ?? '';
			linkPickerHasExistingLink = !!currentUrl;
			linkPickerPos = { ...selectionToolbarPos };
			showSelectionToolbar = false;
			showLinkPicker = true;
			return;
		}

		toggleMark(schema.marks[mark])(view.state, view.dispatch);
		view.focus();
	}

	function handleLinkSubmit(url: string) {
		if (!view) return;

		const { from, to, empty } = view.state.selection;
		if (empty) return;

		// Remove any existing link marks first, then add the new one
		const tr = view.state.tr;
		tr.removeMark(from, to, schema.marks.link);
		tr.addMark(from, to, schema.marks.link.create({ href: url }));
		view.dispatch(tr);
		view.focus();
	}

	function handleLinkPickerSelect(entity: EntityResult) {
		handleLinkSubmit(entity.url);
		closeLinkPicker();
	}

	function handleLinkRemove() {
		if (!view) return;

		const { from, to } = view.state.selection;
		const tr = view.state.tr;
		tr.removeMark(from, to, schema.marks.link);
		view.dispatch(tr);
		closeLinkPicker();
	}

	function closeLinkPicker() {
		showLinkPicker = false;
		linkPickerInitialQuery = '';
		linkPickerHasExistingLink = false;
		view?.focus();
	}

	// Table toolbar functions
	function openTableToolbar(position: TableToolbarPosition) {
		// Don't show if other floating UI is open
		if (showEntityPicker || showSlashMenu || showLinkPicker) return;
		tableToolbarPos = position;
		showTableToolbar = true;
	}

	function closeTableToolbar() {
		showTableToolbar = false;
	}

	function handleTableCommand(command: TableCommand) {
		if (!view) return;
		executeTableCommand(view, command);
		view.focus();
	}

	// Active marks computed when toolbar shows (updated via selectionToolbarPos changes)
	const activeMarks = $derived.by(() => {
		// This re-runs when selectionToolbarPos changes (which happens on selection change)
		void selectionToolbarPos;
		if (!view)
			return {
				strong: false,
				em: false,
				underline: false,
				code: false,
				strikethrough: false,
				link: false,
			};
		return getActiveMarks(view.state);
	});

	// Get filtered slash commands
	const filteredSlashCommands = $derived(
		filterSlashCommands(getSlashCommands(), slashQuery),
	);

	function isMediaFile(
		name: string,
		mimeType?: string,
	): { isMedia: boolean; type: "image" | "audio" | "video" | null } {
		const ext = name.split(".").pop()?.toLowerCase() || "";

		const imageExts = [
			"jpg",
			"jpeg",
			"png",
			"gif",
			"webp",
			"svg",
			"bmp",
			"ico",
		];
		const audioExts = ["mp3", "wav", "m4a", "ogg", "flac", "aac", "wma"];
		const videoExts = ["mp4", "mov", "webm", "avi", "mkv", "m4v", "wmv"];

		if (imageExts.includes(ext)) return { isMedia: true, type: "image" };
		if (audioExts.includes(ext)) return { isMedia: true, type: "audio" };
		if (videoExts.includes(ext)) return { isMedia: true, type: "video" };

		if (mimeType) {
			if (mimeType.startsWith("image/"))
				return { isMedia: true, type: "image" };
			if (mimeType.startsWith("audio/"))
				return { isMedia: true, type: "audio" };
			if (mimeType.startsWith("video/"))
				return { isMedia: true, type: "video" };
		}

		return { isMedia: false, type: null };
	}

	function handleEntitySelect(entity: EntityResult) {
		if (!view) return;

		const selection: EntitySelection = {
			href: entity.url,
			label: entity.name,
		};

		insertEntity(view, selection);
		closeEntityPickerUI();
	}

	// Handle navigation from entity links
	function handleNavigation(
		e: CustomEvent<{ href: string; entityId?: string }>,
	) {
		e.preventDefault();
		e.stopPropagation();

		spaceStore.openTabFromRoute(e.detail.href, {
			forceNew: true,
			preferEmptyPane: true,
		});
	}

	// =============================================================================
	// SLASH COMMAND MEDIA HANDLERS
	// =============================================================================

	/**
	 * Creates a hidden file input and triggers file selection
	 */
	function createFileInput(accept: string, onFile: (file: File) => void) {
		const input = document.createElement("input");
		input.type = "file";
		input.accept = accept;
		input.style.display = "none";
		input.onchange = () => {
			const file = input.files?.[0];
			if (file) {
				onFile(file);
			}
			input.remove();
		};
		document.body.appendChild(input);
		input.click();
	}

	/**
	 * Upload file and insert media node at cursor position
	 */
	async function uploadAndInsertMedia(
		file: File,
		type: "image" | "video" | "audio",
	) {
		if (!view) return;

		try {
			const result = await uploadMedia(file);

			// Create the appropriate node based on type
			let node: ReturnType<typeof schema.nodes.image.create> | undefined;
			if (type === "image") {
				node = schema.nodes.image.create({
					src: result.url,
					alt: result.filename,
				});
			} else if (type === "video") {
				node = schema.nodes.video_player.create({
					src: result.url,
					name: result.filename,
				});
			} else if (type === "audio") {
				node = schema.nodes.audio_player.create({
					src: result.url,
					name: result.filename,
				});
			}

			if (node) {
				const tr = view.state.tr.replaceSelectionWith(node);
				view.dispatch(tr);
				view.focus();
			}
		} catch (error) {
			console.error(`Failed to upload ${type}:`, error);
		}
	}

	function handleSlashCommandImage(e: Event) {
		e.preventDefault();
		createFileInput("image/*", (file) =>
			uploadAndInsertMedia(file, "image"),
		);
	}

	function handleSlashCommandVideo(e: Event) {
		e.preventDefault();
		createFileInput("video/*", (file) =>
			uploadAndInsertMedia(file, "video"),
		);
	}

	function handleSlashCommandAudio(e: Event) {
		e.preventDefault();
		createFileInput("audio/*", (file) =>
			uploadAndInsertMedia(file, "audio"),
		);
	}

	// =============================================================================
	onMount(() => {
		// Build plugins
		const plugins = [
			// Yjs plugins if available, otherwise local history
			...(yjsDoc?.plugins ?? [history()]),

			// Input rules for markdown-style formatting (must come before keymaps)
			createFormattingInputRules(),

			// Keymaps
			keymap({
				"Mod-z": undo,
				"Mod-y": redo,
				"Mod-Shift-z": redo,
				Tab: goToNextCell(1),
				"Shift-Tab": goToNextCell(-1),
			}),
			// List keybindings - Enter must chain with default behavior
			keymap({
				"Shift-Enter": chainCommands(
					exitCode,
					(state, dispatch) => {
						if (dispatch)
							dispatch(
								state.tr
									.replaceSelectionWith(
										schema.nodes.hard_break.create(),
									)
									.scrollIntoView(),
							);
						return true;
					},
				),
				Enter: chainCommands(
					newlineInCode,
					splitListItem(schema.nodes.list_item),
					createParagraphNear,
					liftEmptyBlock,
					splitBlock,
				),
				Tab: sinkListItem(schema.nodes.list_item),
				"Shift-Tab": liftListItem(schema.nodes.list_item),
			}),
			// Formatting keybindings
			keymap({
				"Mod-b": toggleMark(schema.marks.strong),
				"Mod-i": toggleMark(schema.marks.em),
				"Mod-u": toggleMark(schema.marks.underline),
				"Mod-e": toggleMark(schema.marks.code),
				"Mod-`": toggleMark(schema.marks.code),
				"Mod-Shift-s": toggleMark(schema.marks.strikethrough),
				"Mod-Shift-x": toggleMark(schema.marks.strikethrough),
			}),
			keymap(baseKeymap),

			// Table editing
			tableEditing(),

			// Drop cursor and gap cursor
			dropCursor(),
			gapCursor(),

			// Entity picker plugin
			createEntityPickerPlugin({
				onOpen: (_coords, query) => {
					// Get coords from the view after state update (same pattern as slash menu)
					// The coords from plugin state are placeholders (0,0,0) since state.apply() doesn't have view
					setTimeout(() => {
						if (view) {
							const coords = getCursorCoords(view);
							if (coords) {
								openEntityPicker(coords, query);
							}
						}
					}, 0);
				},
				onClose: () => {
					closeEntityPickerUI();
				},
				onQueryChange: (query) => {
					entityQuery = query;
				},
			}),

			// Slash menu plugin
			createSlashMenuPlugin({
				onOpen: () => {
					// Get coords from the view after state update
					setTimeout(() => {
						if (view) {
							const coords = getSlashMenuCoords(view);
							if (coords) {
								openSlashMenu(coords, "");
							}
						}
					}, 0);
				},
				onClose: () => {
					closeSlashMenuUI();
				},
				onQueryChange: (query) => {
					slashQuery = query;
				},
			}),

			// Drag handle plugin
			createDragHandlePlugin({ enabled: showDragHandles }),

			// Selection toolbar plugin
			createSelectionToolbarPlugin({
				onShow: (position) => openSelectionToolbar(position),
				onHide: () => closeSelectionToolbar(),
				debounceMs: 50,
			}),

			// Table toolbar plugin
			createTableToolbarPlugin({
				onShow: (position) => openTableToolbar(position),
				onHide: () => closeTableToolbar(),
				debounceMs: 100,
			}),

			// Media paste/drop plugin
			createMediaPastePlugin({
				uploadFn: async (file, onProgress) => {
					const result = await uploadMedia(file, onProgress);
					return { url: result.url, filename: result.filename };
				},
			}),

			// Syntax highlighting for code blocks
			createCodeHighlightPlugin(),

			// Placeholder plugin
			createPlaceholderPlugin(),
		];

		// Create editor state (y-prosemirror provides the document from XmlFragment)
		const state = EditorState.create({
			schema,
			plugins,
		});

		// Create editor view
		view = new EditorView(editorContainer, {
			state,
			nodeViews: createNodeViews(),
			editable: () => !disabled,
			handleDOMEvents: {
				click: (view, event) => {
					const target = event.target as HTMLElement;
					const entityLink = target.closest(".pm-entity-link");
					if (entityLink) {
						event.preventDefault();
						const href = entityLink.getAttribute("href");
						if (href) {
							spaceStore.openTabFromRoute(href, {
								forceNew: true,
								preferEmptyPane: true,
							});
						}
						return true; // Handled
					}
					return false; // Let ProseMirror handle it
				},
			},
			dispatchTransaction: (tr: Transaction) => {
				if (!view || !view.dom?.parentNode) return;

				const newState = view.state.apply(tr);
				view.updateState(newState);

				// Compute doc stats directly from ProseMirror (no markdown serialization)
				if (tr.docChanged && !isExternalUpdate) {
					const text = newState.doc.textContent;
					const wordCount = text.trim()
						? text.trim().split(/\s+/).length
						: 0;
					const charCount = text.length;
					let linkCount = 0;
					let mediaCount = 0;
					newState.doc.descendants((node) => {
						if (node.type.name === "entity_link") linkCount++;
						if (node.type.name === "media" || node.type.name === "image")
							mediaCount++;
						// Count link marks at the block level to avoid double-counting
						// (a link spanning bold+normal text creates multiple text nodes)
						if (node.isTextblock) {
							let inLink = false;
							node.forEach((child) => {
								const hasLink =
									child.marks?.some((m) => m.type.name === "link") ??
									false;
								if (hasLink && !inLink) linkCount++;
								inLink = hasLink;
							});
						}
					});
					onDocChange?.({ wordCount, charCount, linkCount, mediaCount });
				}
			},
		});

		// For Yjs mode: if XmlFragment is empty but we have initial content, initialize.
		// This is a fallback — the server now initializes Yjs from markdown in DocCache.
		// This path handles the edge case where the server hasn't initialized yet.
		if (yjsDoc && view) {
			let initialized = false;
			const unsubscribe = yjsDoc.isSynced.subscribe((synced) => {
				if (initialized) return;
				if (
					synced &&
					view &&
					yjsDoc.yxmlFragment.length === 0 &&
					initialContent &&
					initialContent.trim()
				) {
					initialized = true;
					const parsedDoc = parseMarkdown(initialContent);
					if (parsedDoc) {
						isExternalUpdate = true;
						try {
							const tr = view.state.tr.replaceWith(
								0,
								view.state.doc.content.size,
								parsedDoc.content,
							);
							view.dispatch(tr);
						} finally {
							isExternalUpdate = false;
						}
					}
					setTimeout(() => unsubscribe(), 0);
				} else if (synced) {
					initialized = true;
					setTimeout(() => unsubscribe(), 0);
				}
			});
		}

		// Listen for navigation events from node views
		editorContainer.addEventListener(
			"page-navigate",
			handleNavigation as EventListener,
		);

		// Listen for slash command media events
		editorContainer.addEventListener(
			"slash-command-image",
			handleSlashCommandImage,
		);
		editorContainer.addEventListener(
			"slash-command-video",
			handleSlashCommandVideo,
		);
		editorContainer.addEventListener(
			"slash-command-audio",
			handleSlashCommandAudio,
		);
	});

	// Note: Non-Yjs external content sync was removed. All pages use Yjs now.
	// The initialContent prop is only used for first-load initialization.

	// Update editable state when disabled changes
	$effect(() => {
		if (view && disabled !== undefined) {
			// Force a re-render to update editable state
			view.setProps({ editable: () => !disabled });
		}
	});

	// Update drag handle enabled state when showDragHandles changes
	$effect(() => {
		if (view && showDragHandles !== undefined) {
			setDragHandlesEnabled(view, showDragHandles);
		}
	});

	onDestroy(() => {
		// Null out view first to prevent async y-prosemirror callbacks
		// from dispatching transactions during teardown
		const v = view;
		view = null;

		editorContainer?.removeEventListener(
			"page-navigate",
			handleNavigation as EventListener,
		);
		editorContainer?.removeEventListener(
			"slash-command-image",
			handleSlashCommandImage,
		);
		editorContainer?.removeEventListener(
			"slash-command-video",
			handleSlashCommandVideo,
		);
		editorContainer?.removeEventListener(
			"slash-command-audio",
			handleSlashCommandAudio,
		);
		v?.destroy();
	});
</script>

<div class="page-editor-wrapper">
	<!-- Connection/sync status is shown in the page footer status bar -->

	<div
		class="page-editor ProseMirror"
		class:disabled
		bind:this={editorContainer}
	></div>

	{#if showEntityPicker}
		<EntityPicker
			mode="single"
			position={entityPickerPos}
			placeholder="Search pages, people, places..."
			onSelect={handleEntitySelect}
			onClose={closeEntityPickerUI}
		/>
	{/if}

	{#if showSlashMenu}
		<SlashMenu
			query={slashQuery}
			commands={filteredSlashCommands}
			position={slashMenuPos}
			onSelect={handleSlashSelect}
			onClose={closeSlashMenuUI}
		/>
	{/if}

	{#if showSelectionToolbar}
		<SelectionToolbar
			position={selectionToolbarPos}
			{activeMarks}
			onFormat={handleFormat}
			onClose={closeSelectionToolbar}
		/>
	{/if}

	{#if showLinkPicker}
		<EntityPicker
			mode="single"
			position={linkPickerPos}
			placeholder="Search or paste a URL..."
			initialQuery={linkPickerInitialQuery}
			onSelect={handleLinkPickerSelect}
			onClose={closeLinkPicker}
			footerAction={linkPickerHasExistingLink ? {
				label: 'Remove Link',
				icon: 'ri:link-unlink',
				action: handleLinkRemove,
				variant: 'destructive'
			} : undefined}
		/>
	{/if}

	{#if showTableToolbar}
		<TableToolbar
			position={tableToolbarPos}
			onCommand={handleTableCommand}
			onClose={closeTableToolbar}
		/>
	{/if}
</div>

<style>
	.page-editor-wrapper {
		position: relative;
	}

	.page-editor {
		min-height: 300px;
	}

	.page-editor.disabled {
		opacity: 0.6;
		pointer-events: none;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}
</style>
