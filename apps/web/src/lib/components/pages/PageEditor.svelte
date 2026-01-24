<script lang="ts">
	import { onMount, onDestroy, tick } from "svelte";
	import { workspaceStore } from "$lib/stores/workspace.svelte";
	import "iconify-icon";
	import {
		EditorView,
		keymap,
		placeholder,
		drawSelection,
	} from "@codemirror/view";
	import { EditorState } from "@codemirror/state";
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

	interface Props {
		content: string;
		onSave?: (content: string) => void;
		placeholder?: string;
	}

	let { content = $bindable(), onSave, placeholder: placeholderText }: Props = $props();

	let editorContainer: HTMLDivElement;
	let view: EditorView | null = null;
	let isExternalUpdate = false;

	// Link picker state (for @ mentions - links to pages, people, places, files, etc.)
	let showLinkPicker = $state(false);
	let linkPickerQuery = $state("");
	let linkPickerResults = $state<Array<{ id: string; name: string; entity_type: string; icon: string }>>([]);
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

	function selectLink(item: { id: string; name: string }) {
		if (view && atSignPosition !== null) {
			const linkText = `[${item.name}](entity:${item.id})`;
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
			case "thing": return "ri:box-3-line";
			case "file": return "ri:file-line";
			case "page": return "ri:file-text-line";
			default: return "ri:links-line";
		}
	}

onMount(() => {
		const extensions = [
			// Core editing
			history(),
			keymap.of([...defaultKeymap, ...historyKeymap]),
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

			// Misc
			placeholder(placeholderText ?? "Start writing... Use @ to link pages, people, places, and files."),
			EditorView.lineWrapping,

			// Detect @ for entity picker
			EditorView.updateListener.of((update) => {
				if (update.docChanged && !isExternalUpdate) {
					content = update.state.doc.toString();
					onSave?.(content);

					// Check if @ was just typed
					update.changes.iterChanges((_fromA, _toA, fromB, _toB, inserted) => {
						const insertedText = inserted.toString();
						if (insertedText === "@") {
							// Get cursor position for popover placement
							const cursorPos = update.state.selection.main.head;
							const coords = view?.coordsAtPos(cursorPos);
							
							console.log("[link-picker] cursorPos:", cursorPos);
							console.log("[link-picker] coords:", coords);
							console.log("[link-picker] window size:", window.innerWidth, window.innerHeight);
							console.log("[link-picker] editorContainer rect:", editorContainer?.getBoundingClientRect());
							
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

		view = new EditorView({
			state: EditorState.create({
				doc: content,
				extensions,
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
		
		// Use windowTabs to open in split pane if available
		workspaceStore.openTabFromRoute(e.detail.href, {
			forceNew: true,
			preferEmptyPane: true,
		});
	}

	// Sync external content changes to editor
	$effect(() => {
		if (view && content !== view.state.doc.toString()) {
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

	onDestroy(() => {
		editorContainer?.removeEventListener(
			"page-navigate",
			handleNavigation as EventListener,
		);
		view?.destroy();
	});
</script>

<div class="page-editor-wrapper">
	<div class="page-editor" bind:this={editorContainer}></div>

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
				<iconify-icon icon="ri:search-line" width="16"></iconify-icon>
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
							<iconify-icon icon={getLinkIcon(item.entity_type)} width="16"></iconify-icon>
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

	:global(.link-picker-search iconify-icon) {
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

	:global(.link-picker-item iconify-icon) {
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
