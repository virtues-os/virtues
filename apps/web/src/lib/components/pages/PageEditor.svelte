<script lang="ts">
	import { onMount, onDestroy } from "svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import EntityPicker, {
		type EntityResult,
	} from "$lib/components/EntityPicker.svelte";
	import SlashMenu from "$lib/components/SlashMenu.svelte";
	import SelectionToolbar from "$lib/components/SelectionToolbar.svelte";
	import LinkPopover from "$lib/components/LinkPopover.svelte";
	import TableToolbar from "$lib/components/TableToolbar.svelte";

	// ProseMirror imports
	import { EditorState, type Transaction } from "prosemirror-state";
	import { EditorView } from "prosemirror-view";
	import { keymap } from "prosemirror-keymap";
	import {
		baseKeymap,
		chainCommands,
		createParagraphNear,
		liftEmptyBlock,
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
	import {
		parseMarkdown,
		serializeMarkdown,
	} from "$lib/prosemirror/markdown";
	import { createNodeViews } from "$lib/prosemirror/node-views";
	import {
		aiHighlightPlugin,
		addAIHighlight,
		removeAIHighlight,
		getAIHighlights,
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
		type EntitySelection,
		type SlashCommand,
		type SelectionToolbarPosition,
		type TableToolbarPosition,
		type TableCommand,
	} from "$lib/prosemirror/plugins";
	import { uploadMedia } from "$lib/api/client";
	import type {
		AIEditHighlightEvent,
		AIEditAcceptEvent,
		AIEditRejectEvent,
	} from "$lib/events/aiEdit";

	// Yjs integration
	import type { YjsDocument } from "$lib/yjs";

	// Import ProseMirror theme
	import "$lib/prosemirror/theme.css";

	interface Props {
		content: string;
		onSave?: (content: string) => void;
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
		content = $bindable(),
		onSave,
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

	// Link popover state
	let showLinkPopover = $state(false);
	let linkPopoverPos = $state<SelectionToolbarPosition>({ x: 0, y: 0 });
	let existingLinkUrl = $state<string | undefined>(undefined);

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

		// Link shows popover for URL input
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

			existingLinkUrl = currentUrl;
			linkPopoverPos = { ...selectionToolbarPos };
			showSelectionToolbar = false; // Hide toolbar when showing link popover
			showLinkPopover = true;
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
		showLinkPopover = false;
		existingLinkUrl = undefined;
	}

	function handleLinkRemove() {
		if (!view) return;

		const { from, to } = view.state.selection;
		const tr = view.state.tr;
		tr.removeMark(from, to, schema.marks.link);
		view.dispatch(tr);
		view.focus();
		showLinkPopover = false;
		existingLinkUrl = undefined;
	}

	function closeLinkPopover() {
		showLinkPopover = false;
		existingLinkUrl = undefined;
		view?.focus();
	}

	// Table toolbar functions
	function openTableToolbar(position: TableToolbarPosition) {
		// Don't show if other floating UI is open
		if (showEntityPicker || showSlashMenu || showLinkPopover) return;
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
	// AI EDIT EVENT HANDLERS
	// =============================================================================

	function handleAIEditHighlight(e: Event) {
		const event = e as CustomEvent<AIEditHighlightEvent>;
		if (!view || !pageId || event.detail.pageId !== pageId) return;

		const { editId, text } = event.detail;
		const currentView = view; // Capture for closure

		// Find the text in the document
		const doc = currentView.state.doc;
		let found = false;
		doc.descendants((node, pos) => {
			if (found || !node.isText) return;
			const nodeText = node.text ?? "";
			const idx = nodeText.indexOf(text);
			if (idx !== -1) {
				const from = pos + idx;
				const to = from + text.length;
				addAIHighlight(
					editId,
					from,
					to,
				)(currentView.state, currentView.dispatch);
				found = true;
				return false; // Stop iteration
			}
		});
	}

	function handleAIEditAccept(e: Event) {
		const event = e as CustomEvent<AIEditAcceptEvent>;
		if (!view || !pageId || event.detail.pageId !== pageId) return;
		removeAIHighlight(event.detail.editId)(view.state, view.dispatch);
	}

	function handleAIEditReject(e: Event) {
		const event = e as CustomEvent<AIEditRejectEvent>;
		if (!view || !pageId || event.detail.pageId !== pageId) return;
		// Remove the highlight - content revert was already done via API
		removeAIHighlight(event.detail.editId)(view.state, view.dispatch);
	}

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
				"Shift-Enter": (state, dispatch) => {
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
				Enter: chainCommands(
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

			// AI edit highlight plugin
			aiHighlightPlugin,

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

			// Placeholder plugin
			createPlaceholderPlugin(),
		];

		// Parse initial content for non-Yjs mode
		const initialDoc = yjsDoc
			? null // y-prosemirror will provide the document from XmlFragment
			: parseMarkdown(content);

		// Create editor state
		const state = EditorState.create({
			doc: initialDoc || undefined,
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

				// Serialize to markdown and update content on doc changes
				if (tr.docChanged && !isExternalUpdate) {
					const markdown = serializeMarkdown(newState.doc);
					content = markdown;

					// Only call onSave if not using Yjs
					if (!yjsDoc) {
						onSave?.(markdown);
					}
				}
			},
		});

		// For Yjs mode: if XmlFragment is empty but we have content prop, initialize the doc
		// This handles the case where a page has text content but no Yjs state yet
		if (yjsDoc && view) {
			let initialized = false;
			const unsubscribe = yjsDoc.isSynced.subscribe((synced) => {
				if (initialized) return; // Already handled
				if (
					synced &&
					view &&
					yjsDoc.yxmlFragment.length === 0 &&
					content &&
					content.trim()
				) {
					initialized = true;
					// XmlFragment is empty but we have content - initialize through ProseMirror
					// This will sync to Yjs via y-prosemirror
					const parsedDoc = parseMarkdown(content);
					if (parsedDoc) {
						isExternalUpdate = true;
						const tr = view.state.tr.replaceWith(
							0,
							view.state.doc.content.size,
							parsedDoc.content,
						);
						view.dispatch(tr);
						isExternalUpdate = false;
					}
					// Unsubscribe after initializing (use setTimeout to avoid sync call issue)
					setTimeout(() => unsubscribe(), 0);
				} else if (synced) {
					// Already synced with content or empty - just unsubscribe
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

		// Listen for AI edit events
		window.addEventListener("ai-edit-highlight", handleAIEditHighlight);
		window.addEventListener("ai-edit-accept", handleAIEditAccept);
		window.addEventListener("ai-edit-reject", handleAIEditReject);

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

	// Sync external content changes to editor (only when not using Yjs)
	$effect(() => {
		if (!yjsDoc && view) {
			const currentMarkdown = serializeMarkdown(view.state.doc);
			if (content !== currentMarkdown) {
				isExternalUpdate = true;
				const newDoc = parseMarkdown(content);
				if (newDoc) {
					const tr = view.state.tr.replaceWith(
						0,
						view.state.doc.content.size,
						newDoc.content,
					);
					view.dispatch(tr);
				}
				isExternalUpdate = false;
			}
		}
	});

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
		window.removeEventListener("ai-edit-highlight", handleAIEditHighlight);
		window.removeEventListener("ai-edit-accept", handleAIEditAccept);
		window.removeEventListener("ai-edit-reject", handleAIEditReject);
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

	{#if showLinkPopover}
		<LinkPopover
			position={linkPopoverPos}
			initialUrl={existingLinkUrl}
			onSubmit={handleLinkSubmit}
			onRemove={existingLinkUrl ? handleLinkRemove : undefined}
			onClose={closeLinkPopover}
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
