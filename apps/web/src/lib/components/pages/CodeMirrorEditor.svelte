<script lang="ts">
	import { onMount, onDestroy } from "svelte";
	import { type EditorView } from "@codemirror/view";
	import { createCodeMirrorEditor } from "$lib/codemirror/editor";
	import type { YjsDocument } from "$lib/yjs";
	import { createEntityPicker, insertEntity } from "$lib/codemirror/extensions/entity-picker";
	import {
		createSlashCommands,
		getDefaultSlashCommands,
		filterSlashCommands,
		type SlashCommand,
	} from "$lib/codemirror/extensions/slash-commands";
	import { createSelectionToolbar } from "$lib/codemirror/extensions/selection-toolbar";
	import { createMediaPaste } from "$lib/codemirror/extensions/media-paste";
	import EntityPicker from "$lib/components/EntityPicker.svelte";
	import type { EntityResult } from "$lib/components/EntityPicker.svelte";
	import SlashMenu from "$lib/components/SlashMenu.svelte";
	import SelectionToolbar from "$lib/components/SelectionToolbar.svelte";

	// Import CodeMirror theme CSS
	import "$lib/codemirror/theme.css";

	export interface DocStats {
		wordCount: number;
		charCount: number;
		linkCount: number;
		mediaCount: number;
	}

	interface Props {
		/** Initial markdown content (used for non-Yjs init and Yjs empty-doc init) */
		initialContent?: string;
		/** Called when the document changes with computed stats */
		onDocChange?: (stats: DocStats) => void;
		placeholder?: string;
		/** Optional Yjs document for real-time collaboration */
		yjsDoc?: YjsDocument;
		/** Whether connected to the sync server */
		isConnected?: boolean;
		/** Whether synced with the server */
		isSynced?: boolean;
		/** Whether line numbers are shown */
		showDragHandles?: boolean;
		/** Page ID for filtering events */
		pageId?: string;
	}

	let {
		initialContent = "",
		onDocChange,
		placeholder: placeholderText,
		yjsDoc,
		isConnected = true,
		isSynced = true,
		showDragHandles = true,
		pageId,
	}: Props = $props();

	let editorContainer: HTMLDivElement;
	let view: EditorView | null = null;

	// --- Entity Picker state ---
	let entityPickerOpen = $state(false);
	let entityPickerPos = $state({ x: 0, y: 0 });
	let entityPickerFrom = $state(0);

	// --- Slash Menu state ---
	let slashMenuOpen = $state(false);
	let slashMenuPos = $state({ x: 0, y: 0 });
	let slashMenuQuery = $state("");
	let slashMenuFrom = $state(0);
	const allSlashCommands = getDefaultSlashCommands();
	const filteredCommands = $derived(filterSlashCommands(allSlashCommands, slashMenuQuery));

	// --- Selection Toolbar state ---
	let selToolbarOpen = $state(false);
	let selToolbarPos = $state({ x: 0, y: 0 });
	let selToolbarMarks = $state({
		strong: false,
		em: false,
		underline: false,
		code: false,
		strikethrough: false,
		link: false,
	});

	function computeStats(content: string): DocStats {
		const words = content.trim().split(/\s+/).filter(Boolean);
		const links = content.match(/\[([^\]]+)\]\(([^)]+)\)/g) || [];
		const media = content.match(/!\[([^\]]*)\]\(([^)]+)\)/g) || [];

		return {
			wordCount: words.length,
			charCount: content.length,
			linkCount: links.length,
			mediaCount: media.length,
		};
	}

	function handleDocChange(content: string) {
		if (onDocChange) {
			onDocChange(computeStats(content));
		}
	}

	// --- Entity Picker handlers ---
	function handleEntitySelect(entity: EntityResult) {
		if (!view) return;
		insertEntity(view, entityPickerFrom, entity.name, entity.url);
		entityPickerOpen = false;
	}

	function handleEntityClose() {
		entityPickerOpen = false;
		view?.focus();
	}

	// --- Slash Menu handlers ---
	function handleSlashSelect(cmd: SlashCommand) {
		if (!view) return;
		cmd.execute(view, slashMenuFrom);
		slashMenuOpen = false;
		view.focus();
	}

	function handleSlashClose() {
		slashMenuOpen = false;
		view?.focus();
	}

	// --- Selection Toolbar handlers ---
	type MarkType = "strong" | "em" | "underline" | "code" | "strikethrough" | "link";

	const FORMAT_WRAPPERS: Record<string, string> = {
		strong: "**",
		em: "*",
		code: "`",
		strikethrough: "~~",
	};

	function handleFormat(mark: MarkType) {
		if (!view) return;
		const { from, to } = view.state.selection.main;
		if (from === to) return;

		if (mark === "underline") {
			const sel = view.state.sliceDoc(from, to);
			if (sel.startsWith("<u>") && sel.endsWith("</u>")) {
				view.dispatch({
					changes: { from, to, insert: sel.slice(3, -4) },
				});
			} else {
				view.dispatch({
					changes: { from, to, insert: `<u>${sel}</u>` },
				});
			}
		} else if (mark === "link") {
			const sel = view.state.sliceDoc(from, to);
			view.dispatch({
				changes: { from, to, insert: `[${sel}](url)` },
				selection: { anchor: from + sel.length + 3, head: from + sel.length + 6 },
			});
		} else {
			const wrapper = FORMAT_WRAPPERS[mark];
			if (!wrapper) return;
			const sel = view.state.sliceDoc(from, to);
			const before = view.state.sliceDoc(Math.max(0, from - wrapper.length), from);
			const after = view.state.sliceDoc(to, Math.min(view.state.doc.length, to + wrapper.length));

			if (before === wrapper && after === wrapper) {
				// Already wrapped — remove
				view.dispatch({
					changes: [
						{ from: from - wrapper.length, to: from },
						{ from: to, to: to + wrapper.length },
					],
				});
			} else {
				// Wrap selection
				view.dispatch({
					changes: { from, to, insert: `${wrapper}${sel}${wrapper}` },
					selection: { anchor: from + wrapper.length, head: to + wrapper.length },
				});
			}
		}
		view.focus();
	}

	function handleSelToolbarClose() {
		selToolbarOpen = false;
	}

	// --- Media upload ---
	let fileInput: HTMLInputElement;

	async function uploadMedia(file: File): Promise<string> {
		const formData = new FormData();
		formData.append("file", file);
		const resp = await fetch("/api/media/upload", { method: "POST", body: formData });
		if (!resp.ok) throw new Error(`Upload failed: ${resp.statusText}`);
		const data = await resp.json();
		return data.url;
	}

	// --- Image slash command handler ---
	function handleImageFileSelect(e: Event) {
		const target = e.target as HTMLInputElement;
		const file = target.files?.[0];
		if (!file || !view) return;

		const pos = view.state.selection.main.head;
		const placeholder = `![Uploading ${file.name}...]()\n`;
		view.dispatch({
			changes: { from: pos, insert: placeholder },
		});

		uploadMedia(file)
			.then((url) => {
				if (!view) return;
				const doc = view.state.doc.toString();
				const pText = `![Uploading ${file.name}...]()`;
				const idx = doc.indexOf(pText);
				if (idx >= 0) {
					view.dispatch({
						changes: {
							from: idx,
							to: idx + pText.length,
							insert: `![${file.name}](${url})`,
						},
					});
				}
			})
			.catch((err) => {
				console.error("Image upload failed:", err);
				if (!view) return;
				const doc = view.state.doc.toString();
				const pText = `![Uploading ${file.name}...]()\n`;
				const idx = doc.indexOf(pText);
				if (idx >= 0) {
					view.dispatch({
						changes: { from: idx, to: idx + pText.length, insert: "" },
					});
				}
			});

		target.value = "";
	}

	onMount(() => {
		if (!yjsDoc) return;

		// Create interactive extensions with callbacks
		const entityPickerExt = createEntityPicker({
			onOpen: (coords, from) => {
				entityPickerFrom = from;
				entityPickerPos = coords;
				entityPickerOpen = true;
			},
			onClose: () => {
				entityPickerOpen = false;
			},
			onQueryChange: () => {
				// EntityPicker component has its own search input, so we just need to keep it open
			},
		});

		const slashCommandsExt = createSlashCommands({
			onOpen: (coords, from) => {
				slashMenuFrom = from;
				slashMenuPos = coords;
				slashMenuQuery = "";
				slashMenuOpen = true;
			},
			onClose: () => {
				slashMenuOpen = false;
			},
			onQueryChange: (query) => {
				slashMenuQuery = query;
			},
		});

		const selectionToolbarExt = createSelectionToolbar({
			onShow: (coords, activeFormats) => {
				selToolbarPos = coords;
				selToolbarMarks = {
					strong: activeFormats.has("bold"),
					em: activeFormats.has("italic"),
					underline: activeFormats.has("underline"),
					code: activeFormats.has("code"),
					strikethrough: activeFormats.has("strikethrough"),
					link: false,
				};
				selToolbarOpen = true;
			},
			onHide: () => {
				selToolbarOpen = false;
			},
		});

		const mediaPasteExt = createMediaPaste(uploadMedia);

		view = createCodeMirrorEditor({
			parent: editorContainer,
			ytext: yjsDoc.ytext,
			awareness: yjsDoc.provider.awareness,
			readOnly: false,
			placeholder: placeholderText || "Type / for commands, @ for entities...",
			showLineNumbers: showDragHandles,
			onDocChange: handleDocChange,
			extensions: [
				entityPickerExt,
				slashCommandsExt,
				selectionToolbarExt,
				mediaPasteExt,
			],
		});

		// Listen for /image slash command
		const handleImageCommand = () => {
			fileInput?.click();
		};
		editorContainer.addEventListener("slash-command-image", handleImageCommand);

		// Compute initial stats
		handleDocChange(yjsDoc.ytext.toString());
	});

	onDestroy(() => {
		view?.destroy();
		view = null;
	});
</script>

<input
	bind:this={fileInput}
	type="file"
	accept="*/*"
	onchange={handleImageFileSelect}
	style="display: none"
/>

<div class="page-editor-wrapper">
	<div
		class="page-editor cm-editor-container"
		bind:this={editorContainer}
	></div>

	<!-- Floating UI overlays -->
	{#if entityPickerOpen}
		<EntityPicker
			position={entityPickerPos}
			onSelect={handleEntitySelect}
			onClose={handleEntityClose}
		/>
	{/if}

	{#if slashMenuOpen}
		<SlashMenu
			query={slashMenuQuery}
			commands={filteredCommands}
			position={slashMenuPos}
			onSelect={handleSlashSelect}
			onClose={handleSlashClose}
		/>
	{/if}

	{#if selToolbarOpen}
		<SelectionToolbar
			position={selToolbarPos}
			activeMarks={selToolbarMarks}
			onFormat={handleFormat}
			onClose={handleSelToolbarClose}
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

</style>
