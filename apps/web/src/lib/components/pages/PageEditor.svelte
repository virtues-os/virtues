<script lang="ts">
	import { onMount, onDestroy, tick } from "svelte";
	import { spaceStore } from "$lib/stores/space.svelte";
	import Icon from "$lib/components/Icon.svelte";
	import {
		EditorView,
		keymap,
		placeholder,
		drawSelection,
	} from "@codemirror/view";
	import { EditorState, Compartment } from "@codemirror/state";
	import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
	import {
		defaultKeymap,
		history,
		historyKeymap,
	} from "@codemirror/commands";
	import { languages } from "@codemirror/language-data";
	import {
		pageEditorTheme,
		pageSyntaxHighlighting,
		livePreviewExtension,
	} from "./page-theme";
	import { criticMarkupExtension, type YjsDocument } from "$lib/yjs";

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
	}

	let { content = $bindable(), onSave, placeholder: placeholderText, yjsDoc, disabled = false, isConnected = true, isSynced = true }: Props = $props();

	let editorContainer: HTMLDivElement;
	let view: EditorView | null = null;
	let isExternalUpdate = false;
	
	// Compartment for dynamically updating editable state
	const editableCompartment = new Compartment();

	// Link picker state (for @ mentions - links to pages, people, places, files, etc.)
	let showLinkPicker = $state(false);
	let linkPickerQuery = $state("");
	let linkPickerResults = $state<Array<{ id: string; name: string; entity_type: string; icon: string; url: string; mime_type?: string }>>([]);
	let linkPickerSelectedIndex = $state(0);
	let linkPickerPos = $state({ x: 0, y: 0 });
	let atSignPosition = $state<number | null>(null);
	let searchInputEl: HTMLInputElement | null = $state(null);
	// Portal action to move element to document.body
	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return {
			destroy() {
				node.remove();
			}
		};
	}

	// Fetch results when query changes
	$effect(() => {
		if (showLinkPicker) {
			fetchLinkResults(linkPickerQuery);
		}
	});

	async function fetchLinkResults(query: string) {
		try {
			const response = await fetch(`/api/pages/search/entities?q=${encodeURIComponent(query)}`);
			if (response.ok) {
				const data = await response.json();
				linkPickerResults = data.results || [];
				linkPickerSelectedIndex = 0;
			}
		} catch (e) {
			console.error("Link picker fetch error:", e);
		}
	}

	function openLinkPicker(pos: { x: number; y: number }, cursorPos: number) {
		// Clamp position to viewport bounds
		const pickerWidth = 300;
		const pickerHeight = 340;
		const padding = 8;

		let x = pos.x;
		let y = pos.y;

		// Keep within horizontal bounds
		if (x + pickerWidth > window.innerWidth - padding) {
			x = window.innerWidth - pickerWidth - padding;
		}
		if (x < padding) {
			x = padding;
		}

		// Keep within vertical bounds - if it would go below, show above the cursor
		if (y + pickerHeight > window.innerHeight - padding) {
			// Position above the @ sign instead
			y = pos.y - pickerHeight - 24; // 24 accounts for line height
		}

		console.log("[link-picker] final position:", { x, y });
		linkPickerPos = { x, y };
		atSignPosition = cursorPos;
		linkPickerQuery = "";
		linkPickerResults = [];
		linkPickerSelectedIndex = 0;
		showLinkPicker = true;
		// Focus the search input after render
		setTimeout(() => searchInputEl?.focus(), 0);
	}

	function closeLinkPicker() {
		showLinkPicker = false;
		atSignPosition = null;
		view?.focus();
	}

	// Check if a file is a media type (image, audio, video) based on extension or mime type
	function isMediaFile(name: string, mimeType?: string): { isMedia: boolean; type: 'image' | 'audio' | 'video' | null } {
		const ext = name.split('.').pop()?.toLowerCase() || '';

		// Check by extension first
		const imageExts = ['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico'];
		const audioExts = ['mp3', 'wav', 'm4a', 'ogg', 'flac', 'aac', 'wma'];
		const videoExts = ['mp4', 'mov', 'webm', 'avi', 'mkv', 'm4v', 'wmv'];

		if (imageExts.includes(ext)) return { isMedia: true, type: 'image' };
		if (audioExts.includes(ext)) return { isMedia: true, type: 'audio' };
		if (videoExts.includes(ext)) return { isMedia: true, type: 'video' };

		// Fallback to mime type
		if (mimeType) {
			if (mimeType.startsWith('image/')) return { isMedia: true, type: 'image' };
			if (mimeType.startsWith('audio/')) return { isMedia: true, type: 'audio' };
			if (mimeType.startsWith('video/')) return { isMedia: true, type: 'video' };
		}

		return { isMedia: false, type: null };
	}

	function selectLink(item: { id: string; name: string; url: string; entity_type: string; mime_type?: string }) {
		if (view && atSignPosition !== null) {
			let linkText: string;

			// For files, check if it's a media type and use image syntax
			if (item.entity_type === 'file') {
				const { isMedia } = isMediaFile(item.name, item.mime_type);
				if (isMedia) {
					// Use image syntax for media files (extension in name determines rendering)
					linkText = `![${item.name}](${item.url})`;
				} else {
					// Regular link for non-media files
					linkText = `[${item.name}](${item.url})`;
				}
			} else {
				// Regular link for entities (person, place, page, etc.)
				linkText = `[${item.name}](${item.url})`;
			}

			// Replace the @ with the link
			view.dispatch({
				changes: {
					from: atSignPosition,
					to: atSignPosition + 1, // Remove the @
					insert: linkText,
				},
			});
		}
		closeLinkPicker();
	}

	function handlePickerKeydown(e: KeyboardEvent) {
		if (e.key === "Escape") {
			e.preventDefault();
			closeLinkPicker();
		} else if (e.key === "ArrowDown") {
			e.preventDefault();
			linkPickerSelectedIndex = Math.min(linkPickerSelectedIndex + 1, linkPickerResults.length - 1);
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			linkPickerSelectedIndex = Math.max(linkPickerSelectedIndex - 1, 0);
		} else if (e.key === "Enter" || e.key === "Tab") {
			e.preventDefault();
			if (linkPickerResults[linkPickerSelectedIndex]) {
				selectLink(linkPickerResults[linkPickerSelectedIndex]);
			}
		}
	}

	function getLinkIcon(type: string): string {
		switch (type) {
			case "person": return "ri:user-line";
			case "place": return "ri:map-pin-line";
			case "file": return "ri:file-line";
			case "page": return "ri:file-text-line";
			default: return "ri:links-line";
		}
	}

