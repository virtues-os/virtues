<script lang="ts">
	import { onMount, onDestroy } from "svelte";
	import { goto } from "$app/navigation";
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
		wikiEditorTheme,
		wikiSyntaxHighlighting,
		livePreviewExtension,
	} from "./wiki-theme";
	import type { LinkedPage } from "$lib/wiki";

	interface Props {
		content: string;
		linkedPages?: LinkedPage[];
		onSave?: (content: string) => void;
		placeholder?: string;
	}

	let { content = $bindable(), linkedPages = [], onSave, placeholder: placeholderText }: Props = $props();

	let editorContainer: HTMLDivElement;
	let view: EditorView | null = null;
	let isExternalUpdate = false;

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
			wikiEditorTheme,
			wikiSyntaxHighlighting,

			// Live preview (Obsidian-style)
			livePreviewExtension,

			// Misc
			placeholder(placeholderText ?? "Start writing..."),
			EditorView.lineWrapping,

			// Sync content changes
			EditorView.updateListener.of((update) => {
				if (update.docChanged && !isExternalUpdate) {
					content = update.state.doc.toString();
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
			"wiki-navigate",
			handleNavigation as EventListener,
		);
	});

	// Handle navigation from wiki links and internal links
	function handleNavigation(e: CustomEvent<{ href: string }>) {
		e.preventDefault();
		e.stopPropagation();
		goto(e.detail.href);
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
			"wiki-navigate",
			handleNavigation as EventListener,
		);
		view?.destroy();
	});
</script>

<div class="wiki-editor" bind:this={editorContainer}></div>

<style>
	.wiki-editor {
		min-height: 300px;
	}

	/* Remove default outlines */
	.wiki-editor :global(.cm-editor) {
		outline: none;
	}

	.wiki-editor :global(.cm-focused) {
		outline: none;
	}

	/* Ensure proper font inheritance */
	.wiki-editor :global(.cm-editor),
	.wiki-editor :global(.cm-scroller),
	.wiki-editor :global(.cm-content),
	.wiki-editor :global(.cm-line) {
		font-family: var(
			--font-sans,
			ui-sans-serif,
			system-ui,
			-apple-system,
			sans-serif
		);
	}

	/* Heading lines get serif */
	.wiki-editor :global(.cm-heading-line) {
		font-family: var(--font-serif, Georgia, "Times New Roman", serif);
	}

	/* Citation highlight animation */
	.wiki-editor :global(.cm-citation-highlight) {
		animation: citation-pulse 1.5s ease-out;
	}

	@keyframes citation-pulse {
		0% {
			background-color: color-mix(
				in srgb,
				var(--color-primary) 30%,
				transparent
			);
		}
		100% {
			background-color: transparent;
		}
	}
</style>