onMount(() => {
		// Build extensions based on whether Yjs is enabled
		const baseExtensions = [
			// Core editing (use Yjs extensions if available, otherwise CodeMirror history)
			...(yjsDoc 
				? yjsDoc.extensions 
				: [history(), keymap.of([...defaultKeymap, ...historyKeymap])]
			),
			drawSelection(),

			// Markdown language support with code highlighting
			markdown({
				base: markdownLanguage,
				codeLanguages: languages,
			}),

			// Custom theme and styling
			pageEditorTheme,
			pageSyntaxHighlighting,

			// Live preview (Obsidian-style)
			livePreviewExtension,

			// CriticMarkup for AI-proposed edits (inline accept/reject)
			...criticMarkupExtension({
				onAccept: (type, content) => {
					console.log('[CriticMarkup] Accepted:', type, content.slice(0, 50));
				},
				onReject: (type, content) => {
					console.log('[CriticMarkup] Rejected:', type, content.slice(0, 50));
				}
			}),

			// Misc
			placeholder(placeholderText ?? "Start writing... Use @ to link pages, people, places, and files."),
			EditorView.lineWrapping,
			// Disable editing when disabled prop is true (e.g., waiting for Yjs sync)
			// Use compartment so we can update it dynamically when disabled changes
			editableCompartment.of(EditorView.editable.of(!disabled)),

			// Detect @ for entity picker + content sync
			EditorView.updateListener.of((update) => {
				if (update.docChanged && !isExternalUpdate) {
					content = update.state.doc.toString();
					// Only call onSave if not using Yjs (Yjs handles persistence via WebSocket)
					if (!yjsDoc) {
						onSave?.(content);
					}

					// Check if @ was just typed
					update.changes.iterChanges((_fromA, _toA, fromB, _toB, inserted) => {
						const insertedText = inserted.toString();
						if (insertedText === "@") {
							// Get cursor position for popover placement
							const cursorPos = update.state.selection.main.head;
							const coords = view?.coordsAtPos(cursorPos);
							
							if (coords) {
								openLinkPicker(
									{ x: coords.left, y: coords.bottom + 6 },
									fromB
								);
							}
						}
					});
				}
			}),
		];

		// For Yjs mode, content comes from ytext (synced via WebSocket)
		// For non-Yjs mode, use the content prop
		const initialDoc = yjsDoc ? yjsDoc.ytext.toString() : content;

		view = new EditorView({
			state: EditorState.create({
				doc: initialDoc,
				extensions: baseExtensions,
			}),
			parent: editorContainer,
		});

		// Listen for custom navigation events from widgets
		editorContainer.addEventListener(
			"page-navigate",
			handleNavigation as EventListener,
		);
	});

	// Handle navigation from entity links
	function handleNavigation(e: CustomEvent<{ href: string; entityId?: string }>) {
		e.preventDefault();
		e.stopPropagation();
		
		// Open in split pane if available
		spaceStore.openTabFromRoute(e.detail.href, {
			forceNew: true,
			preferEmptyPane: true,
		});
	}

	// Sync external content changes to editor (only when not using Yjs)
	// Yjs handles sync automatically via the y-codemirror extension
	$effect(() => {
		if (!yjsDoc && view && content !== view.state.doc.toString()) {
			isExternalUpdate = true;
			view.dispatch({
				changes: {
					from: 0,
					to: view.state.doc.length,
					insert: content,
				},
			});
			isExternalUpdate = false;
		}
	});

	// Update editable state when disabled prop changes
	// This reconfigures the compartment so CodeMirror respects the new state
	$effect(() => {
		if (view) {
			view.dispatch({
				effects: editableCompartment.reconfigure(EditorView.editable.of(!disabled))
			});
		}
	});

	onDestroy(() => {
		editorContainer?.removeEventListener(
			"page-navigate",
			handleNavigation as EventListener,
		);
		view?.destroy();
	});
</script>

<div class="page-editor-wrapper">
	{#if disabled}
		<div class="sync-status syncing">
			<Icon icon="ri:loader-4-line" width="12" class="animate-spin" />
			<span>Connecting...</span>
		</div>
	{:else if yjsDoc}
		{#if !isConnected}
			<div class="sync-status offline">
				<Icon icon="ri:wifi-off-line" width="12" />
				<span>Offline - changes will sync when reconnected</span>
			</div>
		{:else if !isSynced}
			<div class="sync-status syncing">
				<Icon icon="ri:loader-4-line" width="12" class="animate-spin" />
				<span>Syncing...</span>
			</div>
		{/if}
	{/if}
	<div class="page-editor" class:disabled bind:this={editorContainer}></div>

	{#if showLinkPicker}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<div class="link-picker-backdrop" use:portal onclick={closeLinkPicker} onkeydown={(e) => e.key === 'Escape' && closeLinkPicker()} role="button" tabindex="-1"></div>
		<div 
			class="link-picker"
			use:portal
			style="left: {linkPickerPos.x}px; top: {linkPickerPos.y}px;"
		>
			<div class="link-picker-search">
				<Icon icon="ri:search-line" width="16"/>
				<input
					bind:this={searchInputEl}
					type="text"
					placeholder="Search pages, people, places..."
					bind:value={linkPickerQuery}
					onkeydown={handlePickerKeydown}
				/>
			</div>
			<div class="link-picker-results">
				{#if linkPickerResults.length === 0}
					<div class="link-picker-empty">
						{linkPickerQuery ? "No results found" : "Type to search..."}
					</div>
				{:else}
					{#each linkPickerResults as item, i}
						<button
							class="link-picker-item"
							class:selected={i === linkPickerSelectedIndex}
							onclick={() => selectLink(item)}
							onmouseenter={() => linkPickerSelectedIndex = i}
						>
							<Icon icon={getLinkIcon(item.entity_type)} width="16"/>
							<span class="link-name">{item.name}</span>
							<span class="link-type">{item.entity_type}</span>
						</button>
					{/each}
				{/if}
			</div>
		</div>
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

	/* Sync status indicator */
	.sync-status {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		font-size: 12px;
		border-radius: 4px;
		margin-bottom: 8px;
	}

	.sync-status.offline {
		background: var(--color-warning-subtle);
		color: var(--color-warning);
	}

	.sync-status.syncing {
		background: var(--color-primary-subtle);
		color: var(--color-primary);
	}

	:global(.animate-spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	/* Remove default outlines */
	.page-editor :global(.cm-editor) {
		outline: none;
	}

	.page-editor :global(.cm-focused) {
		outline: none;
	}

	/* Ensure proper font inheritance */
	.page-editor :global(.cm-editor),
	.page-editor :global(.cm-scroller),
	.page-editor :global(.cm-content),
	.page-editor :global(.cm-line) {
		font-family: var(
			--font-sans,
			ui-sans-serif,
			system-ui,
			-apple-system,
			sans-serif
		);
	}

	/* Heading lines get serif */
	.page-editor :global(.cm-heading-line) {
		font-family: var(--font-serif, Georgia, "Times New Roman", serif);
	}

	/* Link picker popover (@ mentions) - using :global for fixed positioning */
	:global(.link-picker-backdrop) {
		position: fixed;
		inset: 0;
		z-index: 999;
	}

	:global(.link-picker) {
		position: fixed;
		z-index: 1000;
		background: var(--color-surface);
		border: 1px solid var(--color-border);
		border-radius: 8px;
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.2);
		width: 300px;
		max-height: 340px;
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}

	:global(.link-picker-search) {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px;
		border-bottom: 1px solid var(--color-border);
		background: var(--color-surface-elevated);
	}

	:global(.link-picker-search :global(svg)) {
		color: var(--color-foreground-muted);
	}

	:global(.link-picker-search input) {
		flex: 1;
		border: none;
		background: none;
		font-size: 14px;
		outline: none;
		color: var(--color-foreground);
	}

	:global(.link-picker-search input::placeholder) {
		color: var(--color-foreground-subtle);
	}

	:global(.link-picker-results) {
		flex: 1;
		overflow-y: auto;
		padding: 4px;
	}

	:global(.link-picker-empty) {
		padding: 20px;
		text-align: center;
		color: var(--color-foreground-muted);
		font-size: 13px;
	}

	:global(.link-picker-item) {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 10px 12px;
		border: none;
		background: none;
		border-radius: 6px;
		cursor: pointer;
		text-align: left;
		font-size: 14px;
		color: var(--color-foreground);
		transition: background-color 0.1s ease;
	}

	:global(.link-picker-item:hover),
	:global(.link-picker-item.selected) {
		background: var(--color-primary-subtle);
	}

	:global(.link-picker-item :global(svg)) {
		color: var(--color-foreground-muted);
	}

	:global(.link-picker-item .link-name) {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	:global(.link-picker-item .link-type) {
		font-size: 11px;
		color: var(--color-foreground-subtle);
		text-transform: capitalize;
		padding: 2px 6px;
		background: var(--color-surface-elevated);
		border-radius: 4px;
	}
</style>
